//! Query-seam named tests (D2 R5 / on-hand). Kernel-always-on: identical under
//! both installation profiles (`regulated-device` and `plain-shop`).
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use datum_core::ItemId;
use datum_db::Tx;
use datum_ledger::{has_postings, has_quantity_at};
use uuid::Uuid;

use common::*;

#[tokio::test]
async fn has_postings_false_then_true_after_post() {
    let db = datum_test::db_case!("hp_ft");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.has_postings");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    assert!(
        !has_postings(&mut tx, w.bar).await.unwrap(),
        "FIFO item has no postings before the first group"
    );
    assert!(
        !has_postings(&mut tx, w.screw).await.unwrap(),
        "STANDARD item has no postings before the first group"
    );
    post_case_a(&mut tx, &w).await;
    assert!(
        has_postings(&mut tx, w.bar).await.unwrap(),
        "receipt posts QUANTITY rows for the bar"
    );
    assert!(
        !has_postings(&mut tx, w.screw).await.unwrap(),
        "a posting for one item does not mark another"
    );
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn has_quantity_at_false_then_true_after_receipt_then_false_after_issue() {
    let db = datum_test::db_case!("hq_ftf");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.has_quantity_at");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    assert!(
        !has_quantity_at(&mut tx, w.quarantine).await.unwrap(),
        "empty real location"
    );
    post_case_a(&mut tx, &w).await;
    assert!(
        has_quantity_at(&mut tx, w.quarantine).await.unwrap(),
        "receipt puts positive QUANTITY at quarantine"
    );
    post_case_b(&mut tx, &w).await;
    assert!(
        !has_quantity_at(&mut tx, w.quarantine).await.unwrap(),
        "release/issue empties quarantine"
    );
    assert!(
        has_quantity_at(&mut tx, w.available).await.unwrap(),
        "the same quantity now sits at available"
    );
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn has_postings_no_actor_is_refused() {
    let db = datum_test::db_case!("hp_42501");
    common::migrate(&db).await;
    let item = ItemId::generate().as_uuid();
    let loc = Uuid::now_v7();
    let err = sqlx::query("SELECT ledger.has_postings($1)")
        .bind(item)
        .fetch_one(db.app_pool())
        .await
        .expect_err("has_postings without Tx::begin");
    assert_eq!(pg_code_sqlx(&err), "42501", "err={err}");
    let err = sqlx::query("SELECT ledger.has_quantity_at($1)")
        .bind(loc)
        .fetch_one(db.app_pool())
        .await
        .expect_err("has_quantity_at without Tx::begin");
    assert_eq!(pg_code_sqlx(&err), "42501", "err={err}");
    db.finish().await.unwrap();
}
