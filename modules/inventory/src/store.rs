//! Persistence and posting. Every mutation runs inside [`datum_db::Tx`].
//! Postings go through one [`datum_ledger::GroupBuilder`] per transaction.

use chrono::Utc;
use datum_core::{
    AnyQuantity, AreaDim, Boundary, ConversionContext, CostElement, CountDim, DimensionKind,
    GroupKind, Identifier, ItemId, LengthDim, LocationId, LotId, MassDim, Money, NoPostings,
    PostingGroupHeader, PostingIntent, PostingSink, QuantityPosting, TimeDim, ValueAccount,
    ValuePosting, VolumeDim,
};
use datum_db::Tx;
use datum_ledger::{BalanceSlice, GroupBuilder, load_open_layers, load_stock_item};
use datum_mod_locations::{LocationKind, boundary_location_id};
use datum_mod_lots::{LotStatus, PackageId, StatusTarget, load_lot, package_hierarchy, set_status};
use datum_module::Kernel;
use datum_statemachine::DocRef;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::body_hash::sha256_hex;
use crate::domain::{
    AdjustRequest, BalanceQuery, CountRequest, DOC_TYPE, Document, DocumentKind, DocumentLine,
    DocumentStatus, IssueRequest, LineInput, MoveRequest, ReceiveRequest, ReleaseRequest,
    ReturnRequest, ShipRequest,
};
use crate::error::{Error, Result};
use crate::posting_path::{cover_layers, post_via_transition};
use crate::{events, stamps};

type DocRow = (
    Uuid,
    String,
    String,
    Option<String>,
    Option<Uuid>,
    i64,
    String,
    String,
);

type LineRow = (
    Uuid,
    Uuid,
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Decimal,
    i64,
    String,
    Decimal,
    i64,
    String,
    Decimal,
    Option<String>,
    Option<Uuid>,
);

/// Receive stock from SUPPLIER into `to_location` (D2 case a).
pub async fn receive(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: ReceiveRequest,
) -> Result<Document> {
    let body_hash = hash_receive(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    let supplier = boundary_location_id(tx, Boundary::Supplier).await?;
    let mut prepared = Vec::new();
    let mut received_sum = Decimal::ZERO;
    for mut line in req.lines {
        line.from_location = Some(supplier);
        line.to_location = Some(req.to_location);
        let p = prepare_line(tx, kernel, line).await?;
        received_sum += p.canonical.amount.abs();
        prepared.push(p);
    }
    if let (Some(expected), Some(tol)) = (req.expected, req.tolerance) {
        let item = prepared
            .first()
            .ok_or_else(|| Error::Document("receipt has no lines".into()))?
            .item;
        let conv = convert_entered(tx, kernel, item, None, expected).await?;
        if received_sum > conv.0.amount + tol {
            return Err(Error::Document(
                "over-receipt exceeds source-document tolerance".into(),
            ));
        }
    }
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Receipt,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    let mut builder = movement_builder("inventory.receive", Some(doc_id), None, None);
    for p in &prepared {
        contribute_receive(&mut builder, p, supplier, req.to_location)?;
    }
    let group_id = post_via_transition(kernel, tx, ctx, doc_id, DocumentKind::Receipt, builder)
        .await?
        .expect("receive posts");
    stamp_posted(tx, doc_id, Some(group_id)).await?;
    for p in &prepared {
        if let Some(lot) = p.lot {
            kernel
                .publish_event(
                    tx,
                    events::lot_received(lot, p.item, p.canonical.amount, doc_id)?,
                )
                .await?;
        } else {
            kernel
                .publish_event(
                    tx,
                    events::receipt_posted(
                        p.item,
                        req.to_location,
                        p.canonical.amount,
                        p.canonical.unit.0,
                        group_id,
                        doc_id,
                    )?,
                )
                .await?;
        }
    }
    load_document(tx, doc_id).await
}

/// MOVEMENT Q→A plus `datum_mod_lots::set_status` (D2 case b; PLAN §3 item 4).
pub async fn release_from_quarantine(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: ReleaseRequest,
) -> Result<Document> {
    let body_hash = hash_release(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    let lot_rec = load_lot(tx, req.lot).await?;
    let line = LineInput {
        item: lot_rec.item,
        entered: req.entered,
        lot: Some(req.lot),
        serial: None,
        from_location: Some(req.from_location),
        to_location: Some(req.to_location),
        package: None,
        amount: req.amount,
        reason_code: None,
    };
    let p = prepare_line(tx, kernel, line).await?;
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Move,
        Some("release_from_quarantine".into()),
        std::slice::from_ref(&p),
    )
    .await?;
    let mut builder = movement_builder("inventory.release", Some(doc_id), None, None);
    contribute_move(&mut builder, tx, &p, true).await?;
    let group_id = datum_ledger::post(tx, builder).await?;
    // `datum_mod_lots::set_status` drives the lot machine (`Kernel::transition`),
    // which requires bound action `lot.release`. One Tx can bind one action, so
    // this path stamps the inventory document posted without a second transition
    // (`inventory.move` would ActionMismatch).
    set_status(
        tx,
        kernel,
        ctx.actor,
        StatusTarget::Lot(req.lot),
        LotStatus::Available,
        "released from quarantine",
    )
    .await?;
    stamp_posted(tx, doc_id, Some(group_id)).await?;
    load_document(tx, doc_id).await
}

/// Issue to WIP-`<wo>` (D2 case c). Named lots contribute explicit Consumption.
pub async fn issue_to_wip(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: IssueRequest,
) -> Result<Document> {
    let body_hash = hash_issue(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    let wip = datum_mod_locations::ensure_wip(tx, req.work_order).await?;
    let mut prepared = Vec::new();
    let mut residuals = Vec::new();
    for mut line in req.lines {
        if let Some(lot) = line.lot {
            let rec = load_lot(tx, lot).await?;
            if rec.status != LotStatus::Available {
                return Err(Error::LotNotIssuable);
            }
        }
        line.from_location = Some(req.from_location);
        line.to_location = Some(wip);
        let (p, res) = prepare_line_with_residual(tx, kernel, line).await?;
        if !res.amount.is_zero() {
            residuals.push((p.item, p.lot, res));
        }
        prepared.push(p);
    }
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Issue,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    let mut builder = movement_builder("inventory.issue", Some(doc_id), Some(req.work_order), None);
    for p in &prepared {
        contribute_issue(&mut builder, tx, p, req.work_order).await?;
    }
    let movement_group = post_via_transition(kernel, tx, ctx, doc_id, DocumentKind::Issue, builder)
        .await?
        .expect("issue posts");
    stamp_posted(tx, doc_id, Some(movement_group)).await?;
    post_uom_residuals(
        tx,
        kernel,
        ctx,
        doc_id,
        req.from_location,
        movement_group,
        &residuals,
    )
    .await?;
    for p in &prepared {
        kernel
            .publish_event(
                tx,
                events::issued(
                    p.item,
                    p.canonical.amount,
                    p.lot,
                    Some(req.work_order),
                    doc_id,
                )?,
            )
            .await?;
    }
    load_document(tx, doc_id).await
}

/// Move stock between real locations.
pub async fn move_stock(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: MoveRequest,
) -> Result<Document> {
    let body_hash = hash_move(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    let mut prepared = Vec::new();
    for mut line in req.lines {
        line.from_location = Some(req.from_location);
        line.to_location = Some(req.to_location);
        prepared.push(prepare_line(tx, kernel, line).await?);
    }
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Move,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    let mut builder = movement_builder("inventory.move", Some(doc_id), None, None);
    for p in &prepared {
        contribute_move(&mut builder, tx, p, p.lot.is_some() || p.serial.is_some()).await?;
    }
    let group_id = post_via_transition(kernel, tx, ctx, doc_id, DocumentKind::Move, builder)
        .await?
        .expect("move posts");
    stamp_posted(tx, doc_id, Some(group_id)).await?;
    load_document(tx, doc_id).await
}

/// ADJUSTMENT with a required reason (D2 cases e, f, k).
pub async fn adjust(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: AdjustRequest,
) -> Result<Document> {
    let body_hash = hash_adjust(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    if req.reason.trim().is_empty() {
        return Err(Error::ReasonRequired);
    }
    let counterpart = counterpart_for_reason(tx, &req.reason).await?;
    let mut prepared = Vec::new();
    for mut line in req.lines {
        line.from_location = Some(req.location);
        line.to_location = Some(counterpart.0);
        line.reason_code = Some(req.reason.clone());
        prepared.push(prepare_line(tx, kernel, line).await?);
    }
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Adjustment,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    let mut builder = GroupBuilder::new(
        GroupKind::Adjustment,
        PostingGroupHeader {
            source_kind: "inventory.adjust".into(),
            source_id: Some(doc_id),
            work_order_id: None,
            reason_code: Some(req.reason.clone()),
            reverses_group_id: None,
        },
    );
    for p in &prepared {
        contribute_adjustment(&mut builder, tx, p, counterpart.1).await?;
    }
    let group_id = post_via_transition(kernel, tx, ctx, doc_id, DocumentKind::Adjustment, builder)
        .await?
        .expect("adjust posts");
    stamp_posted(tx, doc_id, Some(group_id)).await?;
    for p in &prepared {
        kernel
            .publish_event(
                tx,
                events::adjusted(p.item, p.canonical.amount, &req.reason, doc_id)?,
            )
            .await?;
    }
    load_document(tx, doc_id).await
}

/// Cycle count: variance is an ADJUSTMENT; tolerance is against the source document.
pub async fn cycle_count(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: CountRequest,
) -> Result<Document> {
    let body_hash = hash_count(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    let mut prepared = Vec::new();
    for line in req.lines {
        let (counted, factor, _) =
            convert_entered(tx, kernel, line.item, line.lot, line.counted).await?;
        let (expected, _, _) =
            convert_entered(tx, kernel, line.item, line.lot, line.expected).await?;
        if (counted.amount - expected.amount).abs() > req.tolerance {
            return Err(Error::Document(
                "cycle count variance exceeds source-document tolerance".into(),
            ));
        }
        let system = on_hand(
            tx,
            BalanceQuery {
                item: line.item,
                location: Some(req.location),
                lot: line.lot,
            },
        )
        .await?;
        let variance = counted.amount - system;
        if variance.is_zero() {
            continue;
        }
        let reason = if variance.is_sign_negative() {
            "CYCLE_COUNT_SHORT"
        } else {
            "CYCLE_COUNT_OVER"
        };
        let layers = load_open_layers(tx, line.item, req.location).await?;
        let (money, _) = cover_layers(&layers, variance.abs(), line.lot, line.serial)?;
        prepared.push(PreparedLine {
            item: line.item,
            lot: line.lot,
            serial: line.serial,
            from_location: Some(req.location),
            to_location: Some(boundary_location_id(tx, Boundary::Adjustment).await?),
            entered: line.counted,
            canonical: AnyQuantity {
                amount: variance,
                unit: counted.unit,
                dimension: counted.dimension,
            },
            conversion_factor: factor,
            amount: Some(money),
            reason_code: Some(reason.into()),
            package: None,
        });
    }
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Count,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    if prepared.is_empty() {
        let doc = DocRef {
            doc_type: DOC_TYPE.into(),
            doc_id,
        };
        kernel
            .engine
            .transition(
                tx,
                Box::new(NoPostings),
                &doc,
                DocumentKind::Count.post_edge(),
                None,
                kernel.signature_gate(),
                ctx,
            )
            .await?;
        stamp_posted(tx, doc_id, None).await?;
        return load_document(tx, doc_id).await;
    }
    let header_reason = prepared[0]
        .reason_code
        .clone()
        .unwrap_or_else(|| "CYCLE_COUNT_SHORT".into());
    let mut builder = GroupBuilder::new(
        GroupKind::Adjustment,
        PostingGroupHeader {
            source_kind: "inventory.count".into(),
            source_id: Some(doc_id),
            work_order_id: None,
            reason_code: Some(header_reason.clone()),
            reverses_group_id: None,
        },
    );
    for p in &prepared {
        contribute_adjustment(&mut builder, tx, p, Boundary::Adjustment).await?;
    }
    let group_id = post_via_transition(kernel, tx, ctx, doc_id, DocumentKind::Count, builder)
        .await?
        .expect("count posts");
    stamp_posted(tx, doc_id, Some(group_id)).await?;
    for p in &prepared {
        kernel
            .publish_event(
                tx,
                events::adjusted(
                    p.item,
                    p.canonical.amount,
                    p.reason_code.as_deref().unwrap_or(&header_reason),
                    doc_id,
                )?,
            )
            .await?;
    }
    load_document(tx, doc_id).await
}

/// Ship to CUSTOMER with COGS value rows (D2 case g).
pub async fn ship_to_customer(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: ShipRequest,
) -> Result<Document> {
    let body_hash = hash_ship(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    let customer = boundary_location_id(tx, Boundary::Customer).await?;
    let mut prepared = Vec::new();
    for mut line in req.lines {
        line.from_location = Some(req.from_location);
        line.to_location = Some(customer);
        prepared.push(prepare_line(tx, kernel, line).await?);
    }
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Issue,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    // `work_order_id` on the header is the sales-order cost object (D2 case g;
    // ledger FK `(group_id, cost_object_id) → posting_group.work_order_id`).
    let mut builder = movement_builder("inventory.ship", Some(doc_id), Some(req.order), None);
    for p in &prepared {
        contribute_ship(&mut builder, tx, p, customer, req.order).await?;
    }
    let group_id = post_via_transition(kernel, tx, ctx, doc_id, DocumentKind::Issue, builder)
        .await?
        .expect("ship posts");
    stamp_posted(tx, doc_id, Some(group_id)).await?;
    load_document(tx, doc_id).await
}

/// Customer return into quarantine (D2 case h).
pub async fn customer_return(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: ReturnRequest,
) -> Result<Document> {
    let body_hash = hash_return(&req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        return load_document(tx, existing).await;
    }
    let customer = boundary_location_id(tx, Boundary::Customer).await?;
    let mut prepared = Vec::new();
    for mut line in req.lines {
        line.from_location = Some(customer);
        line.to_location = Some(req.to_location);
        prepared.push(prepare_line(tx, kernel, line).await?);
    }
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Receipt,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    let mut builder = movement_builder("inventory.return", Some(doc_id), None, None);
    for p in &prepared {
        contribute_return(&mut builder, p, customer, req.to_location)?;
    }
    let group_id = post_via_transition(kernel, tx, ctx, doc_id, DocumentKind::Receipt, builder)
        .await?
        .expect("return posts");
    stamp_posted(tx, doc_id, Some(group_id)).await?;
    load_document(tx, doc_id).await
}

/// On-hand as the ledger fold (`datum_ledger::balance_at`). No stored balance.
pub async fn on_hand(tx: &mut Tx<'_>, query: BalanceQuery) -> Result<Decimal> {
    let stock = load_stock_item(tx, query.item).await?;
    let instant = Utc::now();
    if let Some(location) = query.location {
        return Ok(datum_ledger::balance_at(
            tx,
            BalanceSlice {
                item: query.item,
                location,
                lot: query.lot,
                serial: None,
                unit: stock.stock_uom,
            },
            instant,
        )
        .await?);
    }
    let mut total = Decimal::ZERO;
    for loc in datum_mod_locations::list_flat(tx).await? {
        if loc.boundary_class.is_some() {
            continue;
        }
        total += datum_ledger::balance_at(
            tx,
            BalanceSlice {
                item: query.item,
                location: loc.id,
                lot: query.lot,
                serial: None,
                unit: stock.stock_uom,
            },
            instant,
        )
        .await?;
    }
    Ok(total)
}

/// Quantity at WIP locations (allocated to work orders).
pub async fn allocated(tx: &mut Tx<'_>, query: BalanceQuery) -> Result<Decimal> {
    let stock = load_stock_item(tx, query.item).await?;
    let instant = Utc::now();
    let mut total = Decimal::ZERO;
    for loc in datum_mod_locations::list_flat(tx).await? {
        if loc.kind != LocationKind::Wip {
            continue;
        }
        if let Some(only) = query.location
            && only != loc.id
        {
            continue;
        }
        total += datum_ledger::balance_at(
            tx,
            BalanceSlice {
                item: query.item,
                location: loc.id,
                lot: query.lot,
                serial: None,
                unit: stock.stock_uom,
            },
            instant,
        )
        .await?;
    }
    Ok(total)
}

/// Available to a work order: on-hand of an available lot at a non-WIP real location.
pub async fn available(tx: &mut Tx<'_>, query: BalanceQuery) -> Result<Decimal> {
    if let Some(lot) = query.lot {
        let rec = load_lot(tx, lot).await?;
        if rec.status != LotStatus::Available {
            return Ok(Decimal::ZERO);
        }
        let stock = load_stock_item(tx, query.item).await?;
        let instant = Utc::now();
        if let Some(location) = query.location {
            return Ok(datum_ledger::balance_at(
                tx,
                BalanceSlice {
                    item: query.item,
                    location,
                    lot: Some(lot),
                    serial: None,
                    unit: stock.stock_uom,
                },
                instant,
            )
            .await?);
        }
        let mut total = Decimal::ZERO;
        for loc in datum_mod_locations::list_flat(tx).await? {
            if loc.boundary_class.is_some() || loc.kind == LocationKind::Wip {
                continue;
            }
            total += datum_ledger::balance_at(
                tx,
                BalanceSlice {
                    item: query.item,
                    location: loc.id,
                    lot: Some(lot),
                    serial: None,
                    unit: stock.stock_uom,
                },
                instant,
            )
            .await?;
        }
        return Ok(total);
    }
    let stock = load_stock_item(tx, query.item).await?;
    let instant = Utc::now();
    let mut total = Decimal::ZERO;
    for loc in datum_mod_locations::list_flat(tx).await? {
        if loc.boundary_class.is_some() || loc.kind == LocationKind::Wip {
            continue;
        }
        if let Some(only) = query.location
            && only != loc.id
        {
            continue;
        }
        total += datum_ledger::balance_at(
            tx,
            BalanceSlice {
                item: query.item,
                location: loc.id,
                lot: None,
                serial: None,
                unit: stock.stock_uom,
            },
            instant,
        )
        .await?;
        let layers = load_open_layers(tx, query.item, loc.id).await?;
        let mut seen = std::collections::HashSet::new();
        for layer in layers {
            if let Some(lot) = layer.lot {
                if !seen.insert(lot) {
                    continue;
                }
                let rec = load_lot(tx, lot).await?;
                if rec.status != LotStatus::Available {
                    continue;
                }
                total += datum_ledger::balance_at(
                    tx,
                    BalanceSlice {
                        item: query.item,
                        location: loc.id,
                        lot: Some(lot),
                        serial: None,
                        unit: stock.stock_uom,
                    },
                    instant,
                )
                .await?;
            }
        }
    }
    Ok(total)
}

/// Documents that mention `item` or `lot`.
pub async fn document_history(
    tx: &mut Tx<'_>,
    item: Option<ItemId>,
    lot: Option<LotId>,
) -> Result<Vec<Document>> {
    let rows: Vec<(Uuid,)> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT DISTINCT d.id
                   FROM inventory.document d
                   JOIN inventory.document_line l ON l.document_id = d.id
                  WHERE ($1::uuid IS NULL OR l.item_id = $1)
                    AND ($2::uuid IS NULL OR l.lot_id = $2)
                  ORDER BY d.id",
            )
            .bind(item.map(|i| i.as_uuid()))
            .bind(lot.map(|l| l.as_uuid())),
        )
        .await?;
    let mut out = Vec::new();
    for (id,) in rows {
        out.push(load_document(tx, Identifier::from_uuid(id)).await?);
    }
    Ok(out)
}

/// Load one document and its lines.
pub async fn load_document(tx: &mut Tx<'_>, id: Identifier) -> Result<Document> {
    let row: Option<DocRow> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT id, kind, status, reference, posted_group_id, version,
                        application_version, configuration_version
                   FROM inventory.document WHERE id = $1",
            )
            .bind(id.as_uuid()),
        )
        .await?;
    let Some(row) = row else {
        return Err(Error::NotFound);
    };
    let lines = load_lines(tx, id).await?;
    document_from_row(row, lines)
}

fn document_from_row(row: DocRow, lines: Vec<DocumentLine>) -> Result<Document> {
    let (id, kind, status, reference, posted, version, app, cfg) = row;
    Ok(Document {
        id: Identifier::from_uuid(id),
        kind: DocumentKind::parse(&kind)?,
        status: DocumentStatus::parse(&status)?,
        reference,
        posted_group_id: posted.map(Identifier::from_uuid),
        version,
        application_version: app,
        configuration_version: cfg,
        lines,
    })
}

async fn load_lines(tx: &mut Tx<'_>, doc: Identifier) -> Result<Vec<DocumentLine>> {
    let rows: Vec<LineRow> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT id, document_id, item_id, lot_id, serial_id,
                        from_location_id, to_location_id,
                        entered_amount, entered_uom_id, entered_dimension,
                        canonical_amount, canonical_uom_id, canonical_dimension,
                        conversion_factor, reason_code, package_id
                   FROM inventory.document_line WHERE document_id = $1
                  ORDER BY id",
            )
            .bind(doc.as_uuid()),
        )
        .await?;
    rows.into_iter().map(line_from_row).collect()
}

fn line_from_row(row: LineRow) -> Result<DocumentLine> {
    let (
        id,
        document_id,
        item,
        lot,
        serial,
        from_loc,
        to_loc,
        e_amt,
        e_uom,
        e_dim,
        c_amt,
        c_uom,
        c_dim,
        factor,
        reason,
        package,
    ) = row;
    Ok(DocumentLine {
        id: Identifier::from_uuid(id),
        document_id: Identifier::from_uuid(document_id),
        item: ItemId::from_uuid(item),
        lot: lot.map(LotId::from_uuid),
        serial: serial.map(datum_core::SerialId::from_uuid),
        from_location: from_loc.map(LocationId::from_uuid),
        to_location: to_loc.map(LocationId::from_uuid),
        entered: AnyQuantity {
            amount: e_amt,
            unit: datum_core::UnitId(e_uom),
            dimension: parse_dim(&e_dim)?,
        },
        canonical: AnyQuantity {
            amount: c_amt,
            unit: datum_core::UnitId(c_uom),
            dimension: parse_dim(&c_dim)?,
        },
        conversion_factor: factor,
        reason_code: reason,
        package: package.map(PackageId::from_uuid),
    })
}

fn parse_dim(s: &str) -> Result<DimensionKind> {
    match s {
        "Count" => Ok(DimensionKind::Count),
        "Length" => Ok(DimensionKind::Length),
        "Mass" => Ok(DimensionKind::Mass),
        "Time" => Ok(DimensionKind::Time),
        "Volume" => Ok(DimensionKind::Volume),
        "Area" => Ok(DimensionKind::Area),
        _ => Err(Error::UnknownDimension),
    }
}

#[derive(Debug, Clone)]
struct PreparedLine {
    item: ItemId,
    lot: Option<LotId>,
    serial: Option<datum_core::SerialId>,
    from_location: Option<LocationId>,
    to_location: Option<LocationId>,
    entered: AnyQuantity,
    canonical: AnyQuantity,
    conversion_factor: Decimal,
    amount: Option<Money>,
    reason_code: Option<String>,
    package: Option<PackageId>,
}

async fn prepare_line_with_residual(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    mut line: LineInput,
) -> Result<(PreparedLine, AnyQuantity)> {
    if let Some(pkg) = line.package {
        let lot = line
            .lot
            .ok_or_else(|| Error::Document("package requires a lot entity".into()))?;
        let nodes = package_hierarchy(tx, lot).await?;
        let found = nodes
            .iter()
            .find(|n| n.id == pkg)
            .ok_or_else(|| Error::Document("package is not in this lot".into()))?;
        line.entered = found.contained;
    }
    let (canonical, factor, residual) =
        convert_entered(tx, kernel, line.item, line.lot, line.entered).await?;
    Ok((
        PreparedLine {
            item: line.item,
            lot: line.lot,
            serial: line.serial,
            from_location: line.from_location,
            to_location: line.to_location,
            entered: line.entered,
            canonical,
            conversion_factor: factor,
            amount: line.amount,
            reason_code: line.reason_code,
            package: line.package,
        },
        residual,
    ))
}

async fn prepare_line(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    mut line: LineInput,
) -> Result<PreparedLine> {
    if let Some(pkg) = line.package {
        let lot = line
            .lot
            .ok_or_else(|| Error::Document("package requires a lot entity".into()))?;
        let nodes = package_hierarchy(tx, lot).await?;
        let found = nodes
            .iter()
            .find(|n| n.id == pkg)
            .ok_or_else(|| Error::Document("package is not in this lot".into()))?;
        line.entered = found.contained;
    }
    let (canonical, factor, _residual) =
        convert_entered(tx, kernel, line.item, line.lot, line.entered).await?;
    Ok(PreparedLine {
        item: line.item,
        lot: line.lot,
        serial: line.serial,
        from_location: line.from_location,
        to_location: line.to_location,
        entered: line.entered,
        canonical,
        conversion_factor: factor,
        amount: line.amount,
        reason_code: line.reason_code,
        package: line.package,
    })
}

async fn convert_entered(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    item: ItemId,
    lot: Option<LotId>,
    entered: AnyQuantity,
) -> Result<(AnyQuantity, Decimal, AnyQuantity)> {
    let catalog = datum_uom::load_catalog(tx).await?;
    let ctx = ConversionContext { item, lot };
    match entered.dimension {
        DimensionKind::Count => pack(
            kernel
                .to_stock::<CountDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        ),
        DimensionKind::Length => pack(
            kernel
                .to_stock::<LengthDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        ),
        DimensionKind::Mass => pack(
            kernel
                .to_stock::<MassDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        ),
        DimensionKind::Time => pack(
            kernel
                .to_stock::<TimeDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        ),
        DimensionKind::Volume => pack(
            kernel
                .to_stock::<VolumeDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        ),
        DimensionKind::Area => pack(
            kernel
                .to_stock::<AreaDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        ),
        _ => Err(Error::UnknownDimension),
    }
}

fn pack<D: datum_core::Dimension>(
    conv: datum_uom::StockConversion<D>,
) -> Result<(AnyQuantity, Decimal, AnyQuantity)> {
    Ok((
        AnyQuantity::from(conv.canonical),
        conv.factor,
        AnyQuantity::from(conv.residual),
    ))
}

fn movement_builder(
    source: &str,
    source_id: Option<Identifier>,
    work_order_id: Option<Identifier>,
    reason_code: Option<String>,
) -> GroupBuilder {
    GroupBuilder::new(
        GroupKind::Movement,
        PostingGroupHeader {
            source_kind: source.into(),
            source_id,
            work_order_id,
            reason_code,
            reverses_group_id: None,
        },
    )
}

fn qty(
    item: ItemId,
    quantity: AnyQuantity,
    location: LocationId,
    boundary: Option<Boundary>,
    lot: Option<LotId>,
    serial: Option<datum_core::SerialId>,
    entered: Option<AnyQuantity>,
) -> QuantityPosting {
    QuantityPosting {
        item,
        quantity,
        location,
        boundary,
        lot,
        serial,
        entered,
    }
}

fn signed(base: AnyQuantity, sign: i32) -> AnyQuantity {
    AnyQuantity {
        amount: base.amount * Decimal::from(sign),
        unit: base.unit,
        dimension: base.dimension,
    }
}

fn contribute_receive(
    builder: &mut GroupBuilder,
    p: &PreparedLine,
    supplier: LocationId,
    dest: LocationId,
) -> Result<()> {
    let into = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        p.canonical,
        dest,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        signed(p.canonical, -1),
        supplier,
        Some(Boundary::Supplier),
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    if let Some(amount) = p.amount {
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount,
            values: Some(into),
        }))?;
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::ApAccrual,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: amount.negate(),
            values: None,
        }))?;
    }
    Ok(())
}

async fn contribute_move(
    builder: &mut GroupBuilder,
    tx: &mut Tx<'_>,
    p: &PreparedLine,
    explicit: bool,
) -> Result<()> {
    let from = p
        .from_location
        .ok_or_else(|| Error::Document("from_location".into()))?;
    let to = p
        .to_location
        .ok_or_else(|| Error::Document("to_location".into()))?;
    let out = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        signed(p.canonical, -1),
        from,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let into = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        p.canonical,
        to,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let (money, edges) = if let Some(amount) = p.amount {
        (amount, Vec::new())
    } else {
        let layers = load_open_layers(tx, p.item, from).await?;
        cover_layers(&layers, p.canonical.amount.abs(), p.lot, p.serial)?
    };
    if !money.amount().is_zero() {
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: money.negate(),
            values: Some(out),
        }))?;
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: money,
            values: Some(into),
        }))?;
    }
    if explicit {
        if edges.is_empty() {
            contribute_explicit(builder, tx, p, out, from).await?;
        } else {
            for edge in edges {
                builder.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
                    consuming: out,
                    consumed_posting_id: edge.posting_id,
                    quantity: AnyQuantity {
                        amount: edge.qty,
                        unit: p.canonical.unit,
                        dimension: p.canonical.dimension,
                    },
                    amount: edge.amount,
                }))?;
            }
        }
    }
    Ok(())
}

async fn contribute_issue(
    builder: &mut GroupBuilder,
    tx: &mut Tx<'_>,
    p: &PreparedLine,
    work_order: Identifier,
) -> Result<()> {
    let from = p
        .from_location
        .ok_or_else(|| Error::Document("from_location".into()))?;
    let to = p
        .to_location
        .ok_or_else(|| Error::Document("to_location".into()))?;
    let out = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        signed(p.canonical, -1),
        from,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let into = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        p.canonical,
        to,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let explicit = p.lot.is_some() || p.serial.is_some();
    let (money, edges) = if let Some(amount) = p.amount {
        (amount, Vec::new())
    } else {
        let layers = load_open_layers(tx, p.item, from).await?;
        cover_layers(&layers, p.canonical.amount.abs(), p.lot, p.serial)?
    };
    if !money.amount().is_zero() {
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: money.negate(),
            values: Some(out),
        }))?;
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Wip,
            cost_element: CostElement::Material,
            cost_object: Some(work_order),
            amount: money,
            values: Some(into),
        }))?;
    }
    if explicit {
        if edges.is_empty() {
            contribute_explicit(builder, tx, p, out, from).await?;
        } else {
            for edge in edges {
                builder.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
                    consuming: out,
                    consumed_posting_id: edge.posting_id,
                    quantity: AnyQuantity {
                        amount: edge.qty,
                        unit: p.canonical.unit,
                        dimension: p.canonical.dimension,
                    },
                    amount: edge.amount,
                }))?;
            }
        }
    }
    Ok(())
}

async fn contribute_ship(
    builder: &mut GroupBuilder,
    tx: &mut Tx<'_>,
    p: &PreparedLine,
    customer: LocationId,
    order: Identifier,
) -> Result<()> {
    let from = p
        .from_location
        .ok_or_else(|| Error::Document("from_location".into()))?;
    let out = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        signed(p.canonical, -1),
        from,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        p.canonical,
        customer,
        Some(Boundary::Customer),
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let explicit = p.lot.is_some() || p.serial.is_some();
    let (money, edges) = if let Some(amount) = p.amount {
        (amount, Vec::new())
    } else {
        let layers = load_open_layers(tx, p.item, from).await?;
        cover_layers(&layers, p.canonical.amount.abs(), p.lot, p.serial)?
    };
    if !money.amount().is_zero() {
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: money.negate(),
            values: Some(out),
        }))?;
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Cogs,
            cost_element: CostElement::Material,
            cost_object: Some(order),
            amount: money,
            values: None,
        }))?;
    }
    if explicit {
        if edges.is_empty() {
            contribute_explicit(builder, tx, p, out, from).await?;
        } else {
            for edge in edges {
                builder.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
                    consuming: out,
                    consumed_posting_id: edge.posting_id,
                    quantity: AnyQuantity {
                        amount: edge.qty,
                        unit: p.canonical.unit,
                        dimension: p.canonical.dimension,
                    },
                    amount: edge.amount,
                }))?;
            }
        }
    }
    Ok(())
}

fn contribute_return(
    builder: &mut GroupBuilder,
    p: &PreparedLine,
    customer: LocationId,
    dest: LocationId,
) -> Result<()> {
    builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        signed(p.canonical, -1),
        customer,
        Some(Boundary::Customer),
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let into = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        p.canonical,
        dest,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    if let Some(amount) = p.amount {
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Cogs,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: amount.negate(),
            values: None,
        }))?;
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount,
            values: Some(into),
        }))?;
    }
    Ok(())
}

async fn contribute_adjustment(
    builder: &mut GroupBuilder,
    tx: &mut Tx<'_>,
    p: &PreparedLine,
    boundary: Boundary,
) -> Result<()> {
    let loc = p
        .from_location
        .ok_or_else(|| Error::Document("location".into()))?;
    let dest = p
        .to_location
        .ok_or_else(|| Error::Document("boundary location".into()))?;
    let abs = AnyQuantity {
        amount: p.canonical.amount.abs(),
        unit: p.canonical.unit,
        dimension: p.canonical.dimension,
    };
    let leaving = p.canonical.amount.is_sign_negative() || p.canonical.amount.is_zero();
    let at_real = if leaving { signed(abs, -1) } else { abs };
    let at_bound = if leaving { abs } else { signed(abs, -1) };
    let real_h = builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        at_real,
        loc,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    builder.contribute(PostingIntent::Quantity(qty(
        p.item,
        at_bound,
        dest,
        Some(boundary),
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let expense = match boundary {
        Boundary::Scrap => ValueAccount::ScrapExpense,
        Boundary::Rounding => ValueAccount::Rounding,
        _ => ValueAccount::AdjustmentExpense,
    };
    if let Some(amount) = p.amount {
        let signed_amt = if leaving { amount.negate() } else { amount };
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: signed_amt,
            values: Some(real_h),
        }))?;
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: expense,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: signed_amt.negate(),
            values: None,
        }))?;
    }
    if leaving && (p.lot.is_some() || p.serial.is_some()) {
        contribute_explicit(builder, tx, p, real_h, loc).await?;
    }
    Ok(())
}

async fn contribute_explicit(
    builder: &mut GroupBuilder,
    tx: &mut Tx<'_>,
    p: &PreparedLine,
    consuming: datum_core::PostingHandle,
    location: LocationId,
) -> Result<()> {
    let layers = load_open_layers(tx, p.item, location).await?;
    let layer = layers
        .iter()
        .find(|l| {
            p.lot.is_none_or(|lot| l.lot == Some(lot))
                && p.serial.is_none_or(|s| l.serial == Some(s))
        })
        .ok_or(Error::NoEligibleLayer)?;
    let qty_abs = p.canonical.amount.abs();
    let amt = if let Some(given) = p.amount {
        Money::new(given.amount().abs(), given.currency()).map_err(datum_core::Error::from)?
    } else if layer.remaining_qty.is_zero() {
        Money::zero(layer.currency)
    } else {
        let share = layer.remaining_amt * qty_abs / layer.remaining_qty;
        Money::new(share, layer.currency).map_err(datum_core::Error::from)?
    };
    builder.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
        consuming,
        consumed_posting_id: layer.posting_id,
        quantity: AnyQuantity {
            amount: qty_abs,
            unit: p.canonical.unit,
            dimension: p.canonical.dimension,
        },
        amount: amt,
    }))?;
    Ok(())
}

async fn counterpart_for_reason(tx: &mut Tx<'_>, reason: &str) -> Result<(LocationId, Boundary)> {
    if reason == datum_ledger::UOM_CONVERSION_RESIDUAL {
        Ok((
            boundary_location_id(tx, Boundary::Rounding).await?,
            Boundary::Rounding,
        ))
    } else if reason.to_ascii_uppercase().contains("SCRAP") {
        Ok((
            boundary_location_id(tx, Boundary::Scrap).await?,
            Boundary::Scrap,
        ))
    } else {
        Ok((
            boundary_location_id(tx, Boundary::Adjustment).await?,
            Boundary::Adjustment,
        ))
    }
}

async fn insert_document(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    id: Identifier,
    kind: DocumentKind,
    reference: Option<String>,
    lines: &[PreparedLine],
) -> Result<()> {
    let (app, mut cfg) = stamps(tx).await?;
    if cfg.is_empty() {
        cfg = kernel.profile.spec_version.clone();
    }
    tx.execute(
        sqlx::query(
            "INSERT INTO inventory.document (
                 id, kind, status, reference, posted_group_id, version,
                 application_version, configuration_version
             ) VALUES ($1, $2, 'draft', $3, NULL, 1, $4, $5)",
        )
        .bind(id.as_uuid())
        .bind(kind.as_str())
        .bind(reference.as_deref())
        .bind(&app)
        .bind(&cfg),
    )
    .await?;
    for p in lines {
        tx.execute(
            sqlx::query(
                "INSERT INTO inventory.document_line (
                     id, document_id, item_id, lot_id, serial_id,
                     from_location_id, to_location_id,
                     entered_amount, entered_uom_id, entered_dimension,
                     canonical_amount, canonical_uom_id, canonical_dimension,
                     conversion_factor, reason_code, package_id,
                     application_version, configuration_version
                 ) VALUES (
                     $1, $2, $3, $4, $5, $6, $7,
                     $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18
                 )",
            )
            .bind(Identifier::generate().as_uuid())
            .bind(id.as_uuid())
            .bind(p.item.as_uuid())
            .bind(p.lot.map(|l| l.as_uuid()))
            .bind(p.serial.map(|s| s.as_uuid()))
            .bind(p.from_location.map(|l| l.as_uuid()))
            .bind(p.to_location.map(|l| l.as_uuid()))
            .bind(p.entered.amount)
            .bind(p.entered.unit.0)
            .bind(format!("{:?}", p.entered.dimension))
            .bind(p.canonical.amount)
            .bind(p.canonical.unit.0)
            .bind(format!("{:?}", p.canonical.dimension))
            .bind(p.conversion_factor)
            .bind(p.reason_code.as_deref())
            .bind(p.package.map(|pkg| pkg.as_uuid()))
            .bind(&app)
            .bind(&cfg),
        )
        .await?;
    }
    kernel
        .spawn(
            tx,
            &DocRef {
                doc_type: DOC_TYPE.into(),
                doc_id: id,
            },
            DocumentStatus::Draft.as_str(),
        )
        .await?;
    Ok(())
}

/// Void a posted document (ledger rows remain; state machine `void` edge).
pub async fn void_document(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    id: Identifier,
) -> Result<Document> {
    let doc = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: id,
    };
    kernel
        .engine
        .transition(
            tx,
            Box::new(NoPostings),
            &doc,
            "void",
            None,
            kernel.signature_gate(),
            ctx,
        )
        .await?;
    tx.execute(
        sqlx::query(
            "UPDATE inventory.document SET status = 'voided', version = version + 1 WHERE id = $1",
        )
        .bind(id.as_uuid()),
    )
    .await?;
    load_document(tx, id).await
}

/// Post a ledger [`REVERSAL`] for a posted issue document (D2 case l).
pub async fn reverse_posted_issue(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    id: Identifier,
) -> Result<Identifier> {
    let doc = load_document(tx, id).await?;
    if doc.kind != DocumentKind::Issue {
        return Err(Error::Document(
            "only issue documents can be reversed here".into(),
        ));
    }
    let group = doc
        .posted_group_id
        .ok_or_else(|| Error::Document("document is not posted".into()))?;
    let _ = kernel;
    let _ = ctx;
    Ok(datum_ledger::reverse(tx, group, "ISSUE_REVERSAL").await?)
}

async fn stamp_posted(tx: &mut Tx<'_>, id: Identifier, group_id: Option<Identifier>) -> Result<()> {
    tx.execute(
        sqlx::query(
            "UPDATE inventory.document
                SET status = 'posted', posted_group_id = $2, version = version + 1
              WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(group_id.map(|g| g.as_uuid())),
    )
    .await?;
    Ok(())
}

async fn begin_idempotent(
    tx: &mut Tx<'_>,
    key: Option<Uuid>,
    body_hash: &str,
    document_id: Identifier,
) -> Result<Option<Identifier>> {
    let Some(key) = key else {
        return Ok(None);
    };
    let row: Option<(Uuid, String)> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT document_id, body_hash FROM inventory_transient.idempotency WHERE key = $1",
            )
            .bind(key),
        )
        .await?;
    if let Some((id, hash)) = row {
        if hash != body_hash {
            return Err(Error::IdempotencyConflict);
        }
        return Ok(Some(Identifier::from_uuid(id)));
    }
    tx.execute(
        sqlx::query(
            "INSERT INTO inventory_transient.idempotency (key, body_hash, document_id)
             VALUES ($1, $2, $3)",
        )
        .bind(key)
        .bind(body_hash)
        .bind(document_id.as_uuid()),
    )
    .await?;
    Ok(None)
}

async fn post_uom_residuals(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    _ctx: &datum_db::WriteContext,
    doc_id: Identifier,
    location: LocationId,
    parent_group: Identifier,
    residuals: &[(ItemId, Option<LotId>, AnyQuantity)],
) -> Result<()> {
    let rounding = boundary_location_id(tx, Boundary::Rounding).await?;
    for (item, lot, residual) in residuals {
        if residual.amount.is_zero() {
            continue;
        }
        let stock = load_stock_item(tx, *item).await?;
        let scale = u32::try_from(stock.stock_scale).unwrap_or(0);
        let mut amount = residual.amount;
        if amount.scale() > scale {
            let rounded = amount.round_dp(scale);
            if rounded.is_zero() && !amount.is_zero() {
                // R-2s-6: residual is its own ADJUSTMENT in this Tx; the leftover
                // must be exact at stock_scale to pass quantity_exact_at_scale.
                let quantum = Decimal::new(1, scale);
                amount = if amount.is_sign_negative() {
                    -quantum
                } else {
                    quantum
                };
            } else {
                amount = rounded;
            }
        }
        if amount.is_zero() || amount.abs() > stock.residual_tolerance {
            continue;
        }
        let residual = AnyQuantity {
            amount: -amount.abs(),
            unit: residual.unit,
            dimension: residual.dimension,
        };
        let layers = load_open_layers(tx, *item, location).await?;
        let (money, _) = cover_layers(&layers, residual.amount.abs(), *lot, None)?;
        let adj = GroupBuilder::new(
            GroupKind::Adjustment,
            PostingGroupHeader {
                source_kind: format!("inventory.residual.parent.{parent_group}"),
                source_id: Some(doc_id),
                work_order_id: None,
                reason_code: Some(datum_ledger::UOM_CONVERSION_RESIDUAL.into()),
                reverses_group_id: None,
            },
        );
        let line = PreparedLine {
            item: *item,
            lot: *lot,
            serial: None,
            from_location: Some(location),
            to_location: Some(rounding),
            entered: residual,
            canonical: residual,
            conversion_factor: Decimal::ONE,
            amount: if money.amount().is_zero() {
                None
            } else {
                Some(money)
            },
            reason_code: Some(datum_ledger::UOM_CONVERSION_RESIDUAL.into()),
            package: None,
        };
        let mut builder = adj;
        contribute_adjustment(&mut builder, tx, &line, Boundary::Rounding).await?;
        kernel.bind_sink(tx, &mut builder).await?;
        let _ = datum_ledger::post(tx, builder).await?;
    }
    Ok(())
}

fn hash_receive(req: &ReceiveRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "receive",
            req.to_location,
            req.reference.as_deref(),
            line_fingerprints(&req.lines),
            req.expected,
            req.tolerance,
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn hash_release(req: &ReleaseRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "release",
            req.lot,
            req.from_location,
            req.to_location,
            req.entered.amount,
            req.amount.map(|m| m.amount().to_string()),
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn hash_issue(req: &IssueRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "issue",
            req.work_order,
            req.from_location,
            req.reference.as_deref(),
            line_fingerprints(&req.lines),
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn hash_move(req: &MoveRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "move",
            req.from_location,
            req.to_location,
            req.reference.as_deref(),
            line_fingerprints(&req.lines),
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn hash_adjust(req: &AdjustRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "adjust",
            req.reason.as_str(),
            req.location,
            req.reference.as_deref(),
            line_fingerprints(&req.lines),
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn hash_count(req: &CountRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "count",
            req.location,
            req.reference.as_deref(),
            req.tolerance,
            req.lines
                .iter()
                .map(|l| {
                    format!(
                        "{}:{}:{}:{}",
                        l.item,
                        l.lot.map(|x| x.to_string()).unwrap_or_default(),
                        l.counted.amount,
                        l.expected.amount
                    )
                })
                .collect::<Vec<_>>(),
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn hash_ship(req: &ShipRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "ship",
            req.order,
            req.from_location,
            req.reference.as_deref(),
            line_fingerprints(&req.lines),
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn hash_return(req: &ReturnRequest) -> String {
    sha256_hex(
        serde_json::to_string(&(
            "return",
            req.order,
            req.to_location,
            req.reference.as_deref(),
            line_fingerprints(&req.lines),
        ))
        .unwrap_or_default()
        .as_bytes(),
    )
}

fn line_fingerprints(lines: &[LineInput]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            format!(
                "{}:{}:{}:{}",
                l.item,
                l.lot.map(|x| x.to_string()).unwrap_or_default(),
                l.entered.amount,
                l.amount.map(|m| m.amount().to_string()).unwrap_or_default()
            )
        })
        .collect()
}
