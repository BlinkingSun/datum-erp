use wicket_core::*;
use rust_decimal::Decimal;

fn main() {
    let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).expect("count");
    let converted = Converted::<CountDim>::new(Decimal::from(1), unit, 0);
    let _ = converted.value;
}
