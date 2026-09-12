//! Cross-dimension conversion must not compile.

use datum_core::{
    ConversionContext, ItemId, LengthDim, MassDim, Quantity, UnitCatalog, UnitConverter, UnitId,
    UnitRef,
};
use datum_uom::UomCatalog;

fn cross_dimension_is_unrepresentable(catalog: &UomCatalog) {
    let ctx = ConversionContext {
        item: ItemId::generate(),
        lot: None,
    };
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), catalog.dimension_of(UnitId(3)).unwrap())
        .unwrap();
    let lb = UnitRef::<MassDim>::checked(UnitId(6), catalog.dimension_of(UnitId(6)).unwrap())
        .unwrap();
    let qty = Quantity::new(rust_decimal::Decimal::ONE, inch).unwrap();
    let _ = catalog.convert(qty, lb, &ctx);
}

fn main() {}
