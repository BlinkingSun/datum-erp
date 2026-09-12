//! Pin lot-scoped conversion factors at receipt.

use chrono::{DateTime, Utc};
use datum_core::{LotId, UnitId};
use datum_db::Tx;
use rust_decimal::Decimal;

use crate::Result;

/// Write a lot-scoped factor with open effectivity (`effective_to` NULL).
pub async fn pin_lot_factor(
    tx: &mut Tx<'_>,
    item: datum_core::ItemId,
    lot: LotId,
    from: UnitId,
    to: UnitId,
    numerator: Decimal,
    denominator: Decimal,
) -> Result<()> {
    let (as_of,): (DateTime<Utc>,) = tx
        .fetch_one(sqlx::query_as("SELECT pg_catalog.transaction_timestamp()"))
        .await?;
    tx.execute(
        sqlx::query(
            r#"
            UPDATE uom.factor
               SET effective_to = $1
             WHERE from_unit = $2 AND to_unit = $3
               AND item_id = $4 AND lot_id = $5
               AND effective_to IS NULL
            "#,
        )
        .bind(as_of)
        .bind(from.0)
        .bind(to.0)
        .bind(item.as_uuid())
        .bind(lot.as_uuid()),
    )
    .await?;
    tx.execute(
        sqlx::query(
            r#"
            INSERT INTO uom.factor (
              from_unit, to_unit, item_id, lot_id,
              numerator, denominator, effective_from, effective_to
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)
            "#,
        )
        .bind(from.0)
        .bind(to.0)
        .bind(item.as_uuid())
        .bind(lot.as_uuid())
        .bind(numerator)
        .bind(denominator)
        .bind(as_of),
    )
    .await?;
    Ok(())
}
