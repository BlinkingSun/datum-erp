//! Persistence and posting. Every mutation runs inside [`datum_db::Tx`].
//! Postings go through [`datum_ledger::GroupBuilder`] obtained from the kernel.

use chrono::{DateTime, Utc};
use datum_core::{
    AnyQuantity, AreaDim, Boundary, ConversionContext, CostElement, CountDim, CurrencyId,
    DimensionKind, GroupKind, Identifier, ItemId, LengthDim, LocationId, LotId, MassDim, Money,
    PostingGroupHeader, PostingIntent, PostingSink, QuantityPosting, TimeDim, ValueAccount,
    ValuePosting, VolumeDim,
};
use datum_db::Tx;
use datum_ledger::{CostMethod, Layer, load_open_layers, load_stock_item};
use datum_mod_inventory::{IssueRequest, issue_to_wip};
use datum_mod_lots::{CreateLot, LotStatus, create_lot, create_serials};
use datum_module::Kernel;
use datum_numbering::{ResetPolicy, SequenceId};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{
    CompleteRequest, Completion, CreateWorkOrder, DEFAULT_FINISHED_LOT_TEMPLATE, DOC_TYPE,
    FinishedLotTemplate, IssueLine, IssueMaterialRequest, ListFilter, Page, Status, WorkOrder,
    quantity_from_parts,
};
use crate::error::{Error, Result};
use crate::states::doc_ref;
use crate::{events, stamps};

type WoRow = (
    Uuid,
    Option<String>,
    Uuid,
    Decimal,
    i64,
    String,
    String,
    String,
    Option<Uuid>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    i64,
    String,
    String,
);

type CompletionRow = (
    Uuid,
    Uuid,
    Uuid,
    Decimal,
    i64,
    String,
    Decimal,
    i64,
    String,
    Uuid,
);

type IssueLineRow = (
    Uuid,
    Uuid,
    Uuid,
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Decimal,
    i64,
    String,
    Option<Decimal>,
    Option<i32>,
);

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

/// Insert a draft work order and spawn the state-machine instance.
pub async fn create(tx: &mut Tx<'_>, kernel: &Kernel, spec: CreateWorkOrder) -> Result<WorkOrder> {
    if spec.quantity_ordered.amount <= Decimal::ZERO {
        return Err(Error::InvalidQuantity);
    }
    let _item = datum_mod_items::get(kernel.pool(), spec.item).await?;
    let id = Identifier::generate();
    let (app, cfg) = stamps(tx).await?;
    tx.execute(
        sqlx::query(
            "INSERT INTO production_min.work_order (
                 id, number, item_id,
                 quantity_ordered_amount, quantity_ordered_uom_id, quantity_ordered_dimension,
                 revision, status, wip_location_id, released_at, completed_at, version,
                 application_version, configuration_version
             ) VALUES (
                 $1, NULL, $2, $3, $4, $5, $6, 'draft', NULL, NULL, NULL, 1, $7, $8
             )",
        )
        .bind(id.as_uuid())
        .bind(spec.item.as_uuid())
        .bind(spec.quantity_ordered.amount)
        .bind(spec.quantity_ordered.unit.0)
        .bind(format!("{:?}", spec.quantity_ordered.dimension))
        .bind(&spec.revision)
        .bind(&app)
        .bind(&cfg),
    )
    .await?;
    kernel
        .spawn(tx, &doc_ref(id), Status::Draft.as_str())
        .await?;
    load(tx, id).await
}

/// Allocate the gap-free number late, create the WIP location, transition to released.
pub async fn release(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    id: Identifier,
) -> Result<WorkOrder> {
    let wo = load(tx, id).await?;
    if wo.status != Status::Draft {
        return Err(Error::InvalidTransition {
            edge: "release".into(),
            status: wo.status.as_str().into(),
        });
    }
    let number = allocate_wo_number(tx, kernel).await?;
    let wip = datum_mod_locations::ensure_wip(tx, id).await?;
    kernel
        .transition(tx, &doc_ref(id), "release", None, ctx)
        .await?;
    tx.execute(
        sqlx::query(
            "UPDATE production_min.work_order
                SET number = $2,
                    wip_location_id = $3,
                    status = 'released',
                    released_at = now(),
                    version = version + 1
              WHERE id = $1 AND status = 'draft'",
        )
        .bind(id.as_uuid())
        .bind(&number)
        .bind(wip.as_uuid()),
    )
    .await?;
    kernel
        .publish_event(tx, events::work_order_released(id, &number)?)
        .await?;
    load(tx, id).await
}

/// Delegate to [`issue_to_wip`] and record issued components on this module's table.
///
/// The bound action must be `inventory.issue` (inventory's document machine).
/// Call [`start`] afterwards in a `production.issue` transaction to advance the WO.
pub async fn issue_material(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: IssueMaterialRequest,
) -> Result<WorkOrder> {
    let wo = load(tx, req.work_order).await?;
    if !matches!(wo.status, Status::Released | Status::InProcess) {
        return Err(Error::InvalidTransition {
            edge: "issue".into(),
            status: wo.status.as_str().into(),
        });
    }
    let wip = wo
        .wip_location
        .ok_or_else(|| Error::Manifest("wip location missing after release".into()))?;
    let _ = wip;
    let doc = issue_to_wip(
        tx,
        kernel,
        ctx,
        IssueRequest {
            work_order: req.work_order,
            from_location: req.from_location,
            reference: wo.number.clone(),
            lines: req.lines.clone(),
            idempotency_key: req.idempotency_key,
        },
    )
    .await?;
    let (app, cfg) = stamps(tx).await?;
    for (req_line, doc_line) in req.lines.iter().zip(doc.lines.iter()) {
        tx.execute(
            sqlx::query(
                "INSERT INTO production_min.issue_line (
                     id, work_order_id, inventory_document_id, item_id, lot_id, serial_id,
                     quantity_amount, quantity_uom_id, quantity_dimension,
                     amount, currency_id, application_version, configuration_version
                 ) VALUES (
                     $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
                 )",
            )
            .bind(Identifier::generate().as_uuid())
            .bind(req.work_order.as_uuid())
            .bind(doc.id.as_uuid())
            .bind(doc_line.item.as_uuid())
            .bind(doc_line.lot.map(|l| l.as_uuid()))
            .bind(doc_line.serial.map(|s| s.as_uuid()))
            .bind(doc_line.canonical.amount)
            .bind(doc_line.canonical.unit.0)
            .bind(format!("{:?}", doc_line.canonical.dimension))
            .bind(req_line.amount.map(|m| m.amount()))
            .bind(req_line.amount.map(|m| m.currency().0))
            .bind(&app)
            .bind(&cfg),
        )
        .await?;
    }
    load(tx, req.work_order).await
}

/// `released → in_process`. Bound action must be `production.issue`.
pub async fn start(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    id: Identifier,
) -> Result<WorkOrder> {
    let wo = load(tx, id).await?;
    if wo.status != Status::Released {
        return Err(Error::InvalidTransition {
            edge: "issue".into(),
            status: wo.status.as_str().into(),
        });
    }
    kernel
        .transition(tx, &doc_ref(id), "issue", None, ctx)
        .await?;
    tx.execute(
        sqlx::query(
            "UPDATE production_min.work_order
                SET status = 'in_process', version = version + 1
              WHERE id = $1 AND status = 'released'",
        )
        .bind(id.as_uuid()),
    )
    .await?;
    load(tx, id).await
}

/// Priced TRANSFORMATION (D2 case d) plus optional scrap ADJUSTMENT (case e).
///
/// Bound action must be `production.complete`. Requires `in_process`.
pub async fn complete(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    req: CompleteRequest,
) -> Result<Completion> {
    let wo = load(tx, req.work_order).await?;
    if wo.status != Status::InProcess {
        return Err(Error::InvalidTransition {
            edge: "complete".into(),
            status: wo.status.as_str().into(),
        });
    }
    let wip = wo
        .wip_location
        .ok_or_else(|| Error::Manifest("wip location missing".into()))?;
    let good = convert_entered(tx, kernel, wo.item, None, req.good).await?;
    let scrap = convert_entered(tx, kernel, wo.item, None, req.scrap).await?;
    if scrap.unit != good.unit || scrap.dimension != good.dimension {
        return Err(Error::Manifest(
            "scrap quantity must use the same stock unit as good".into(),
        ));
    }
    if good.amount < Decimal::ZERO || scrap.amount < Decimal::ZERO {
        return Err(Error::InvalidQuantity);
    }
    let produced_qty = good.amount + scrap.amount;
    if produced_qty <= Decimal::ZERO {
        return Err(Error::InvalidQuantity);
    }
    let produced = AnyQuantity {
        amount: produced_qty,
        unit: good.unit,
        dimension: good.dimension,
    };

    let finished = create_lot(
        tx,
        kernel,
        ctx,
        CreateLot {
            item: wo.item,
            number: req.finished_lot.number.clone(),
            template: req
                .finished_lot
                .template
                .clone()
                .or_else(|| Some(DEFAULT_FINISHED_LOT_TEMPLATE.into())),
            supplier_lot: None,
            heat_or_source_ref: wo.number.clone(),
            expiry: None,
            cert_ref: None,
            status: LotStatus::Quarantine,
        },
    )
    .await?;
    maybe_create_serials(tx, kernel, &req.finished_lot, finished.id, &good).await?;

    let mut builder = kernel.posting_sink(
        GroupKind::Transformation,
        PostingGroupHeader {
            source_kind: format!("{DOC_TYPE}.complete"),
            source_id: Some(wo.id),
            work_order_id: Some(wo.id),
            reason_code: None,
            reverses_group_id: None,
        },
    );
    kernel.bind_sink(tx, &mut builder).await?;

    let layers = load_issued_layers(tx, wo.id, wip).await?;
    let mut consumed_value = Money::zero(CurrencyId(840));
    let consumed_loc = datum_mod_locations::boundary_location_id(tx, Boundary::Consumed).await?;
    let produced_loc = datum_mod_locations::boundary_location_id(tx, Boundary::Produced).await?;

    for layer in &layers {
        let qty = AnyQuantity {
            amount: layer.remaining_qty,
            unit: layer.uom,
            dimension: dimension_for_unit(layer.uom.0),
        };
        let layer_money = money(layer.remaining_amt, layer.currency)?;
        let bar_out = builder.contribute(PostingIntent::Quantity(qty_post(
            layer.item,
            signed(qty, -1),
            wip,
            None,
            layer.lot,
            layer.serial,
        )))?;
        builder.contribute(PostingIntent::Quantity(qty_post(
            layer.item,
            qty,
            consumed_loc,
            Some(Boundary::Consumed),
            layer.lot,
            layer.serial,
        )))?;
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Wip,
            cost_element: CostElement::Material,
            cost_object: Some(wo.id),
            amount: layer_money.negate(),
            values: Some(bar_out),
        }))?;
        consumed_value = consumed_value
            .try_add(layer_money)
            .map_err(datum_core::Error::from)?;
    }

    let screw_in = builder.contribute(PostingIntent::Quantity(qty_post(
        wo.item,
        produced,
        req.to_location,
        None,
        Some(finished.id),
        None,
    )))?;
    builder.contribute(PostingIntent::Quantity(qty_post(
        wo.item,
        signed(produced, -1),
        produced_loc,
        Some(Boundary::Produced),
        Some(finished.id),
        None,
    )))?;

    let produced_value = produced_value(tx, wo.item, produced, consumed_value).await?;
    builder.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Inventory,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: produced_value,
        values: Some(screw_in),
    }))?;
    let variance = consumed_value
        .try_sub(produced_value)
        .map_err(datum_core::Error::from)?;
    if !variance.amount().is_zero() {
        builder.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::MfgVariance,
            cost_element: CostElement::Material,
            cost_object: Some(wo.id),
            amount: variance,
            values: None,
        }))?;
    }
    for layer in &layers {
        let qty = AnyQuantity {
            amount: layer.remaining_qty,
            unit: layer.uom,
            dimension: dimension_for_unit(layer.uom.0),
        };
        builder.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
            consuming: screw_in,
            consumed_posting_id: layer.posting_id,
            quantity: qty,
            amount: money(layer.remaining_amt, layer.currency)?,
        }))?;
    }

    let group_id = datum_ledger::post(tx, builder).await?;

    if scrap.amount > Decimal::ZERO {
        post_scrap_adjustment(
            tx,
            kernel,
            wo.id,
            wo.item,
            finished.id,
            req.to_location,
            scrap,
            unit_cost(produced_value, produced_qty, produced_value.currency())?,
        )
        .await?;
    }

    kernel
        .transition(tx, &doc_ref(wo.id), "complete", None, ctx)
        .await?;
    tx.execute(
        sqlx::query(
            "UPDATE production_min.work_order
                SET status = 'completed', completed_at = now(), version = version + 1
              WHERE id = $1 AND status = 'in_process'",
        )
        .bind(wo.id.as_uuid()),
    )
    .await?;

    let completion_id = Identifier::generate();
    let (app, cfg) = stamps(tx).await?;
    tx.execute(
        sqlx::query(
            "INSERT INTO production_min.completion (
                 id, work_order_id, finished_lot_id,
                 quantity_good_amount, quantity_good_uom_id, quantity_good_dimension,
                 quantity_scrap_amount, quantity_scrap_uom_id, quantity_scrap_dimension,
                 group_id, application_version, configuration_version
             ) VALUES (
                 $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
             )",
        )
        .bind(completion_id.as_uuid())
        .bind(wo.id.as_uuid())
        .bind(finished.id.as_uuid())
        .bind(good.amount)
        .bind(good.unit.0)
        .bind(format!("{:?}", good.dimension))
        .bind(scrap.amount)
        .bind(scrap.unit.0)
        .bind(format!("{:?}", scrap.dimension))
        .bind(group_id.as_uuid())
        .bind(&app)
        .bind(&cfg),
    )
    .await?;
    kernel
        .publish_event(tx, events::completed(wo.id, finished.id, group_id)?)
        .await?;
    Ok(Completion {
        id: completion_id,
        work_order: wo.id,
        finished_lot: finished.id,
        quantity_good: good,
        quantity_scrap: scrap,
        group_id,
    })
}

/// Load one work order.
pub async fn load(tx: &mut Tx<'_>, id: Identifier) -> Result<WorkOrder> {
    let row: Option<WoRow> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT id, number, item_id,
                        quantity_ordered_amount, quantity_ordered_uom_id, quantity_ordered_dimension,
                        revision, status, wip_location_id, released_at, completed_at, version,
                        application_version, configuration_version
                   FROM production_min.work_order WHERE id = $1",
            )
            .bind(id.as_uuid()),
        )
        .await?;
    match row {
        Some(r) => work_order_from_row(r),
        None => Err(Error::NotFound),
    }
}

/// Cursor-paginated list. Default sort is `id` ascending.
pub async fn list(tx: &mut Tx<'_>, filter: ListFilter) -> Result<Page<WorkOrder>> {
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Error::InvalidLimit);
    }
    let status = filter.status.map(Status::as_str);
    let cursor = filter.cursor.map(|id| id.as_uuid());
    let fetch = i64::from(limit) + 1;
    let rows: Vec<WoRow> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT id, number, item_id,
                        quantity_ordered_amount, quantity_ordered_uom_id, quantity_ordered_dimension,
                        revision, status, wip_location_id, released_at, completed_at, version,
                        application_version, configuration_version
                   FROM production_min.work_order
                  WHERE ($1::text IS NULL OR status = $1)
                    AND ($2::uuid IS NULL OR id > $2)
                  ORDER BY id
                  LIMIT $3",
            )
            .bind(status)
            .bind(cursor)
            .bind(fetch),
        )
        .await?;
    let has_more = rows.len() as u32 > limit;
    let mut data = Vec::with_capacity(rows.len().min(limit as usize));
    for row in rows.into_iter().take(limit as usize) {
        data.push(work_order_from_row(row)?);
    }
    let next_cursor = if has_more {
        data.last().map(|w| w.id.to_string())
    } else {
        None
    };
    Ok(Page {
        data,
        next_cursor,
        has_more,
    })
}

/// Load the completion for a work order, if any.
pub async fn load_completion(
    tx: &mut Tx<'_>,
    work_order: Identifier,
) -> Result<Option<Completion>> {
    let row: Option<CompletionRow> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT id, work_order_id, finished_lot_id,
                        quantity_good_amount, quantity_good_uom_id, quantity_good_dimension,
                        quantity_scrap_amount, quantity_scrap_uom_id, quantity_scrap_dimension,
                        group_id
                   FROM production_min.completion WHERE work_order_id = $1",
            )
            .bind(work_order.as_uuid()),
        )
        .await?;
    match row {
        None => Ok(None),
        Some((id, wo, lot, ga, gu, gd, sa, su, sd, gid)) => Ok(Some(Completion {
            id: Identifier::from_uuid(id),
            work_order: Identifier::from_uuid(wo),
            finished_lot: LotId::from_uuid(lot),
            quantity_good: quantity_from_parts(ga, gu, &gd)?,
            quantity_scrap: quantity_from_parts(sa, su, &sd)?,
            group_id: Identifier::from_uuid(gid),
        })),
    }
}

async fn allocate_wo_number(tx: &mut Tx<'_>, kernel: &Kernel) -> Result<String> {
    let (doc_type, policy) = match kernel.profile.numbering.get("wo") {
        Some(spec) => ("wo", spec.reset_policy()?),
        None => ("wo", ResetPolicy::Yearly),
    };
    Ok(datum_numbering::next_number(tx, SequenceId::new(doc_type, policy)).await?)
}

async fn load_issued_layers(
    tx: &mut Tx<'_>,
    work_order: Identifier,
    wip: LocationId,
) -> Result<Vec<Layer>> {
    let rows: Vec<(Uuid, Option<Uuid>, Option<Uuid>)> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT item_id, lot_id, serial_id
                   FROM production_min.issue_line WHERE work_order_id = $1",
            )
            .bind(work_order.as_uuid()),
        )
        .await?;
    let mut out = Vec::new();
    let mut seen_items = Vec::new();
    for (item, _lot, _serial) in rows {
        let item = ItemId::from_uuid(item);
        if seen_items.contains(&item) {
            continue;
        }
        seen_items.push(item);
        for layer in load_open_layers(tx, item, wip).await? {
            if layer.remaining_qty > Decimal::ZERO {
                out.push(layer);
            }
        }
    }
    Ok(out)
}

async fn produced_value(
    tx: &mut Tx<'_>,
    item: ItemId,
    produced: AnyQuantity,
    consumed: Money,
) -> Result<Money> {
    let stock = load_stock_item(tx, item).await?;
    match stock.cost_method {
        CostMethod::Standard => match stock.standard {
            Some(std) => {
                let amt = std
                    .amount()
                    .checked_mul(produced.amount)
                    .ok_or(datum_core::Error::Overflow)?;
                money(amt, std.currency())
            }
            None => Ok(consumed),
        },
        _ => Ok(consumed),
    }
}

fn unit_cost(total: Money, qty: Decimal, currency: CurrencyId) -> Result<Money> {
    if qty.is_zero() {
        return Ok(Money::zero(currency));
    }
    let amt = total
        .amount()
        .checked_div(qty)
        .ok_or(datum_core::Error::Overflow)?;
    money(amt, currency)
}

#[allow(clippy::too_many_arguments)]
async fn post_scrap_adjustment(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    work_order: Identifier,
    item: ItemId,
    lot: LotId,
    fg: LocationId,
    scrap: AnyQuantity,
    unit_cost: Money,
) -> Result<()> {
    let scrap_loc = datum_mod_locations::boundary_location_id(tx, Boundary::Scrap).await?;
    let amount = money(
        unit_cost
            .amount()
            .checked_mul(scrap.amount)
            .ok_or(datum_core::Error::Overflow)?,
        unit_cost.currency(),
    )?;
    let mut builder = kernel.posting_sink(
        GroupKind::Adjustment,
        PostingGroupHeader {
            source_kind: format!("{DOC_TYPE}.complete"),
            source_id: Some(work_order),
            work_order_id: Some(work_order),
            reason_code: Some("SCRAP_AT_OP".into()),
            reverses_group_id: None,
        },
    );
    kernel.bind_sink(tx, &mut builder).await?;
    let out = builder.contribute(PostingIntent::Quantity(qty_post(
        item,
        signed(scrap, -1),
        fg,
        None,
        Some(lot),
        None,
    )))?;
    builder.contribute(PostingIntent::Quantity(qty_post(
        item,
        scrap,
        scrap_loc,
        Some(Boundary::Scrap),
        Some(lot),
        None,
    )))?;
    builder.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Inventory,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: amount.negate(),
        values: Some(out),
    }))?;
    builder.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::ScrapExpense,
        cost_element: CostElement::Material,
        cost_object: None,
        amount,
        values: None,
    }))?;
    let _ = datum_ledger::post(tx, builder).await?;
    Ok(())
}

async fn maybe_create_serials(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    template: &FinishedLotTemplate,
    lot: LotId,
    good: &AnyQuantity,
) -> Result<()> {
    let Some(serial_template) = template.serial_template.as_deref() else {
        return Ok(());
    };
    if good.dimension != DimensionKind::Count {
        return Ok(());
    }
    if !good.amount.fract().is_zero() || good.amount <= Decimal::ZERO {
        return Ok(());
    }
    let n = rust_decimal::prelude::ToPrimitive::to_u32(&good.amount).unwrap_or(0);
    if n == 0 || n > 64 {
        return Ok(());
    }
    let _ = create_serials(tx, kernel, lot, n, Some(serial_template)).await?;
    Ok(())
}

async fn convert_entered(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    item: ItemId,
    lot: Option<LotId>,
    entered: AnyQuantity,
) -> Result<AnyQuantity> {
    let catalog = datum_uom::load_catalog(tx).await?;
    let ctx = ConversionContext { item, lot };
    let (canonical, _factor, residual) = match entered.dimension {
        DimensionKind::Count => pack(
            kernel
                .to_stock::<CountDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        )?,
        DimensionKind::Length => pack(
            kernel
                .to_stock::<LengthDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        )?,
        DimensionKind::Mass => pack(
            kernel
                .to_stock::<MassDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        )?,
        DimensionKind::Time => pack(
            kernel
                .to_stock::<TimeDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        )?,
        DimensionKind::Volume => pack(
            kernel
                .to_stock::<VolumeDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        )?,
        DimensionKind::Area => pack(
            kernel
                .to_stock::<AreaDim>(tx, &catalog, item, entered, &ctx)
                .await?,
        )?,
        _ => return Err(Error::UnknownDimension),
    };
    if !residual.amount.is_zero() {
        return Err(Error::ConversionResidual);
    }
    Ok(canonical)
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

fn qty_post(
    item: ItemId,
    quantity: AnyQuantity,
    location: LocationId,
    boundary: Option<Boundary>,
    lot: Option<LotId>,
    serial: Option<datum_core::SerialId>,
) -> QuantityPosting {
    QuantityPosting {
        item,
        quantity,
        location,
        boundary,
        lot,
        serial,
        entered: None,
    }
}

fn signed(base: AnyQuantity, sign: i32) -> AnyQuantity {
    AnyQuantity {
        amount: base.amount * Decimal::from(sign),
        unit: base.unit,
        dimension: base.dimension,
    }
}

fn money(amount: Decimal, currency: CurrencyId) -> Result<Money> {
    Ok(Money::new(amount, currency).map_err(datum_core::Error::from)?)
}

fn dimension_for_unit(uom: i64) -> DimensionKind {
    match uom {
        1 => DimensionKind::Count,
        2..=4 => DimensionKind::Length,
        5 | 6 => DimensionKind::Mass,
        7 | 8 => DimensionKind::Time,
        _ => DimensionKind::Count,
    }
}

fn work_order_from_row(row: WoRow) -> Result<WorkOrder> {
    let (
        id,
        number,
        item,
        qty,
        uom,
        dim,
        revision,
        status,
        wip,
        released_at,
        completed_at,
        version,
        app,
        cfg,
    ) = row;
    Ok(WorkOrder {
        id: Identifier::from_uuid(id),
        number,
        item: ItemId::from_uuid(item),
        quantity_ordered: quantity_from_parts(qty, uom, &dim)?,
        revision,
        status: Status::parse(&status)?,
        wip_location: wip.map(LocationId::from_uuid),
        released_at,
        completed_at,
        version,
        application_version: app,
        configuration_version: cfg,
    })
}

/// Recorded issue lines (tests / complete).
pub async fn load_issue_lines(tx: &mut Tx<'_>, work_order: Identifier) -> Result<Vec<IssueLine>> {
    let rows: Vec<IssueLineRow> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT id, work_order_id, inventory_document_id, item_id, lot_id, serial_id,
                        quantity_amount, quantity_uom_id, quantity_dimension, amount, currency_id
                   FROM production_min.issue_line WHERE work_order_id = $1 ORDER BY id",
            )
            .bind(work_order.as_uuid()),
        )
        .await?;
    let mut out = Vec::new();
    for (id, wo, doc, item, lot, serial, amt, uom, dim, money_amt, cur) in rows {
        out.push(IssueLine {
            id: Identifier::from_uuid(id),
            work_order: Identifier::from_uuid(wo),
            inventory_document: Identifier::from_uuid(doc),
            item: ItemId::from_uuid(item),
            lot: lot.map(LotId::from_uuid),
            serial: serial.map(datum_core::SerialId::from_uuid),
            quantity: quantity_from_parts(amt, uom, &dim)?,
            amount: match (money_amt, cur) {
                (Some(a), Some(c)) => Some(money(a, CurrencyId(c))?),
                _ => None,
            },
        });
    }
    Ok(out)
}
