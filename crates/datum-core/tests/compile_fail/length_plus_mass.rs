use datum_core::*;
use rust_decimal::Decimal;

fn main() {
    let length_unit =
        UnitRef::<LengthDim>::checked(UnitId(1), DimensionKind::Length).expect("length");
    let mass_unit = UnitRef::<MassDim>::checked(UnitId(2), DimensionKind::Mass).expect("mass");
    let length = Quantity::new(Decimal::from(1), length_unit).expect("qty");
    let mass = Quantity::new(Decimal::from(1), mass_unit).expect("qty");
    let _ = length.try_add(mass);
}
