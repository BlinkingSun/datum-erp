//! ADDENDUM 1–2 named tests (cycle-1 residuals).
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use datum_core::{
    Boundary, GroupKind, ItemId, LotId, PostingError, PostingIntent, PostingSink, ValueAccount,
};
use datum_db::Tx;
use datum_ledger::{
    BalanceSlice, CostMethod, GroupBuilder, apply_group, balance_at, bind_tx, commit, post,
    rebuild, reverse, upsert_stock_item, verify_projection,
};
use rust_decimal::Decimal;

use common::*;

#[tokio::test]
async fn explicit_consumption_skips_auto_allocation() {
    let db = datum_test::db_case!("p3ex");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.p3ex");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let layer = layer_at(&mut tx, w.bar, w.available).await.expect("layer");
    commit_ok(tx).await;

    let ctx2 = write_ctx(actor(), "ledger.p3ex2");
    let mut tx = Tx::begin(&pool, &ctx2).await.unwrap();
    let mut header = movement_header("p3ex");
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Movement, header);
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-20.0000"),
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("20.0000"),
            w.wip,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-47.20"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("47.20"),
        Some(into),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
        consuming: out,
        consumed_posting_id: datum_core::PostingId(layer),
        quantity: qty_ft("20.0000"),
        amount: usd("47.20"),
    }))
    .unwrap();
    let gid = post(&mut tx, b).await.unwrap();
    commit_ok(tx).await;
    let mut tx = Tx::begin(&pool, &ctx2).await.unwrap();
    let n: (i64,) = tx
        .fetch_one(
            sqlx::query_as("SELECT COUNT(*) FROM ledger.consumption WHERE group_id = $1")
                .bind(gid.as_uuid()),
        )
        .await
        .unwrap();
    assert_eq!(n.0, 1, "explicit edge only; FIFO must not double-allocate");
    db.finish().await.unwrap();
}

#[tokio::test]
async fn unfinalized_box_dyn_posting_sink_poisons() {
    let db = datum_test::db_case!("unfin_box");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "unfin_box");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut builder = GroupBuilder::new(GroupKind::Movement, movement_header("t"));
    bind_tx(&mut builder, &mut tx).await.unwrap();
    let mut sink: Box<dyn PostingSink> = Box::new(builder);
    sink.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("1.0000"),
        w.quarantine,
        None,
        None,
    )))
    .unwrap();
    drop(sink);
    let err = commit(tx, &[]).await.expect_err("box dyn poison");
    assert!(matches!(err, datum_ledger::Error::Unfinalized));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn unfinalized_mut_dyn_posting_sink_poisons() {
    let db = datum_test::db_case!("unfin_mut");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "unfin_mut");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut builder = GroupBuilder::new(GroupKind::Movement, movement_header("t"));
    bind_tx(&mut builder, &mut tx).await.unwrap();
    {
        let sink: &mut dyn PostingSink = &mut builder;
        sink.contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("1.0000"),
            w.quarantine,
            None,
            None,
        )))
        .unwrap();
    }
    drop(builder);
    let err = commit(tx, &[]).await.expect_err("mut dyn poison");
    assert!(matches!(err, datum_ledger::Error::Unfinalized));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn poisoned_transaction_cannot_commit() {
    let db = datum_test::db_case!("poison_commit");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "poison_commit");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("pc"));
    bind_tx(&mut b, &mut tx).await.unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("1.0000"),
        w.quarantine,
        None,
        None,
    )))
    .ok();
    drop(b);
    let err = commit(tx, &[]).await.expect_err("poisoned tx");
    assert!(matches!(err, datum_ledger::Error::Unfinalized));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn standard_costing_posts_ppv() {
    let db = datum_test::db_case!("std_ppv");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "std_ppv");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let item = ItemId::generate();
    upsert_stock_item(
        &mut tx,
        item,
        EA,
        0,
        dec("0"),
        CostMethod::Standard,
        Some(usd("1.00")),
    )
    .await
    .unwrap();
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.item_stock (item_id, stock_unit_id, stock_scale, residual_tolerance)
             VALUES ($1, $2, 0, 0)",
        )
        .bind(item.as_uuid())
        .bind(EA.0),
    )
    .await
    .unwrap();
    let supplier = loc_simple(&mut tx, Some(Boundary::Supplier)).await;
    let fg = loc_simple(&mut tx, None).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("rcv"));
    let h = b
        .contribute(PostingIntent::Quantity(q_post(
            item,
            qty_ea("-10"),
            supplier,
            Some(Boundary::Supplier),
            None,
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        item,
        qty_ea("10"),
        fg,
        None,
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("15.00"),
        Some(h),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::ApAccrual,
        usd("-15.00"),
        None,
        None,
    )))
    .unwrap();
    post(&mut tx, b).await.unwrap();
    commit_ok(tx).await;

    let ctx2 = write_ctx(actor(), "std_ppv2");
    let mut tx = Tx::begin(&pool, &ctx2).await.unwrap();
    let mut b2 = GroupBuilder::new(GroupKind::Movement, movement_header("iss"));
    let hw = b2
        .contribute(PostingIntent::Quantity(q_post(
            item,
            qty_ea("-10"),
            fg,
            None,
            None,
        )))
        .unwrap();
    let cust = loc_simple(&mut tx, Some(Boundary::Customer)).await;
    b2.contribute(PostingIntent::Quantity(q_post(
        item,
        qty_ea("10"),
        cust,
        Some(Boundary::Customer),
        None,
    )))
    .unwrap();
    b2.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-10.00"),
        Some(hw),
        None,
    )))
    .unwrap();
    b2.contribute(PostingIntent::Value(v_post(
        ValueAccount::Cogs,
        usd("10.00"),
        None,
        None,
    )))
    .unwrap();
    let ig = post(&mut tx, b2).await.unwrap();
    commit_ok(tx).await;

    let mut tx = Tx::begin(&pool, &ctx2).await.unwrap();
    let cons: (Decimal,) = tx
        .fetch_one(
            sqlx::query_as("SELECT amount FROM ledger.consumption WHERE group_id = $1 LIMIT 1")
                .bind(ig.as_uuid()),
        )
        .await
        .unwrap();
    assert_eq!(cons.0, dec("10.00"), "consumption at standard layer qty");
    let ppv: Option<(Decimal,)> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT amount FROM ledger.posting WHERE group_id = $1 AND account = 'PPV'",
            )
            .bind(ig.as_uuid()),
        )
        .await
        .unwrap();
    assert!(
        ppv.is_some_and(|(a,)| !a.is_zero()),
        "STANDARD issue must post PPV for layer vs standard variance"
    );
    db.finish().await.unwrap();
}

async fn loc_simple(tx: &mut Tx<'_>, boundary: Option<Boundary>) -> datum_core::LocationId {
    let id = datum_core::LocationId::generate();
    datum_ledger::upsert_location(tx, id, boundary)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn balance_at_is_lot_and_serial_aware() {
    let db = datum_test::db_case!("bal_lot");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "bal_lot");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let lot_a = LotId::generate();
    let lot_b = LotId::generate();
    for (lot, qty) in [(lot_a, "10.0000"), (lot_b, "20.0000")] {
        let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("lot"));
        let into = b
            .contribute(PostingIntent::Quantity(q_post(
                w.bar,
                qty_ft(qty),
                w.quarantine,
                None,
                Some(lot),
            )))
            .unwrap();
        b.contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft(&format!("-{qty}")),
            w.supplier,
            Some(Boundary::Supplier),
            Some(lot),
        )))
        .unwrap();
        b.contribute(PostingIntent::Value(v_post(
            ValueAccount::Inventory,
            usd("10.00"),
            Some(into),
            None,
        )))
        .unwrap();
        b.contribute(PostingIntent::Value(v_post(
            ValueAccount::ApAccrual,
            usd("-10.00"),
            None,
            None,
        )))
        .unwrap();
        post(&mut tx, b).await.unwrap();
    }
    let slice_a = BalanceSlice {
        item: w.bar,
        location: w.quarantine,
        lot: Some(lot_a),
        serial: None,
        unit: FT,
    };
    let slice_b = BalanceSlice {
        item: w.bar,
        location: w.quarantine,
        lot: Some(lot_b),
        serial: None,
        unit: FT,
    };
    let qa = balance_at(&mut tx, slice_a, chrono::Utc::now())
        .await
        .unwrap();
    let qb = balance_at(&mut tx, slice_b, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(qa, dec("10.0000"));
    assert_eq!(qb, dec("20.0000"));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn reverse_kind_from_target_not_sink() {
    let db = datum_test::db_case!("revkind");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "revkind");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let gid = post_case_a(&mut tx, &w).await;
    commit_ok(tx).await;
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let rid = reverse(&mut tx, gid, "oops").await.unwrap();
    commit_ok(tx).await;
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let row: (String, String) = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT kind::text, reverses_kind::text FROM ledger.posting_group WHERE group_id = $1",
            )
            .bind(rid.as_uuid()),
        )
        .await
        .unwrap();
    assert_eq!(row.0, "REVERSAL");
    assert_eq!(row.1, "MOVEMENT");
    db.finish().await.unwrap();
}

#[tokio::test]
async fn no_reversal_of_reversal() {
    let db = datum_test::db_case!("norev");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "norev");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let gid = post_case_a(&mut tx, &w).await;
    let rid = reverse(&mut tx, gid, "oops").await.unwrap();
    commit_ok(tx).await;
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let err = reverse(&mut tx, rid, "again").await.unwrap_err();
    let s = format!("{err:?}");
    assert!(
        s.contains("cannot reverse a reversal") || s.contains("23514"),
        "{s}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn ineligible_layer_rejected() {
    let db = datum_test::db_case!("inelig");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "inelig");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let layer = layer_at(&mut tx, w.bar, w.available).await.expect("layer");
    commit_ok(tx).await;
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("inelig"));
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-5.0000"),
            w.quarantine,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("5.0000"),
        w.supplier,
        Some(Boundary::Supplier),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
        consuming: out,
        consumed_posting_id: datum_core::PostingId(layer),
        quantity: qty_ft("5.0000"),
        amount: usd("11.80"),
    }))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("wrong location layer");
    assert!(
        matches!(
            err,
            datum_ledger::Error::Posting(PostingError::IneligibleLayer { .. })
        ),
        "{err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn explicit_consumption_money_half_validated() {
    let db = datum_test::db_case!("ex_money");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ex_money");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let layer = layer_at(&mut tx, w.bar, w.available).await.unwrap();
    commit_ok(tx).await;
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let mut header = movement_header("wo");
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Movement, header);
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-20.0000"),
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("20.0000"),
            w.wip,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-47.20"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("47.20"),
        Some(into),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
        consuming: out,
        consumed_posting_id: datum_core::PostingId(layer),
        quantity: qty_ft("20.0000"),
        amount: usd("4.72"),
    }))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("money");
    assert!(matches!(
        err,
        datum_ledger::Error::Posting(PostingError::AllocationMismatch(_))
    ));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn verify_projection_catches_each_column_poison() {
    let db = datum_test::db_case!("vproj");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "vproj");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    verify_projection(&mut tx).await.expect("clean");
    tx.execute(sqlx::query(
        "UPDATE transient.balance_projection SET quantity = quantity + 1",
    ))
    .await
    .unwrap();
    assert!(verify_projection(&mut tx).await.is_err());
    rebuild(&mut tx).await.unwrap();
    tx.execute(sqlx::query(
        "UPDATE transient.layer_projection SET remaining_qty = remaining_qty + 1",
    ))
    .await
    .unwrap();
    assert!(verify_projection(&mut tx).await.is_err());
    rebuild(&mut tx).await.unwrap();
    tx.execute(sqlx::query(
        "UPDATE transient.layer_projection SET remaining_amt = remaining_amt + 1",
    ))
    .await
    .unwrap();
    assert!(verify_projection(&mut tx).await.is_err());
    db.finish().await.unwrap();
}

#[tokio::test]
async fn allocation_is_per_location() {
    let db = datum_test::db_case!("p3loc");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "p3loc");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    commit_ok(tx).await;
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("p3loc"));
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-20.0000"),
        w.quarantine,
        None,
        Some(w.lot_bar),
    )))
    .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("20.0000"),
        w.available,
        None,
        Some(w.lot_bar),
    )))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("wrong loc pool");
    assert!(matches!(
        err,
        datum_ledger::Error::Posting(PostingError::AllocationRequired(_))
    ));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn apply_group_incremental_matches_rebuild() {
    let db = datum_test::db_case!("incrb");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "incrb");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let a = post_case_a(&mut tx, &w).await;
    let b = post_case_b(&mut tx, &w).await;
    rebuild(&mut tx).await.unwrap();
    verify_projection(&mut tx).await.expect("rebuild");
    tx.execute(sqlx::query("DELETE FROM transient.balance_projection"))
        .await
        .unwrap();
    tx.execute(sqlx::query("DELETE FROM transient.layer_projection"))
        .await
        .unwrap();
    apply_group(&mut tx, a).await.unwrap();
    apply_group(&mut tx, b).await.unwrap();
    verify_projection(&mut tx)
        .await
        .expect("incremental equals fold");
    db.finish().await.unwrap();
}
