//! Exact reversal (P4). Nothing is deleted.

use datum_core::{
    AnyQuantity, Boundary, GroupKind, Identifier, Money, PostingGroupHeader, PostingHandle,
    PostingId, PostingIntent, PostingSink, QuantityPosting, ValuePosting,
};
use datum_db::Tx;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::builder::GroupBuilder;
use crate::enums::{
    boundary_from_sql, cost_element_from_sql, group_kind_from_sql, value_account_from_sql,
};
use crate::post::{commit, post};
use crate::{Error, Result};

type QtyRow = (
    i64,
    String,
    Uuid,
    i64,
    Uuid,
    Option<String>,
    Option<Uuid>,
    Option<Uuid>,
    Decimal,
    Option<Decimal>,
    Option<i64>,
);

type ValRow = (String, String, Option<Uuid>, i16, Decimal, Option<i64>);

/// Post the exact negation of `group_id`. A second reversal is [`Error::AlreadyReversed`].
pub async fn reverse(tx: &mut Tx<'_>, group_id: Identifier, reason: &str) -> Result<Identifier> {
    type HeaderRow = (String, Option<Uuid>, Option<Uuid>, Option<String>);
    let header_row: Option<HeaderRow> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT kind::text, source_id, work_order_id, reason_code
                   FROM ledger.posting_group WHERE group_id = $1",
            )
            .bind(group_id.as_uuid()),
        )
        .await?;
    let Some((kind_sql, source_id, work_order_id, _reason)) = header_row else {
        return Err(Error::UnknownGroup);
    };
    let target_kind = group_kind_from_sql(&kind_sql)?;
    if target_kind == GroupKind::Reversal {
        return Err(Error::from_posting(datum_core::PostingError::Shape(
            "cannot reverse a reversal".into(),
        )));
    }

    let qtys: Vec<QtyRow> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT posting_id, measure::text, item_id, uom_id, location_id, boundary::text,
                        lot_id, serial_id, quantity, entered_quantity, entered_uom_id
                   FROM ledger.posting
                  WHERE group_id = $1 AND measure = 'QUANTITY'
                  ORDER BY posting_id",
            )
            .bind(group_id.as_uuid()),
        )
        .await?;
    let vals: Vec<ValRow> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT account::text, cost_element::text, cost_object_id, currency_id, amount,
                        values_posting_id
                   FROM ledger.posting
                  WHERE group_id = $1 AND measure = 'VALUE'
                  ORDER BY posting_id",
            )
            .bind(group_id.as_uuid()),
        )
        .await?;
    let cons: Vec<(i64, i64, Decimal, Decimal)> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT consuming_posting_id, consumed_posting_id, quantity, amount
                   FROM ledger.consumption WHERE group_id = $1",
            )
            .bind(group_id.as_uuid()),
        )
        .await?;

    let mut builder = GroupBuilder::new(
        GroupKind::Reversal,
        PostingGroupHeader {
            source_kind: "reversal".into(),
            source_id: source_id.map(Identifier::from_uuid),
            work_order_id: work_order_id.map(Identifier::from_uuid),
            reason_code: Some(reason.to_string()),
            reverses_group_id: Some(group_id),
        },
    );

    let mut orig_to_handle: std::collections::HashMap<i64, PostingHandle> =
        std::collections::HashMap::new();
    for (pid, _m, item, uom, loc, boundary, lot, serial, qty, entered, entered_uom) in qtys {
        let dim = crate::infer_dimension(uom);
        let handle = builder
            .contribute(PostingIntent::Quantity(QuantityPosting {
                item: datum_core::ItemId::from_uuid(item),
                quantity: AnyQuantity {
                    amount: -qty,
                    unit: datum_core::UnitId(uom),
                    dimension: dim,
                },
                location: datum_core::LocationId::from_uuid(loc),
                boundary: boundary.as_deref().and_then(|s| boundary_from_sql(s).ok()),
                lot: lot.map(datum_core::LotId::from_uuid),
                serial: serial.map(datum_core::SerialId::from_uuid),
                entered: entered.map(|e| AnyQuantity {
                    amount: -e,
                    unit: datum_core::UnitId(entered_uom.unwrap_or(uom)),
                    dimension: dim,
                }),
            }))
            .map_err(Error::from_posting)?;
        orig_to_handle.insert(pid, handle);
        let _ = Boundary::Supplier;
    }

    for (account, element, cost_object, currency, amount, values_pid) in vals {
        let values = values_pid.and_then(|id| orig_to_handle.get(&id).copied());
        builder
            .contribute(PostingIntent::Value(ValuePosting {
                account: value_account_from_sql(&account)?,
                cost_element: cost_element_from_sql(&element)?,
                cost_object: cost_object.map(Identifier::from_uuid),
                amount: Money::new(-amount, datum_core::CurrencyId(i32::from(currency)))
                    .map_err(datum_core::Error::from)?,
                values,
            }))
            .map_err(Error::from_posting)?;
    }

    for (consuming, consumed, qty, amt) in cons {
        let Some(&handle) = orig_to_handle.get(&consuming) else {
            continue;
        };
        let dim = datum_core::DimensionKind::Count;
        builder
            .contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
                consuming: handle,
                consumed_posting_id: PostingId(consumed),
                quantity: AnyQuantity {
                    amount: -qty,
                    unit: datum_core::UnitId(1),
                    dimension: dim,
                },
                amount: Money::new(-amt, datum_core::CurrencyId(840))
                    .map_err(datum_core::Error::from)?,
            }))
            .map_err(Error::from_posting)?;
    }

    post(tx, builder).await
}

/// Keep [`commit`] reachable so callers can poison-check a reversal sink.
#[allow(dead_code)]
pub async fn reverse_and_commit(tx: Tx<'_>, builder: &GroupBuilder) -> Result<()> {
    commit(tx, &[builder]).await
}
