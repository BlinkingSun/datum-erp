use datum_core::*;
use rust_decimal::Decimal;

fn main() {
    let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).expect("count");
    let qty = Quantity::new(Decimal::from(1), unit).expect("qty");
    let money = Money::new(Decimal::from(1), CurrencyId(840)).expect("money");
    let _ = money + qty;
}
