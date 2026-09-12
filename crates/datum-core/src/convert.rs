//! Conversion traits. Implementation lives in `datum-uom`.

use crate::id::{ItemId, LotId};
use crate::quantity::{Quantity, QuantityError};
use crate::residual::Converted;
use crate::units::{Dimension, DimensionKind, UnitId, UnitRef};

/// Item and lot context, because a conversion factor in this domain is item data and
/// sometimes lot data (bar stock lb-per-ft varies by heat lot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversionContext {
    /// Item whose conversion rules apply.
    pub item: ItemId,
    /// Lot when the factor varies by heat lot; `None` for item-level factors.
    pub lot: Option<LotId>,
}

/// Catalog of units. Implemented by `datum-uom`.
pub trait UnitCatalog {
    /// Dimension recorded on the unit master for `unit`.
    fn dimension_of(&self, unit: UnitId) -> Result<DimensionKind, QuantityError>;
    /// Decimal places this unit is tracked to for this item. Drives [`Converted::split`].
    fn scale_of(&self, unit: UnitId, ctx: &ConversionContext) -> Result<u32, QuantityError>;

    /// Resolve a raw id into a dimension-proven reference. Blanket-provided.
    fn resolve<D: Dimension>(&self, unit: UnitId) -> Result<UnitRef<D>, QuantityError> {
        UnitRef::<D>::checked(unit, self.dimension_of(unit)?)
    }
}

/// Converts a quantity within a dimension. Cross-dimension conversion is unwritable.
pub trait UnitConverter: UnitCatalog {
    /// Within a dimension only — the signature makes cross-dimension conversion
    /// unwritable. Returns [`Converted`], never a bare [`Quantity`].
    fn convert<D: Dimension>(
        &self,
        qty: Quantity<D>,
        to: UnitRef<D>,
        ctx: &ConversionContext,
    ) -> Result<Converted<D>, QuantityError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{CountDim, DimensionKind};

    struct OneToOne;

    impl UnitCatalog for OneToOne {
        fn dimension_of(&self, _unit: UnitId) -> Result<DimensionKind, QuantityError> {
            Ok(DimensionKind::Count)
        }
        fn scale_of(&self, _unit: UnitId, _ctx: &ConversionContext) -> Result<u32, QuantityError> {
            Ok(4)
        }
    }

    impl UnitConverter for OneToOne {
        fn convert<D: Dimension>(
            &self,
            qty: Quantity<D>,
            to: UnitRef<D>,
            _ctx: &ConversionContext,
        ) -> Result<Converted<D>, QuantityError> {
            Ok(Converted::new(qty.amount(), to, 4))
        }
    }

    #[test]
    fn unit_converter_returns_converted() {
        let from = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let to = UnitRef::<CountDim>::checked(UnitId(2), DimensionKind::Count).unwrap();
        let qty = Quantity::new(rust_decimal::Decimal::from(10), from).unwrap();
        let ctx = ConversionContext {
            item: ItemId::from_uuid(uuid::Uuid::nil()),
            lot: None,
        };
        let converted = OneToOne.convert(qty, to, &ctx).unwrap();
        let _ = converted.has_residual();
        let (value, residual) = converted.split(crate::residual::Rounding::HalfUp);
        let _ = value.try_add(residual).unwrap();
    }
}
