//! Genealogy traces (SPEC deliverable 10).
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use wicket_db::Tx;
use wicket_ledger::{TraceStart, trace_backward, trace_forward};

use common::*;

#[tokio::test]
async fn trace_backward_and_forward_follow_consumption() {
    let db = wicket_test::db_case!("geneal");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.geneal");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let c = post_case_c(&mut tx, &w).await;

    let consuming: i64 = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT consuming_posting_id FROM ledger.consumption WHERE group_id = $1",
            )
            .bind(c.as_uuid()),
        )
        .await
        .map(|(id,): (i64,)| id)
        .unwrap();
    let consumed: i64 = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT consumed_posting_id FROM ledger.consumption WHERE group_id = $1",
            )
            .bind(c.as_uuid()),
        )
        .await
        .map(|(id,): (i64,)| id)
        .unwrap();

    let back = trace_backward(
        &mut tx,
        TraceStart::Posting(wicket_core::PostingId(consuming)),
    )
    .await
    .expect("backward");
    assert!(
        back.iter()
            .any(|n| n.children.iter().any(|c| c.posting.0 == consumed)),
        "backward from issue must name the consumed layer; tree={back:?}"
    );

    let fwd = trace_forward(
        &mut tx,
        TraceStart::Posting(wicket_core::PostingId(consumed)),
    )
    .await
    .expect("forward");
    assert!(
        fwd.iter()
            .any(|n| n.children.iter().any(|c| c.posting.0 == consuming)),
        "forward from the layer must name the issue; tree={fwd:?}"
    );

    let by_lot = trace_backward(&mut tx, TraceStart::Lot(w.lot_bar))
        .await
        .expect("lot");
    assert!(
        !by_lot.is_empty(),
        "lot start must return trees for BAR lot"
    );

    commit_ok(tx).await;
    db.finish().await.unwrap();
}
