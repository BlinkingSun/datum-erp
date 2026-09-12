//! Published writers for `ledger.stock_item` and `ledger.location`.
//!
//! Modules write these registries through this interface, never by touching the
//! tables directly (SPEC Owns).

use datum_core::{Boundary, CurrencyId, Identifier, ItemId, LocationId, Money, UnitId};
use datum_db::Tx;
use rust_decimal::Decimal;

use crate::enums::{CostMethod, boundary_sql, cost_method_sql};
use crate::{Error, GroupId, Result};

/// Insert or replace an item's stock measure and cost method.
pub async fn upsert_stock_item(
    tx: &mut Tx<'_>,
    item: ItemId,
    stock_uom: UnitId,
    stock_scale: i16,
    residual_tolerance: Decimal,
    cost_method: CostMethod,
    standard: Option<Money>,
) -> Result<()> {
    let (standard_cost, standard_currency) = match standard {
        Some(m) => (Some(m.amount()), Some(m.currency().0 as i16)),
        None => (None, None),
    };
    tx.execute(
        sqlx::query(
            "INSERT INTO ledger.stock_item (
                 item_id, stock_uom_id, stock_scale, residual_tolerance,
                 cost_method, standard_cost, standard_currency
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (item_id) DO UPDATE SET
                 stock_uom_id = EXCLUDED.stock_uom_id,
                 stock_scale = EXCLUDED.stock_scale,
                 residual_tolerance = EXCLUDED.residual_tolerance,
                 cost_method = EXCLUDED.cost_method,
                 standard_cost = EXCLUDED.standard_cost,
                 standard_currency = EXCLUDED.standard_currency",
        )
        .bind(item.as_uuid())
        .bind(stock_uom.0)
        .bind(stock_scale)
        .bind(residual_tolerance)
        .bind(cost_method_sql(cost_method))
        .bind(standard_cost)
        .bind(standard_currency),
    )
    .await?;
    Ok(())
}

/// Insert a location. `boundary` is `None` for a real, owned, valued place.
pub async fn upsert_location(
    tx: &mut Tx<'_>,
    location: LocationId,
    boundary: Option<Boundary>,
) -> Result<()> {
    let class = match boundary {
        Some(b) => Some(boundary_sql(b)?),
        None => None,
    };
    tx.execute(
        sqlx::query(
            "INSERT INTO ledger.location (location_id, boundary_class)
             VALUES ($1, $2::ledger.boundary)
             ON CONFLICT (location_id) DO UPDATE SET
                 boundary_class = EXCLUDED.boundary_class",
        )
        .bind(location.as_uuid())
        .bind(class),
    )
    .await?;
    Ok(())
}

/// Row loaded from `ledger.stock_item` at posting time.
#[derive(Debug, Clone)]
pub struct StockItem {
    /// Item.
    pub item: ItemId,
    /// Canonical stock unit.
    pub stock_uom: UnitId,
    /// Declared scale (0–8).
    pub stock_scale: i16,
    /// Dust bound.
    pub residual_tolerance: Decimal,
    /// Cost method.
    pub cost_method: CostMethod,
    /// Standard unit cost when [`CostMethod::Standard`].
    pub standard: Option<Money>,
}

/// Whether `item_id` has any row in `ledger.posting`.
///
/// D2 R5: stock unit / scale / tolerance are immutable while this is true.
/// Reads through the ledger-schema SQL helper so a module never selects
/// `ledger.posting` itself. Requires a sealed [`Tx`] (no actor → SQLSTATE
/// `42501`).
pub async fn has_postings(tx: &mut Tx<'_>, item_id: ItemId) -> Result<bool> {
    let row: (bool,) = tx
        .fetch_one(sqlx::query_as("SELECT ledger.has_postings($1)").bind(item_id.as_uuid()))
        .await?;
    Ok(row.0)
}

/// Child posting groups whose `parent_group_id` is `group_id` (R-2s-6).
///
/// Reads through `ledger.children_of(uuid)`. Requires a sealed [`Tx`]
/// (no actor → SQLSTATE `42501`).
pub async fn children_of(tx: &mut Tx<'_>, group_id: GroupId) -> Result<Vec<GroupId>> {
    let rows: Vec<(uuid::Uuid,)> = tx
        .fetch_all(sqlx::query_as("SELECT * FROM ledger.children_of($1)").bind(group_id.as_uuid()))
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id,)| Identifier::from_uuid(id))
        .collect())
}

/// Whether any quantity slice at `location_id` currently nets above zero.
///
/// Folds `ledger.posting` (the ledger-owned source of truth), not a module
/// read of `transient.balance_projection`. Requires a sealed [`Tx`]
/// (no actor → SQLSTATE `42501`).
pub async fn has_quantity_at(tx: &mut Tx<'_>, location_id: LocationId) -> Result<bool> {
    let row: (bool,) = tx
        .fetch_one(sqlx::query_as("SELECT ledger.has_quantity_at($1)").bind(location_id.as_uuid()))
        .await?;
    Ok(row.0)
}

/// Load a stock-item registry row.
pub async fn load_stock_item(tx: &mut Tx<'_>, item: ItemId) -> Result<StockItem> {
    type StockRow = (i64, i16, Decimal, String, Option<Decimal>, Option<i16>);
    let row: Option<StockRow> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT stock_uom_id, stock_scale, residual_tolerance, cost_method,
                    standard_cost, standard_currency
               FROM ledger.stock_item WHERE item_id = $1",
            )
            .bind(item.as_uuid()),
        )
        .await?;
    let Some((uom, scale, tol, method, sc, scur)) = row else {
        return Err(Error::UnknownRegistry(format!("stock_item {}", item)));
    };
    let cost_method = crate::enums::cost_method_from_sql(&method)
        .ok_or_else(|| Error::UnknownRegistry(format!("cost_method {method}")))?;
    let standard = match (sc, scur) {
        (Some(amount), Some(cur)) => {
            Some(Money::new(amount, CurrencyId(i32::from(cur))).map_err(datum_core::Error::from)?)
        }
        _ => None,
    };
    Ok(StockItem {
        item,
        stock_uom: UnitId(uom),
        stock_scale: scale,
        residual_tolerance: tol,
        cost_method,
        standard,
    })
}
