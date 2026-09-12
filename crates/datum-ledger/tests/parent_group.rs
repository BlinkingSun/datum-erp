//! Parent group link (R-2s-6): builder, post, `children_of`, FK and actor gates.
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use datum_core::{Boundary, GroupKind, Identifier, PostingIntent, PostingSink};
use datum_db::Tx;
use datum_ledger::{Error, GroupBuilder, UOM_CONVERSION_RESIDUAL, children_of, post};
use uuid::Uuid;

use common::*;

#[tokio::test]
async fn residual_group_links_to_parent() {
    let db = datum_test::db_case!("pg_residual");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.parent_residual");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let movement = post_case_a(&mut tx, &w).await;

    let mut header = movement_header("residual_child");
    header.reason_code = Some(UOM_CONVERSION_RESIDUAL.into());
    let mut adj = GroupBuilder::new(GroupKind::Adjustment, header);
    adj.parent(movement);
    adj.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("0.0050"),
        w.scrap,
        Some(Boundary::Scrap),
        None,
    )))
    .unwrap();
    adj.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-0.0050"),
        w.adjustment,
        Some(Boundary::Adjustment),
        None,
    )))
    .unwrap();
    let child = post(&mut tx, adj).await.expect("adjustment conserves");
    assert!(
        children_of(&mut tx, movement).await.unwrap() == vec![child],
        "children_of must list the linked adjustment"
    );
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn parent_must_exist() {
    let db = datum_test::db_case!("pg_fk");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.parent_fk");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let missing = Identifier::generate();

    let mut header = movement_header("bad_parent");
    header.reason_code = Some(UOM_CONVERSION_RESIDUAL.into());
    let mut b = GroupBuilder::new(GroupKind::Adjustment, header);
    b.parent(missing);
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-0.0050"),
        w.quarantine,
        None,
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("0.0050"),
        w.rounding,
        Some(Boundary::Rounding),
        None,
    )))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("parent row missing");
    assert!(
        matches!(err, Error::ParentMustExist),
        "expected ParentMustExist, got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn children_of_no_actor_is_refused() {
    let db = datum_test::db_case!("pg_42501");
    common::migrate(&db).await;
    let parent = Uuid::now_v7();
    let err = sqlx::query("SELECT ledger.children_of($1)")
        .bind(parent)
        .fetch_one(db.app_pool())
        .await
        .expect_err("children_of without Tx::begin");
    assert_eq!(pg_code_sqlx(&err), "42501", "err={err}");
    db.finish().await.unwrap();
}
