//! Integration tests for `datum-uom` (Postgres when `DATUM_REQUIRE_PG=1`).
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use datum_core::{
    AnyQuantity, ConversionContext, DimensionKind, ItemId, LengthDim, LotId, Quantity, Rounding,
    UnitCatalog, UnitConverter, UnitId, UnitRef,
};
use datum_db::WritePool;
use proptest::prelude::*;
use rust_decimal::Decimal;

use datum_uom::{
    MIGRATOR, apply_rounding_policy, load_catalog, pin_lot_factor, split_with_policy, to_stock,
};

fn dec(s: &str) -> Decimal {
    s.parse().expect("decimal")
}

#[tokio::test]
async fn convert_within_dimension_returns_converted_with_residual() {
    let db = datum_test::db_case!("convert_within");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.convert");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let catalog = load_catalog(&mut tx).await.unwrap();
    let item = ItemId::generate();
    common::insert_item_stock(&mut tx, item, UnitId(4), 4).await;
    let conv_ctx = ConversionContext { item, lot: None };
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let foot = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();
    let qty = Quantity::new(Decimal::from(7), inch).unwrap();
    let exact = (Decimal::from(7) * dec("1")) / dec("12");
    let converted = catalog.convert(qty, foot, &conv_ctx).unwrap();
    assert!(converted.has_residual());
    let (value, residual) = split_with_policy(&catalog, converted, &conv_ctx, foot.id());
    assert_eq!(value.amount(), dec("0.5833"));
    assert_eq!(
        value.try_add(residual).unwrap().amount(),
        exact,
        "split must reconstruct the converted amount"
    );
    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn effectivity_picks_the_factor_in_force() {
    let db = datum_test::db_case!("effectivity");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.effectivity");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let item = ItemId::generate();
    // Closed window [now-30d, now) at 1/10; open window [now, ∞) at 1/5.
    // `now()` is transaction_timestamp(), the same instant load_catalog uses, so
    // the boundary instant is in the second window ([from, to) is half-open).
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.factor (from_unit, to_unit, item_id, numerator, denominator, effective_from, effective_to)
             VALUES (3, 4, $1, 1, 10, pg_catalog.now() - interval '30 days', pg_catalog.now())",
        )
        .bind(item.as_uuid()),
    )
    .await
    .unwrap();
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.factor (from_unit, to_unit, item_id, numerator, denominator, effective_from)
             VALUES (3, 4, $1, 1, 5, pg_catalog.now())",
        )
        .bind(item.as_uuid()),
    )
    .await
    .unwrap();
    let catalog = load_catalog(&mut tx).await.unwrap();
    let conv_ctx = ConversionContext { item, lot: None };
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let foot = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();
    let qty = Quantity::new(Decimal::ONE, inch).unwrap();
    let converted = catalog.convert(qty, foot, &conv_ctx).unwrap();
    let (v, _) = split_with_policy(&catalog, converted, &conv_ctx, foot.id());
    // In-force 1/5 → 0.2000 at scale 4. Closed 1/10 would be 0.1000; global 1/12 is 0.0833.
    assert_eq!(v.amount(), dec("0.2000"));
    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn inverse_factor_is_inferred_exactly() {
    let db = datum_test::db_case!("inverse_factor");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.inverse");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let item = ItemId::generate();
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let foot = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();
    let conv_ctx = ConversionContext { item, lot: None };

    // Global seed IN→FT (1/12) implies FT→IN without a stored FT→IN row.
    let catalog = load_catalog(&mut tx).await.unwrap();
    let one_ft = Quantity::new(Decimal::ONE, foot).unwrap();
    let to_in = catalog.convert(one_ft, inch, &conv_ctx).unwrap();
    let (inches, _) = split_with_policy(&catalog, to_in, &conv_ctx, inch.id());
    assert_eq!(inches.amount(), Decimal::from(12));
    let back = catalog.convert(inches, foot, &conv_ctx).unwrap();
    let (ft_back, res) = split_with_policy(&catalog, back, &conv_ctx, foot.id());
    assert_eq!(ft_back.try_add(res).unwrap().amount(), Decimal::ONE);

    // Stored FT→IN beats the reciprocal implied from item-scoped IN→FT.
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.factor (from_unit, to_unit, item_id, numerator, denominator, effective_from)
             VALUES (3, 4, $1, 1, 12, '-infinity')",
        )
        .bind(item.as_uuid()),
    )
    .await
    .unwrap();
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.factor (from_unit, to_unit, item_id, numerator, denominator, effective_from)
             VALUES (4, 3, $1, 1, 10, '-infinity')",
        )
        .bind(item.as_uuid()),
    )
    .await
    .unwrap();
    let catalog = load_catalog(&mut tx).await.unwrap();
    let to_in_stored = catalog.convert(one_ft, inch, &conv_ctx).unwrap();
    let (inches_stored, _) = split_with_policy(&catalog, to_in_stored, &conv_ctx, inch.id());
    assert_eq!(
        inches_stored.amount(),
        dec("0.1000"),
        "explicit FT→IN 1/10 must beat implied 12/1 from IN→FT"
    );

    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn repin_closes_the_open_pin() {
    let db = datum_test::db_case!("repin");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.repin");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let item = ItemId::generate();
    let lot = LotId::generate();
    pin_lot_factor(
        &mut tx,
        item,
        lot,
        UnitId(3),
        UnitId(4),
        Decimal::ONE,
        Decimal::from(8),
    )
    .await
    .unwrap();
    let (first_to,): (Option<chrono::DateTime<chrono::Utc>>,) = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT effective_to FROM uom.factor WHERE lot_id = $1 ORDER BY id LIMIT 1",
            )
            .bind(lot.as_uuid()),
        )
        .await
        .unwrap();
    assert!(first_to.is_none());

    pin_lot_factor(
        &mut tx,
        item,
        lot,
        UnitId(3),
        UnitId(4),
        Decimal::ONE,
        Decimal::from(10),
    )
    .await
    .unwrap();

    let rows: Vec<(Option<chrono::DateTime<chrono::Utc>>, Decimal, Decimal)> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT effective_to, numerator, denominator FROM uom.factor WHERE lot_id = $1 ORDER BY id",
            )
            .bind(lot.as_uuid()),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "closed pin and new open pin");
    assert!(rows[0].0.is_some(), "first row closed at re-pin");
    assert!(rows[1].0.is_none());
    assert_eq!(rows[1].1, Decimal::ONE);
    assert_eq!(rows[1].2, Decimal::from(10));

    let catalog = load_catalog(&mut tx).await.unwrap();
    let lot_ctx = ConversionContext {
        item,
        lot: Some(lot),
    };
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let foot = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();
    let qty = Quantity::new(Decimal::from(10), inch).unwrap();
    let converted = catalog.convert(qty, foot, &lot_ctx).unwrap();
    let (v, _) = split_with_policy(&catalog, converted, &lot_ctx, foot.id());
    assert_eq!(v.amount(), Decimal::ONE, "active pin is 1/10");

    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_factor_beats_item_factor_beats_global() {
    let db = datum_test::db_case!("factor_rank");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.rank");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let item = ItemId::generate();
    let lot = LotId::generate();
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.factor (from_unit, to_unit, item_id, numerator, denominator, effective_from)
             VALUES (3, 4, $1, 1, 10, '-infinity')",
        )
        .bind(item.as_uuid()),
    )
    .await
    .unwrap();
    pin_lot_factor(
        &mut tx,
        item,
        lot,
        UnitId(3),
        UnitId(4),
        Decimal::ONE,
        Decimal::from(8),
    )
    .await
    .unwrap();
    let catalog = load_catalog(&mut tx).await.unwrap();
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let foot = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();

    // Item 1/10 beats seed global 1/12 when no lot is on the context.
    let item_ctx = ConversionContext { item, lot: None };
    let qty_ten = Quantity::new(Decimal::from(10), inch).unwrap();
    let converted = catalog.convert(qty_ten, foot, &item_ctx).unwrap();
    let (v, _) = split_with_policy(&catalog, converted, &item_ctx, foot.id());
    assert_eq!(
        v.amount(),
        Decimal::ONE,
        "item 1/10 must beat global 1/12 with lot=None"
    );

    // Lot 1/8 beats item 1/10 when the lot is in context.
    let lot_ctx = ConversionContext {
        item,
        lot: Some(lot),
    };
    let qty_eight = Quantity::new(Decimal::from(8), inch).unwrap();
    let converted = catalog.convert(qty_eight, foot, &lot_ctx).unwrap();
    let (v, _) = split_with_policy(&catalog, converted, &lot_ctx, foot.id());
    assert_eq!(v.amount(), Decimal::ONE, "lot 1/8 must beat item 1/10");

    // A different item with no item factor uses the global 1/12.
    let other = ItemId::generate();
    let global_ctx = ConversionContext {
        item: other,
        lot: None,
    };
    let qty_twelve = Quantity::new(Decimal::from(12), inch).unwrap();
    let converted = catalog.convert(qty_twelve, foot, &global_ctx).unwrap();
    let (v, _) = split_with_policy(&catalog, converted, &global_ctx, foot.id());
    assert_eq!(
        v.amount(),
        Decimal::ONE,
        "unscoped item must use global 1/12"
    );

    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn pin_then_to_stock_in_one_tx() {
    let db = datum_test::db_case!("pin_to_stock");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.pin_to_stock");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let item = ItemId::generate();
    let lot = LotId::generate();
    common::insert_item_stock(&mut tx, item, UnitId(4), 4).await;
    let stale = load_catalog(&mut tx).await.unwrap();
    pin_lot_factor(
        &mut tx,
        item,
        lot,
        UnitId(3),
        UnitId(4),
        Decimal::ONE,
        Decimal::from(8),
    )
    .await
    .unwrap();
    let lot_ctx = ConversionContext {
        item,
        lot: Some(lot),
    };
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let entered = AnyQuantity::from(Quantity::new(Decimal::from(8), inch).unwrap());
    let conv = to_stock::<LengthDim>(&mut tx, &stale, item, entered, &lot_ctx)
        .await
        .unwrap();
    assert_eq!(conv.canonical.amount(), Decimal::ONE);
    assert_eq!(conv.factor, dec("0.125"));
    assert!(conv.residual.amount().is_zero());
    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn to_stock_with_and_without_lot_pin() {
    let db = datum_test::db_case!("to_stock_lot");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.to_stock");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let item = ItemId::generate();
    let lot = LotId::generate();
    common::insert_item_stock(&mut tx, item, UnitId(4), 4).await;
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.factor (from_unit, to_unit, item_id, numerator, denominator, effective_from)
             VALUES (3, 4, $1, 1, 10, '-infinity')",
        )
        .bind(item.as_uuid()),
    )
    .await
    .unwrap();
    pin_lot_factor(
        &mut tx,
        item,
        lot,
        UnitId(3),
        UnitId(4),
        Decimal::ONE,
        Decimal::from(8),
    )
    .await
    .unwrap();
    let catalog = load_catalog(&mut tx).await.unwrap();
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();

    let item_ctx = ConversionContext { item, lot: None };
    let entered_item = AnyQuantity::from(Quantity::new(Decimal::from(10), inch).unwrap());
    let without_lot = to_stock::<LengthDim>(&mut tx, &catalog, item, entered_item, &item_ctx)
        .await
        .unwrap();
    assert_eq!(without_lot.canonical.amount(), Decimal::ONE);
    assert_eq!(without_lot.factor, dec("0.1"));

    let lot_ctx = ConversionContext {
        item,
        lot: Some(lot),
    };
    let entered_lot = AnyQuantity::from(Quantity::new(Decimal::from(8), inch).unwrap());
    let with_lot = to_stock::<LengthDim>(&mut tx, &catalog, item, entered_lot, &lot_ctx)
        .await
        .unwrap();
    assert_eq!(with_lot.canonical.amount(), Decimal::ONE);
    assert_eq!(with_lot.factor, dec("0.125"));

    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn seeded_units_have_right_dimensions() {
    let db = datum_test::db_case!("seed_dims");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.seed");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    let catalog = load_catalog(&mut tx).await.unwrap();
    assert_eq!(
        catalog.dimension_of(UnitId(1)).unwrap(),
        DimensionKind::Count
    );
    assert_eq!(
        catalog.dimension_of(UnitId(3)).unwrap(),
        DimensionKind::Length
    );
    assert_eq!(
        catalog.dimension_of(UnitId(5)).unwrap(),
        DimensionKind::Mass
    );
    assert_eq!(
        catalog.dimension_of(UnitId(7)).unwrap(),
        DimensionKind::Time
    );
    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn stock_unit_immutable_while_postings_exist() {
    let db = datum_test::db_case!("stock_immut");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.stock");
    let item = ItemId::generate();
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    common::insert_item_stock(&mut tx, item, UnitId(4), 4).await;
    tx.execute(
        sqlx::query("INSERT INTO uom.posting_stub (item_id) VALUES ($1)").bind(item.as_uuid()),
    )
    .await
    .unwrap();
    let err = tx
        .execute(
            sqlx::query("UPDATE uom.item_stock SET stock_scale = 2 WHERE item_id = $1")
                .bind(item.as_uuid()),
        )
        .await
        .unwrap_err();
    assert_eq!(common::pg_code(&err), "23514");
    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn every_uom_table_is_audited() {
    let db = datum_test::db_case!("audited");
    common::migrate(&db).await;
    for table in ["unit", "item_stock", "factor", "rounding_policy"] {
        let (exists,): (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS (
              SELECT 1 FROM pg_trigger t
              JOIN pg_class c ON c.oid = t.tgrelid
              JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'uom' AND c.relname = $1
               AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
            )
            "#,
        )
        .bind(table)
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
        assert!(exists, "uom.{table} missing zz_audit_row");
    }
    db.finish().await.unwrap();
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = datum_test::db_case!("writes_tx");
    common::migrate(&db).await;
    let err = sqlx::query(
        "INSERT INTO uom.unit (id, code, name, dimension, symbol, scale_default)
         VALUES (99, 'ZZ', 'ZZ', 'Count', 'zz', 0)",
    )
    .execute(db.app_pool())
    .await
    .unwrap_err();
    assert_eq!(
        err.as_database_error()
            .and_then(|d| d.code().map(|c| c.to_string()))
            .unwrap_or_default(),
        "42501"
    );
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM uom.unit WHERE id = 99")
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    db.finish().await.unwrap();
}

fn one_ulp_at_scale(scale: u32) -> Decimal {
    Decimal::ONE / Decimal::from(10u64.pow(scale))
}

#[tokio::test]
async fn round_trip_ab_a_within_one_ulp_at_stock_scale_8() {
    let db = datum_test::db_case!("roundtrip_s8");
    common::migrate(&db).await;
    let pool = WritePool::new(db.app_pool().clone());
    let ctx = common::write_ctx("uom.roundtrip_s8");
    let mut tx = datum_db::Tx::begin(&pool, &ctx).await.unwrap();
    tx.execute(sqlx::query(
        "INSERT INTO uom.factor (from_unit, to_unit, numerator, denominator, effective_from)
         VALUES (4, 3, 12, 1, '-infinity')",
    ))
    .await
    .unwrap();
    let catalog = load_catalog(&mut tx).await.unwrap();
    let item = ItemId::generate();
    common::insert_item_stock(&mut tx, item, UnitId(4), 8).await;
    let conv_ctx = ConversionContext { item, lot: None };
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let ulp = one_ulp_at_scale(8);

    for seed in 0..256u64 {
        let whole = (seed % 1000) + 1;
        let frac = (seed / 1000) % 100_000_000;
        let amount = Decimal::from(whole) + Decimal::from(frac) / dec("100000000");
        let original = Quantity::new(amount, inch).unwrap();
        let entered = AnyQuantity::from(original);
        let to_ft = to_stock::<LengthDim>(&mut tx, &catalog, item, entered, &conv_ctx)
            .await
            .unwrap();
        assert_eq!(
            to_ft.canonical.try_add(to_ft.residual).unwrap().amount(),
            (original.amount() * dec("1")) / dec("12"),
            "stock split must reconstruct unrounded ft at scale 8"
        );
        let back = catalog.convert(to_ft.canonical, inch, &conv_ctx).unwrap();
        let (in_in, r2) = split_with_policy(&catalog, back, &conv_ctx, inch.id());
        let (r1_in, _) = catalog
            .convert_amount(to_ft.residual.amount(), UnitId(4), UnitId(3), &conv_ctx)
            .unwrap();
        let slip = original.amount() - in_in.amount();
        let residual_gap = (r1_in + r2.amount() - slip).abs();
        assert!(
            residual_gap <= ulp,
            "residuals must sum to the slip within 1 ulp (gap {residual_gap}, seed {seed})"
        );
        assert!(
            slip.abs() <= ulp,
            "slip {slip} exceeds one ulp at scale 8 (seed {seed})"
        );
    }
    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[test]
fn round_half_even_at_stock_scale() {
    let ties = [
        (dec("1.005"), 2, dec("1.00"), dec("0.005")),
        (dec("1.015"), 2, dec("1.02"), dec("-0.005")),
        (dec("2.675"), 2, dec("2.68"), dec("-0.005")),
    ];
    let unit = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();
    for (amount, scale, want_value, want_residual) in ties {
        let converted = datum_core::Converted::new(amount, unit, scale);
        let (value, residual) = apply_rounding_policy(converted, Rounding::HalfEven);
        assert_eq!(value.amount(), want_value, "value at {amount}");
        assert_eq!(residual.amount(), want_residual, "residual at {amount}");
        assert_eq!(value.try_add(residual).unwrap().amount(), amount);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, .. ProptestConfig::default() })]

    #[test]
    fn residual_is_never_dropped(unscaled in 1i64..100_000i64) {
        let unit = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();
        let exact = Decimal::from(unscaled) / dec("10000");
        let converted = datum_core::Converted::new(exact, unit, 4);
        let (value, residual) = apply_rounding_policy(converted, Rounding::HalfEven);
        prop_assert_eq!(value.try_add(residual).unwrap().amount(), exact);
    }
}

#[test]
fn cross_dimension_is_unrepresentable() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn migrator_has_uom() {
        assert!(MIGRATOR.iter().any(|m| m.description.contains("uom")));
    }
}
