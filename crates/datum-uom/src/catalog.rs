//! Per-transaction unit catalog and conversion engine.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use datum_core::{
    ConversionContext, Converted, Dimension, DimensionKind, ItemId, LotId, Quantity, QuantityError,
    UnitCatalog, UnitConverter, UnitId, UnitRef,
};
use datum_db::Tx;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::Result;
use crate::policy::{Operation, rounding_for, rule_from_db};

#[derive(Debug, Clone)]
struct UnitRow {
    dimension: DimensionKind,
    scale_default: u32,
}

#[derive(Debug, Clone)]
struct ItemStockRow {
    stock_unit: UnitId,
    stock_scale: u32,
}

#[derive(Debug, Clone)]
struct FactorRow {
    from: UnitId,
    to: UnitId,
    item: Option<ItemId>,
    lot: Option<LotId>,
    numerator: Decimal,
    denominator: Decimal,
}

struct ResolvedFactor<'a> {
    row: &'a FactorRow,
    inverse: bool,
}

impl ResolvedFactor<'_> {
    fn apply(&self, amount: Decimal) -> (Decimal, Decimal) {
        if self.inverse {
            let factor = self.row.denominator / self.row.numerator;
            ((amount * self.row.denominator) / self.row.numerator, factor)
        } else {
            let factor = self.row.numerator / self.row.denominator;
            ((amount * self.row.numerator) / self.row.denominator, factor)
        }
    }
}

type FactorDbRow = (i64, i64, Option<Uuid>, Option<Uuid>, Decimal, Decimal);

/// Cached catalog state for one database transaction.
#[derive(Debug, Clone)]
pub struct UomCatalog {
    units: HashMap<UnitId, UnitRow>,
    item_stock: HashMap<ItemId, ItemStockRow>,
    factors: Vec<FactorRow>,
    policies: HashMap<(ItemId, UnitId, Operation), datum_core::Rounding>,
    as_of: DateTime<Utc>,
}

impl UomCatalog {
    /// Transaction timestamp used for effectivity (same as load time).
    pub fn as_of(&self) -> DateTime<Utc> {
        self.as_of
    }

    /// Stock unit and scale declared for an item, if any.
    pub fn stock_for(&self, item: ItemId) -> Option<(UnitId, u32)> {
        self.item_stock
            .get(&item)
            .map(|s| (s.stock_unit, s.stock_scale))
    }

    fn unit_row(&self, unit: UnitId) -> Result<&UnitRow> {
        self.units.get(&unit).ok_or(crate::Error::UnknownUnit(unit))
    }

    fn find_factor(
        &self,
        from: UnitId,
        to: UnitId,
        ctx: &ConversionContext,
    ) -> core::result::Result<ResolvedFactor<'_>, QuantityError> {
        // kind 0 = stored row in query direction; kind 1 = reciprocal of opposite-direction row.
        let mut best: Option<(i32, i32, &FactorRow)> = None;
        for f in &self.factors {
            let kind = if f.from == from && f.to == to {
                0
            } else if f.from == to && f.to == from {
                1
            } else {
                continue;
            };
            let Some(rank) = factor_rank(f, ctx) else {
                continue;
            };
            let better = match best {
                None => true,
                Some((br, bk, _)) => rank < br || (rank == br && kind < bk),
            };
            if better {
                best = Some((rank, kind, f));
            }
        }
        best.map(|(_, kind, f)| ResolvedFactor {
            row: f,
            inverse: kind == 1,
        })
        .ok_or(QuantityError::NoConversionPath {
            from,
            to,
            item: ctx.item,
        })
    }

    /// Resolve rounding policy for an item and unit.
    pub fn rounding_rule(
        &self,
        item: ItemId,
        unit: UnitId,
        op: Operation,
        default: datum_core::Rounding,
    ) -> datum_core::Rounding {
        rounding_for(&self.policies, item, unit, op, default)
    }

    /// Multiply `amount` in `from` into `to` using pinned factors at load time.
    pub fn convert_amount(
        &self,
        amount: Decimal,
        from: UnitId,
        to: UnitId,
        ctx: &ConversionContext,
    ) -> core::result::Result<(Decimal, Decimal), QuantityError> {
        if from == to {
            return Ok((amount, Decimal::ONE));
        }
        let f = self.find_factor(from, to, ctx)?;
        Ok(f.apply(amount))
    }
}

/// Lookup order: lot-scoped, then item-scoped, then global.
///
/// A lot-scoped row matches only when both lot and item agree. `None == None` is
/// not a lot match — that would give item-scoped and global rows the same rank.
fn factor_rank(f: &FactorRow, ctx: &ConversionContext) -> Option<i32> {
    match (f.lot, ctx.lot) {
        (Some(factor_lot), Some(ctx_lot)) if factor_lot == ctx_lot && f.item == Some(ctx.item) => {
            Some(0)
        }
        (Some(_), _) => None,
        (None, _) if f.item == Some(ctx.item) => Some(1),
        (None, _) if f.item.is_some() => None,
        (None, _) => Some(2),
    }
}

/// Load units, factors effective at the transaction time, policies, and item stock rows.
pub async fn load_catalog(tx: &mut Tx<'_>) -> Result<UomCatalog> {
    let (as_of_ts,): (DateTime<Utc>,) = tx
        .fetch_one(sqlx::query_as("SELECT pg_catalog.transaction_timestamp()"))
        .await?;

    let unit_rows: Vec<(i64, String, i16)> = tx
        .fetch_all(sqlx::query_as(
            "SELECT id, dimension, scale_default FROM uom.unit ORDER BY id",
        ))
        .await?;

    let mut units = HashMap::new();
    for (id, dim, scale) in unit_rows {
        let dimension = dimension_from_db(&dim).ok_or_else(|| {
            crate::Error::Core(datum_core::Error::Invariant(format!(
                "unknown dimension {dim}"
            )))
        })?;
        units.insert(
            UnitId(id),
            UnitRow {
                dimension,
                scale_default: u32::from(scale as u16),
            },
        );
    }

    let stock_rows: Vec<(Uuid, i64, i16)> = tx
        .fetch_all(sqlx::query_as(
            "SELECT item_id, stock_unit_id, stock_scale FROM uom.item_stock",
        ))
        .await?;

    let mut item_stock = HashMap::new();
    for (item, unit, scale) in stock_rows {
        item_stock.insert(
            ItemId::from_uuid(item),
            ItemStockRow {
                stock_unit: UnitId(unit),
                stock_scale: u32::from(scale as u16),
            },
        );
    }

    let factors: Vec<FactorRow> = tx
        .fetch_all(
            sqlx::query_as::<_, FactorDbRow>(
                r#"
            SELECT from_unit, to_unit, item_id, lot_id, numerator, denominator
              FROM uom.factor
             WHERE effective_from <= $1
               AND (effective_to IS NULL OR effective_to > $1)
            "#,
            )
            .bind(as_of_ts),
        )
        .await?
        .into_iter()
        .map(|(from, to, item, lot, num, den)| FactorRow {
            from: UnitId(from),
            to: UnitId(to),
            item: item.map(ItemId::from_uuid),
            lot: lot.map(LotId::from_uuid),
            numerator: num,
            denominator: den,
        })
        .collect();

    let policy_rows: Vec<(Uuid, i64, String, String)> = tx
        .fetch_all(sqlx::query_as(
            "SELECT item_id, unit_id, operation, rule FROM uom.rounding_policy",
        ))
        .await?;

    let mut policies = HashMap::new();
    for (item, unit, op, rule) in policy_rows {
        let op = match op.as_str() {
            "stock" => Operation::Stock,
            "convert" => Operation::Convert,
            _ => continue,
        };
        if let Some(r) = rule_from_db(&rule) {
            policies.insert((ItemId::from_uuid(item), UnitId(unit), op), r);
        }
    }

    Ok(UomCatalog {
        units,
        item_stock,
        factors,
        policies,
        as_of: as_of_ts,
    })
}

fn dimension_from_db(s: &str) -> Option<DimensionKind> {
    match s {
        "Count" => Some(DimensionKind::Count),
        "Length" => Some(DimensionKind::Length),
        "Mass" => Some(DimensionKind::Mass),
        "Time" => Some(DimensionKind::Time),
        "Volume" => Some(DimensionKind::Volume),
        "Area" => Some(DimensionKind::Area),
        _ => None,
    }
}

impl UnitCatalog for UomCatalog {
    fn dimension_of(&self, unit: UnitId) -> core::result::Result<DimensionKind, QuantityError> {
        self.unit_row(unit)
            .map(|r| r.dimension)
            .map_err(|_| QuantityError::UnknownUnit(unit))
    }

    fn scale_of(
        &self,
        unit: UnitId,
        ctx: &ConversionContext,
    ) -> core::result::Result<u32, QuantityError> {
        if let Some(stock) = self.item_stock.get(&ctx.item)
            && stock.stock_unit == unit
        {
            return Ok(stock.stock_scale);
        }
        self.unit_row(unit)
            .map(|r| r.scale_default)
            .map_err(|_| QuantityError::UnknownUnit(unit))
    }
}

impl UnitConverter for UomCatalog {
    fn convert<D: Dimension>(
        &self,
        qty: Quantity<D>,
        to: UnitRef<D>,
        ctx: &ConversionContext,
    ) -> core::result::Result<Converted<D>, QuantityError> {
        let from = qty.unit_id();
        let scale = self.scale_of(to.id(), ctx)?;
        let (converted, _) = self.convert_amount(qty.amount(), from, to.id(), ctx)?;
        Ok(Converted::new(converted, to, scale))
    }
}

/// Split a converted amount using per-item convert rounding policy.
pub fn split_with_policy<D: Dimension>(
    catalog: &UomCatalog,
    converted: Converted<D>,
    ctx: &ConversionContext,
    unit: UnitId,
) -> (Quantity<D>, Quantity<D>) {
    let rule = rounding_for(
        &catalog.policies,
        ctx.item,
        unit,
        Operation::Convert,
        datum_core::Rounding::HalfEven,
    );
    crate::policy::apply_rounding_policy(converted, rule)
}
