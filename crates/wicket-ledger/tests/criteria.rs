//! PLAN §7 criteria, each a named test.
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use rust_decimal::Decimal;
use wicket_core::{Boundary, GroupKind, PostingError, PostingIntent, PostingSink, ValueAccount};
use wicket_db::Tx;
use wicket_ledger::{
    BalanceSlice, GroupBuilder, balance_at, post, rebuild, reverse, verify_projection,
};

use common::*;

#[tokio::test]
async fn every_legitimate_sequence_commits() {
    let db = wicket_test::db_case!("crit1");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit1");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    // Generator of prefixes of the canonical a–c–l sequence (PLAN §7 criterion 1).
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let c = post_case_c(&mut tx, &w).await;
    reverse(&mut tx, c, "seq_l").await.expect("l");
    verify_projection(&mut tx).await.expect("sequence fold");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn transposed_digit_is_rejected_and_names_predicate() {
    let db = wicket_test::db_case!("crit2");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit2");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("po"));
    let recv = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("0200.0000"),
            w.quarantine,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-2000.0000"),
        w.supplier,
        Some(Boundary::Supplier),
        Some(w.lot_bar),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("4720.00"),
        Some(recv),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::ApAccrual,
        usd("-4720.00"),
        None,
        None,
    )))
    .unwrap();
    post(&mut tx, b).await.expect("insert unbalanced");
    let err = tx.commit().await.expect_err("P1");
    assert_eq!(pg_code_db(&err), "ZL002", "predicate P1 / ZL002 err={err}");
    db.finish().await.unwrap();
}

#[tokio::test]
async fn dropped_counterpart_is_rejected() {
    let db = wicket_test::db_case!("crit3");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit3");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
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
    b.contribute(PostingIntent::Quantity(q_post(
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
    // dropped WIP counterpart
    post(&mut tx, b).await.expect("insert");
    let err = tx.commit().await.expect_err("P2-A");
    assert_eq!(
        pg_code_db(&err),
        "ZL003",
        "dropped value counterpart → ZL003"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn identity_crossing_outside_transformation_is_rejected() {
    let db = wicket_test::db_case!("crit4");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit4");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("bad_seam"));
    let err = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("20.0000"),
            w.consumed,
            Some(Boundary::Consumed),
            None,
        )))
        .unwrap_err();
    assert!(matches!(err, PostingError::Shape(_)));
    // The database CHECK is the gate: insert CONSUMED on MOVEMENT → 23514.
    let gid = raw_group(&mut tx, "MOVEMENT", "bad_seam", None, None, None, None).await;
    let res = tx
        .execute(
            sqlx::query(
                "INSERT INTO ledger.posting (
                     group_id, kind, measure, item_id, uom_id, stock_scale, residual_tolerance,
                     location_id, boundary, quantity
                 ) VALUES (
                     $1, 'MOVEMENT', 'QUANTITY', $2, $3, 4, 0.0100,
                     $4, 'CONSUMED', 20.0000
                 )",
            )
            .bind(gid)
            .bind(w.bar.as_uuid())
            .bind(FT.0)
            .bind(w.consumed.as_uuid()),
        )
        .await;
    let err = res.expect_err("boundary_permitted");
    assert_eq!(
        pg_code_db(&err),
        "23514",
        "CONSUMED on MOVEMENT must be 23514, got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn allocation_not_reproducing_quantity_is_rejected() {
    let db = wicket_test::db_case!("crit5");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit5");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let layer = layer_at(&mut tx, w.bar, w.available).await.unwrap();
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
    b.contribute(PostingIntent::Consumption(
        wicket_core::ConsumptionPosting {
            consuming: out,
            consumed_posting_id: wicket_core::PostingId(layer),
            quantity: qty_ft("2.0000"),
            amount: usd("47.20"),
        },
    ))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("qty mismatch");
    assert!(
        matches!(
            err,
            wicket_ledger::Error::Posting(PostingError::AllocationMismatch(_))
        ),
        "got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn allocation_not_reproducing_money_is_rejected() {
    let db = wicket_test::db_case!("crit6");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit6");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let layer = layer_at(&mut tx, w.bar, w.available).await.unwrap();
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
    b.contribute(PostingIntent::Consumption(
        wicket_core::ConsumptionPosting {
            consuming: out,
            consumed_posting_id: wicket_core::PostingId(layer),
            quantity: qty_ft("20.0000"),
            amount: usd("4.72"),
        },
    ))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("money mismatch");
    assert!(
        matches!(
            err,
            wicket_ledger::Error::Posting(PostingError::AllocationMismatch(_))
        ),
        "got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn projections_equal_fold_after_every_sequence_and_after_rebuild() {
    let db = wicket_test::db_case!("crit7");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit7");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    verify_projection(&mut tx).await.expect("after sequence");
    rebuild(&mut tx).await.expect("rebuild");
    verify_projection(&mut tx).await.expect("after rebuild");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn balance_reconstructible_at_any_instant() {
    let db = wicket_test::db_case!("crit8");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit8");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    let t1: chrono::DateTime<chrono::Utc> = tx
        .fetch_one(sqlx::query_as("SELECT clock_timestamp()"))
        .await
        .map(|(t,): (chrono::DateTime<chrono::Utc>,)| t)
        .unwrap();
    post_case_b(&mut tx, &w).await;
    let slice = BalanceSlice {
        item: w.bar,
        location: w.quarantine,
        lot: Some(w.lot_bar),
        serial: None,
        unit: FT,
    };
    let q = balance_at(&mut tx, slice, t1).await.unwrap();
    assert_eq!(q, dec("2000.0000"));
    let later: chrono::DateTime<chrono::Utc> = tx
        .fetch_one(sqlx::query_as("SELECT clock_timestamp()"))
        .await
        .map(|(t,): (chrono::DateTime<chrono::Utc>,)| t)
        .unwrap();
    let q2 = balance_at(&mut tx, slice, later).await.unwrap();
    assert_eq!(q2, Decimal::ZERO);
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn reversal_restores_state_without_deleting() {
    let db = wicket_test::db_case!("crit9");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.crit9");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let c = post_case_c(&mut tx, &w).await;
    reverse(&mut tx, c, "undo").await.unwrap();
    let n: i64 = tx
        .fetch_one(sqlx::query_as("SELECT count(*) FROM ledger.posting_group"))
        .await
        .map(|(n,): (i64,)| n)
        .unwrap();
    assert!(n >= 4, "original groups still present");
    let slice = BalanceSlice {
        item: w.bar,
        location: w.available,
        lot: Some(w.lot_bar),
        serial: None,
        unit: FT,
    };
    let restored = balance_at(
        &mut tx,
        slice,
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    assert_eq!(restored, dec("2000.0000"), "reversal restores available");
    let err = reverse(&mut tx, c, "again")
        .await
        .expect_err("second reverse");
    assert!(
        matches!(err, wicket_ledger::Error::AlreadyReversed) || pg_code_ledger(&err) == "23505",
        "got {err}"
    );
    commit_ok(tx).await;
    db.finish().await.unwrap();
}
