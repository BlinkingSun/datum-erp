//! `uom.item_stock` writes and R5 immutability while postings exist.
//!
//! `datum-uom` cannot depend on `datum-ledger` (CONTRACT §4 dependency graph:
//! `datum-ledger` → `datum-uom`). R5 postings presence uses the ledger query
//! seam `ledger.has_postings` on the caller's [`Tx`] (same source of truth as
//! `datum_ledger::has_postings`).

use datum_core::{ItemId, UnitId};
use datum_db::Tx;
use rust_decimal::Decimal;

use crate::{Error, Result};

/// Stock measure fields stored on `uom.item_stock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStockMeasure {
    /// Canonical stock unit.
    pub stock_unit: UnitId,
    /// Declared scale (0–8).
    pub stock_scale: i16,
    /// Dust bound.
    pub residual_tolerance: Decimal,
}

/// Update an item's stock measure when the ledger reports no postings (D2 R5).
pub async fn update_item_stock(
    tx: &mut Tx<'_>,
    item: ItemId,
    measure: ItemStockMeasure,
) -> Result<()> {
    if !(0..=8).contains(&measure.stock_scale) {
        return Err(Error::InvalidStockScale);
    }
    if measure.residual_tolerance < Decimal::ZERO {
        return Err(Error::InvalidResidualTolerance);
    }

    type Row = (i64, i16, Decimal);
    let current: Option<Row> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT stock_unit_id, stock_scale, residual_tolerance
                   FROM uom.item_stock WHERE item_id = $1",
            )
            .bind(item.as_uuid()),
        )
        .await?;
    let Some((unit, scale, tol)) = current else {
        return Err(Error::UnknownItemStock(item));
    };

    let measure_changed = measure.stock_unit.0 != unit
        || measure.stock_scale != scale
        || measure.residual_tolerance != tol;
    if measure_changed && ledger_has_postings(tx, item).await? {
        return Err(Error::StockMeasureImmutable);
    }

    let n = tx
        .execute(
            sqlx::query(
                "UPDATE uom.item_stock
                    SET stock_unit_id = $2,
                        stock_scale = $3,
                        residual_tolerance = $4
                  WHERE item_id = $1",
            )
            .bind(item.as_uuid())
            .bind(measure.stock_unit.0)
            .bind(measure.stock_scale)
            .bind(measure.residual_tolerance),
        )
        .await?;
    if n.rows_affected() != 1 {
        return Err(Error::UnknownItemStock(item));
    }
    Ok(())
}

async fn ledger_has_postings(tx: &mut Tx<'_>, item: ItemId) -> Result<bool> {
    let row: (bool,) = tx
        .fetch_one(sqlx::query_as("SELECT ledger.has_postings($1)").bind(item.as_uuid()))
        .await?;
    Ok(row.0)
}
