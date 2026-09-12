//! Ledger boundary conversion: canonical stock quantity plus residual.

use datum_core::{
    AnyQuantity, ConversionContext, Dimension, ItemId, Quantity, QuantityError, Rounding,
    UnitCatalog,
};
use datum_db::Tx;
use rust_decimal::Decimal;

use crate::Result;
use crate::catalog::UomCatalog;
use crate::policy::{Operation, apply_rounding_policy};

/// Result of converting entered quantity to an item's stock unit at the boundary (D2 R2/R4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockConversion<D: Dimension> {
    /// Canonical quantity in the item stock unit at `stock_scale`.
    pub canonical: Quantity<D>,
    /// What the operator entered (provenance).
    pub entered: AnyQuantity,
    /// Factor applied (`entered * factor = unrounded canonical`).
    pub factor: Decimal,
    /// Rounding residual in the stock unit (caller posts as `UOM_CONVERSION_RESIDUAL`).
    pub residual: Quantity<D>,
}

/// Convert `entered` to the item's stock unit using half-even at stock scale (unless policy overrides).
pub async fn to_stock<D: Dimension>(
    tx: &mut Tx<'_>,
    _catalog: &UomCatalog,
    item: ItemId,
    entered: AnyQuantity,
    ctx: &ConversionContext,
) -> Result<StockConversion<D>> {
    let catalog = crate::load_catalog(tx).await?;
    let downcast = entered.downcast::<D>()?;
    let (stock_unit_id, stock_scale) = catalog.stock_for(item).ok_or_else(|| {
        crate::Error::Core(datum_core::Error::Quantity(
            QuantityError::NoConversionPath {
                from: entered.unit,
                to: entered.unit,
                item,
            },
        ))
    })?;

    let stock_unit = catalog.resolve::<D>(stock_unit_id)?;

    let mut ctx = *ctx;
    ctx.item = item;

    let (unrounded, factor) =
        catalog.convert_amount(downcast.amount(), downcast.unit_id(), stock_unit.id(), &ctx)?;
    let converted = datum_core::Converted::new(unrounded, stock_unit, stock_scale);

    let rule = catalog.rounding_rule(item, stock_unit.id(), Operation::Stock, Rounding::HalfEven);
    let (canonical, residual) = apply_rounding_policy(converted, rule);

    Ok(StockConversion {
        canonical,
        entered,
        factor,
        residual,
    })
}
