//! Property tests required by SPEC-core.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use proptest::prelude::*;
use rust_decimal::Decimal;
use wicket_core::*;

fn count_unit() -> UnitRef<CountDim> {
    UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap()
}

fn dec_scale8() -> impl Strategy<Value = Decimal> {
    (-10_000i64..=10_000, 0u32..=8).prop_map(|(n, s)| Decimal::new(n, s))
}

fn money_dec() -> impl Strategy<Value = Decimal> {
    (-10_000i64..=10_000, 0u32..=6).prop_map(|(n, s)| Decimal::new(n, s))
}

proptest! {
    #[test]
    fn try_add_commutative(a in dec_scale8(), b in dec_scale8()) {
        let unit = count_unit();
        let qa = Quantity::new(a, unit).unwrap();
        let qb = Quantity::new(b, unit).unwrap();
        let ab = qa.try_add(qb);
        let ba = qb.try_add(qa);
        match (ab, ba) {
            (Ok(x), Ok(y)) => prop_assert_eq!(x, y),
            (Err(e1), Err(e2)) => prop_assert_eq!(e1, e2),
            other => prop_assert!(false, "asymmetric: {:?}", other),
        }
    }

    #[test]
    fn try_add_associative(a in dec_scale8(), b in dec_scale8(), c in dec_scale8()) {
        let unit = count_unit();
        let qa = Quantity::new(a, unit).unwrap();
        let qb = Quantity::new(b, unit).unwrap();
        let qc = Quantity::new(c, unit).unwrap();
        let left = qa.try_add(qb).and_then(|s| s.try_add(qc));
        let right = qb.try_add(qc).and_then(|s| qa.try_add(s));
        match (left, right) {
            (Ok(x), Ok(y)) => prop_assert_eq!(x, y),
            (Err(e1), Err(e2)) => prop_assert_eq!(e1, e2),
            other => prop_assert!(false, "asymmetric: {:?}", other),
        }
    }

    #[test]
    fn negate_is_involution(a in dec_scale8()) {
        let unit = count_unit();
        let q = Quantity::new(a, unit).unwrap();
        prop_assert_eq!(q.negate().negate(), q);
    }

    #[test]
    fn split_reconstructs_original(a in dec_scale8(), factor in dec_scale8(), to_scale in 0u32..=8) {
        let unit = count_unit();
        let q = Quantity::new(a, unit).unwrap();
        let scaled = q.scale(factor, to_scale);
        if scaled.has_residual() || true {
            for rule in [
                Rounding::HalfUp,
                Rounding::HalfEven,
                Rounding::TowardZero,
                Rounding::AwayFromZero,
            ] {
                let original = q.amount().checked_mul(factor);
                let Some(exact) = original else { continue };
                let (v, r) = scaled.split(rule);
                prop_assert_eq!(v.try_add(r).unwrap().amount(), exact);
            }
        }
    }

    #[test]
    fn money_allocate_sums_and_zero_weights(
        amount in money_dec(),
        w0 in 0u64..100,
        w1 in 0u64..100,
        w2 in 0u64..100,
        scale in 0u32..=6,
    ) {
        let m = Money::new(amount, CurrencyId(840)).unwrap();
        let weights = [w0, w1, w2];
        match m.allocate(&weights, scale) {
            Ok(parts) => {
                let sum = Money::try_sum(parts).unwrap().unwrap();
                prop_assert_eq!(sum, m);
            }
            Err(MoneyError::EmptyAllocation) => {
                prop_assert_eq!(w0 + w1 + w2, 0);
            }
            Err(e) => prop_assert!(false, "unexpected {e:?}"),
        }
    }

    #[test]
    fn any_quantity_serde_roundtrip_scale8(n in -10_000i64..=10_000i64) {
        let amount = Decimal::new(n, 8);
        let q = AnyQuantity {
            amount,
            unit: UnitId(42),
            dimension: DimensionKind::Length,
        };
        let json = serde_json::to_string(&q).unwrap();
        prop_assert!(json.contains("\"amount\":\""));
        let back: AnyQuantity = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(back.amount, amount);
        prop_assert_eq!(back.amount.scale(), 8);
        prop_assert_eq!(back.unit, UnitId(42));
        prop_assert_eq!(back.dimension, DimensionKind::Length);
    }

    #[test]
    fn accepted_quantity_roundtrips_through_decimal_string(
        mantissa in -10i128.pow(24)..=10i128.pow(24),
        scale in 0u32..=8u32,
    ) {
        let Ok(amount) = Decimal::try_from_i128_with_scale(mantissa, scale) else {
            return Ok(());
        };
        let unit = count_unit();
        if let Ok(q) = Quantity::new(amount, unit) {
            let s = q.amount().to_string();
            let back: Decimal = s.parse().unwrap();
            prop_assert_eq!(back, q.amount());
            prop_assert_eq!(back.to_string(), s);
        }
    }

    #[test]
    fn identifier_display_fromstr_roundtrip(_n in 0u8..32) {
        let id = Identifier::generate();
        let s = id.to_string();
        let parsed: Identifier = s.parse().unwrap();
        prop_assert_eq!(id, parsed);
        let lower = s.to_ascii_lowercase();
        prop_assert_eq!(s, lower);
    }
}
