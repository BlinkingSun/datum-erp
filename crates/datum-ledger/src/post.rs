//! Insert a finalized group: header, postings, consumption (CONTRACT §6.2 rule 4).

use std::collections::HashMap;

use datum_core::{
    GroupKind, Identifier, Money, PostingError, PostingHandle, PostingSink, QuantityPosting,
    ValuePosting,
};
use datum_db::Tx;
use uuid::Uuid;

use crate::allocate::{
    AllocationEdge, ExtraValue, allocate_withdrawal, check_explicit_layer, explicit_matches,
    load_open_layers,
};
use crate::builder::{GroupBuilder, is_withdrawal, needs_lineage};
use crate::enums::{
    boundary_sql, cost_element_sql, group_kind_from_sql, group_kind_sql, value_account_sql,
};
use crate::poison;
use crate::registry::load_stock_item;
use crate::{Error, Result};

/// Bind `builder` to the current transaction id (trait-path poison).
pub async fn bind_tx(builder: &mut GroupBuilder, tx: &mut Tx<'_>) -> Result<()> {
    if builder.txid.is_none() {
        builder.set_txid(tx.pg_txid().await.map_err(Error::from)?);
    }
    Ok(())
}

/// Write a finalized [`GroupBuilder`] into `tx`.
///
/// Calls [`PostingSink::finalize`] if the caller has not. Insert order is header,
/// postings, consumption. The actor is read from the transaction context (rule 2).
///
/// `datum-db` does not expose `Tx::poison`. An unfinalized sink is detected here
/// (and in [`crate::commit`]) rather than by a GUC `datum.ledger_unfinalized`.
pub async fn post(tx: &mut Tx<'_>, mut builder: GroupBuilder) -> Result<Identifier> {
    bind_tx(&mut builder, tx).await?;
    let watch = builder.clone();
    {
        let boxed: Box<dyn PostingSink> = Box::new(builder);
        boxed.finalize().map_err(Error::from_posting)?;
    }
    insert_finalized(tx, &watch).await
}

/// Commit `tx` after checking unfinalized sinks (in-process poison keyed by xact id).
pub async fn commit(mut tx: Tx<'_>, sinks: &[&GroupBuilder]) -> Result<()> {
    let txid = tx.pg_txid().await.map_err(Error::from)?;
    if poison::is_marked(&txid) || sinks.iter().any(|s| s.unfinalized()) {
        let _ = tx.rollback().await;
        poison::clear(&txid);
        return Err(Error::Unfinalized);
    }
    tx.commit().await.map_err(Error::from)
}

struct Snapshot {
    kind: GroupKind,
    header: datum_core::PostingGroupHeader,
    quantities: Vec<(PostingHandle, QuantityPosting)>,
    values: Vec<(PostingHandle, ValuePosting)>,
    consumptions: Vec<datum_core::ConsumptionPosting>,
}

fn snapshot(builder: &GroupBuilder) -> Result<Snapshot> {
    let inner = builder.lock_inner().map_err(Error::from_posting)?;
    if inner.contributed && !inner.finalized {
        return Err(Error::Unfinalized);
    }
    if inner.quantities.is_empty() && inner.values.is_empty() {
        return Err(Error::from_posting(PostingError::EmptyGroup));
    }
    Ok(Snapshot {
        kind: inner.kind,
        header: inner.header.clone(),
        quantities: inner.quantities.clone(),
        values: inner.values.clone(),
        consumptions: inner.consumptions.clone(),
    })
}

async fn insert_finalized(tx: &mut Tx<'_>, builder: &GroupBuilder) -> Result<Identifier> {
    let snap = snapshot(builder)?;
    let group_id = Identifier::generate();
    let actor_s = tx.setting("datum.actor_id").await?;
    let actor_id = Uuid::parse_str(&actor_s)
        .map_err(|e| Error::Core(datum_core::Error::Invariant(e.to_string())))?;

    let reverses_kind = match snap.header.reverses_group_id {
        Some(rid) => Some(load_reverses_kind(tx, rid).await?),
        None => None,
    };

    tx.execute(
        sqlx::query(
            "INSERT INTO ledger.posting_group (
                 group_id, kind, actor_id, source_kind, source_id, work_order_id,
                 reason_code, reverses_group_id, reverses_kind
             ) VALUES (
                 $1, $2::ledger.group_kind, $3, $4, $5, $6, $7, $8, $9::ledger.group_kind
             )",
        )
        .bind(group_id.as_uuid())
        .bind(group_kind_sql(snap.kind)?)
        .bind(actor_id)
        .bind(&snap.header.source_kind)
        .bind(snap.header.source_id.map(|i| i.as_uuid()))
        .bind(snap.header.work_order_id.map(|i| i.as_uuid()))
        .bind(snap.header.reason_code.as_deref())
        .bind(snap.header.reverses_group_id.map(|i| i.as_uuid()))
        .bind(reverses_kind.as_deref()),
    )
    .await?;

    let mut handle_to_id: HashMap<u32, i64> = HashMap::new();
    for (handle, q) in &snap.quantities {
        let pid = insert_quantity(tx, group_id, snap.kind, q).await?;
        handle_to_id.insert(handle.0, pid);
    }

    let mut extra_values: Vec<(PostingHandle, ValuePosting)> = snap.values.clone();

    let mut edges: Vec<AllocationEdge> = Vec::new();
    let mut explicit_by_handle: HashMap<u32, Vec<datum_core::ConsumptionPosting>> = HashMap::new();
    for c in &snap.consumptions {
        explicit_by_handle
            .entry(c.consuming.0)
            .or_default()
            .push(c.clone());
    }

    for (handle, q) in &snap.quantities {
        if snap.kind == GroupKind::Reversal || !is_withdrawal(q) {
            continue;
        }
        let value_sum = sum_values_for(*handle, &snap.values)?;
        if let Some(explicit) = explicit_by_handle.remove(&handle.0) {
            let layers = load_open_layers(tx, q.item, q.location).await?;
            let mut built = Vec::new();
            for e in &explicit {
                let Some(layer) = layers
                    .iter()
                    .find(|l| l.posting_id == e.consumed_posting_id)
                else {
                    return Err(Error::from_posting(PostingError::IneligibleLayer {
                        consuming: *handle,
                        consumed: e.consumed_posting_id,
                    }));
                };
                check_explicit_layer(*handle, q, layer, e.quantity.amount.abs())
                    .map_err(Error::from_posting)?;
                built.push(AllocationEdge {
                    consuming: *handle,
                    consumed: e.consumed_posting_id,
                    quantity: e.quantity,
                    amount: e.amount,
                });
            }
            if !explicit_matches(q, value_sum, &built) {
                return Err(Error::from_posting(PostingError::AllocationMismatch(
                    *handle,
                )));
            }
            edges.extend(built);
        } else {
            let stock = load_stock_item(tx, q.item).await?;
            let layers = load_open_layers(tx, q.item, q.location).await?;
            let (built, extra) =
                allocate_withdrawal(*handle, q, &stock, &layers).map_err(Error::from_posting)?;
            edges.extend(built);
            for ExtraValue { posting } in extra {
                extra_values.push((PostingHandle(u32::MAX), posting));
            }
        }
    }

    if snap.kind == GroupKind::Transformation {
        for (handle, q) in &snap.quantities {
            if needs_lineage(snap.kind, q) {
                let has = snap.consumptions.iter().any(|c| c.consuming == *handle)
                    || edges.iter().any(|e| e.consuming == *handle);
                if !has {
                    return Err(Error::from_posting(PostingError::LineageRequired(*handle)));
                }
            }
        }
    }
    for cons in explicit_by_handle.into_values().flatten() {
        edges.push(AllocationEdge {
            consuming: cons.consuming,
            consumed: cons.consumed_posting_id,
            quantity: cons.quantity,
            amount: cons.amount,
        });
    }

    for (_h, v) in &extra_values {
        insert_value(tx, group_id, snap.kind, v, &handle_to_id).await?;
    }

    for e in &edges {
        let consuming_id = *handle_to_id
            .get(&e.consuming.0)
            .ok_or_else(|| Error::from_posting(PostingError::UnknownHandle(e.consuming)))?;
        insert_consumption(tx, group_id, consuming_id, e).await?;
    }

    crate::projections::apply_group(tx, group_id).await?;
    Ok(group_id)
}

async fn load_reverses_kind(tx: &mut Tx<'_>, rid: Identifier) -> Result<String> {
    let row: Option<(String,)> = tx
        .fetch_optional(
            sqlx::query_as("SELECT kind::text FROM ledger.posting_group WHERE group_id = $1")
                .bind(rid.as_uuid()),
        )
        .await?;
    let Some((kind,)) = row else {
        return Err(Error::UnknownGroup);
    };
    if group_kind_from_sql(&kind)? == GroupKind::Reversal {
        return Err(Error::from_posting(PostingError::Shape(
            "cannot reverse a reversal".into(),
        )));
    }
    Ok(kind)
}

async fn insert_quantity(
    tx: &mut Tx<'_>,
    group_id: Identifier,
    kind: GroupKind,
    q: &QuantityPosting,
) -> Result<i64> {
    let stock = load_stock_item(tx, q.item).await?;
    if q.quantity.unit != stock.stock_uom {
        return Err(Error::from_posting(PostingError::Shape(
            "quantity unit must be the item stock unit".into(),
        )));
    }
    let entered_qty = q.entered.map(|e| e.amount);
    let entered_uom = q.entered.map(|e| e.unit.0);
    let row: (i64,) = tx
        .fetch_one(
            sqlx::query_as(
                "INSERT INTO ledger.posting (
                     group_id, kind, measure,
                     item_id, uom_id, stock_scale, residual_tolerance,
                     location_id, boundary, lot_id, serial_id, quantity,
                     entered_quantity, entered_uom_id
                 ) VALUES (
                     $1, $2::ledger.group_kind, 'QUANTITY',
                     $3, $4, $5, $6,
                     $7, $8::ledger.boundary, $9, $10, $11,
                     $12, $13
                 ) RETURNING posting_id",
            )
            .bind(group_id.as_uuid())
            .bind(group_kind_sql(kind)?)
            .bind(q.item.as_uuid())
            .bind(q.quantity.unit.0)
            .bind(stock.stock_scale)
            .bind(stock.residual_tolerance)
            .bind(q.location.as_uuid())
            .bind(q.boundary.map(boundary_sql).transpose()?)
            .bind(q.lot.map(|l| l.as_uuid()))
            .bind(q.serial.map(|s| s.as_uuid()))
            .bind(q.quantity.amount)
            .bind(entered_qty)
            .bind(entered_uom),
        )
        .await?;
    Ok(row.0)
}

async fn insert_value(
    tx: &mut Tx<'_>,
    group_id: Identifier,
    kind: GroupKind,
    v: &ValuePosting,
    handles: &HashMap<u32, i64>,
) -> Result<i64> {
    let values_id = match v.values {
        Some(h) => Some(
            *handles
                .get(&h.0)
                .ok_or_else(|| Error::from_posting(PostingError::UnknownHandle(h)))?,
        ),
        None => None,
    };
    let row: (i64,) = tx
        .fetch_one(
            sqlx::query_as(
                "INSERT INTO ledger.posting (
                     group_id, kind, measure,
                     account, cost_element, cost_object_id, currency_id, amount,
                     values_posting_id
                 ) VALUES (
                     $1, $2::ledger.group_kind, 'VALUE',
                     $3::ledger.value_account, $4::ledger.cost_element, $5, $6, $7,
                     $8
                 ) RETURNING posting_id",
            )
            .bind(group_id.as_uuid())
            .bind(group_kind_sql(kind)?)
            .bind(value_account_sql(v.account)?)
            .bind(cost_element_sql(v.cost_element)?)
            .bind(v.cost_object.map(|i| i.as_uuid()))
            .bind(v.amount.currency().0 as i16)
            .bind(v.amount.amount())
            .bind(values_id),
        )
        .await?;
    Ok(row.0)
}

async fn insert_consumption(
    tx: &mut Tx<'_>,
    group_id: Identifier,
    consuming_id: i64,
    e: &AllocationEdge,
) -> Result<()> {
    tx.execute(
        sqlx::query(
            "INSERT INTO ledger.consumption (
                 consuming_posting_id, consumed_posting_id, group_id, quantity, amount
             ) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(consuming_id)
        .bind(e.consumed.0)
        .bind(group_id.as_uuid())
        .bind(e.quantity.amount)
        .bind(e.amount.amount()),
    )
    .await?;
    Ok(())
}

fn sum_values_for(
    handle: PostingHandle,
    values: &[(PostingHandle, ValuePosting)],
) -> Result<Money> {
    let mut acc: Option<Money> = None;
    for (h, v) in values {
        if v.values == Some(handle) {
            acc = Some(match acc {
                Some(a) => a.try_add(v.amount).map_err(datum_core::Error::from)?,
                None => v.amount,
            });
        }
        let _ = h;
    }
    Ok(acc.unwrap_or_else(|| Money::zero(datum_core::CurrencyId(840))))
}
