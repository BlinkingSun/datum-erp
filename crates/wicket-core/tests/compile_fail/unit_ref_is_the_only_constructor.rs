use wicket_core::*;
use rust_decimal::Decimal;

fn main() {
    let _ = Quantity::<CountDim>::new(Decimal::from(1), UnitId(1));
}
