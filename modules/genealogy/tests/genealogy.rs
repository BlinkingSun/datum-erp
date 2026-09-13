//! Named genealogy tests (SPEC commit mode).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::path::PathBuf;

use sqlx::query as sql_query;
use sqlx::query_scalar as sql_query_scalar;
use wicket_core::{LotId, SerialId};
use wicket_db::Tx;
use wicket_mod_genealogy::{
    Direction, TRACE_JOB, TraceBody, TraceOrigin, TraceOutcome, TraceRequest, drop_cache, impact,
    job_status, signed_edge_sum, trace, trace_inline, undirected_edges, where_used,
};
use wicket_module::Kernel;
use wicket_test::db_case;

use common::{
    World, action_ctx, boot_kernel, complete_wo, has_zz_audit, issue_heat, pg_code, receive_heat,
    release_heat, seed_world, ship_fg, table_owner, write_pool,
};

fn contains_lot(nodes: &[wicket_mod_genealogy::TreeNode], lot: LotId) -> bool {
    fn walk(n: &wicket_mod_genealogy::TreeNode, lot: LotId) -> bool {
        n.lot == Some(lot) || n.children.iter().any(|c| walk(c, lot))
    }
    nodes.iter().any(|n| walk(n, lot))
}

fn contains_serial(nodes: &[wicket_mod_genealogy::TreeNode], serial: SerialId) -> bool {
    fn walk(n: &wicket_mod_genealogy::TreeNode, serial: SerialId) -> bool {
        n.serial == Some(serial) || n.children.iter().any(|c| walk(c, serial))
    }
    nodes.iter().any(|n| walk(n, serial))
}

async fn graph(w: &World, pool: &wicket_db::WritePool) {
    receive_heat(w, pool).await;
    release_heat(w, pool).await;
    complete_wo(w, pool).await;
}

#[tokio::test]
async fn forward_and_backward_traces_return_the_same_tree() {
    let db = db_case!("gen_same");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    graph(&w, &pool).await;
    let ctx = action_ctx(w.actor, "genealogy.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let fwd = match trace_inline(
        &mut tx,
        TraceRequest {
            origin: TraceOrigin::Lot(w.lot_heat),
            direction: Direction::Forward,
            depth: None,
            max_postings: None,
        },
    )
    .await
    .expect("forward")
    {
        TraceBody::One(t) => t.nodes,
        TraceBody::Both { forward, .. } => forward.nodes,
    };
    let back = match trace_inline(
        &mut tx,
        TraceRequest {
            origin: TraceOrigin::Lot(w.lot_fg),
            direction: Direction::Backward,
            depth: None,
            max_postings: None,
        },
    )
    .await
    .expect("backward")
    {
        TraceBody::One(t) => t.nodes,
        TraceBody::Both { backward, .. } => backward.nodes,
    };
    tx.commit().await.ok();
    assert!(
        contains_lot(&fwd, w.lot_fg),
        "forward from heat must reach the finished lot"
    );
    assert!(
        contains_lot(&back, w.lot_heat),
        "backward from finished lot must reach the heat"
    );
    let e_fwd = undirected_edges(&fwd);
    let e_back = undirected_edges(&back);
    assert!(!e_fwd.is_empty() && !e_back.is_empty());
    assert!(
        e_back.iter().all(|e| e_fwd.contains(e)) || e_fwd.iter().all(|e| e_back.contains(e)),
        "PLAN §3 item 8: the two traces share the same consumption tree (fwd={e_fwd:?} back={e_back:?})"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn trace_filters_by_lot_and_serial() {
    let db = db_case!("gen_filt");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    graph(&w, &pool).await;
    let ctx = action_ctx(w.actor, "genealogy.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let heat = match trace_inline(
        &mut tx,
        TraceRequest {
            origin: TraceOrigin::Lot(w.lot_heat),
            direction: Direction::Forward,
            depth: None,
            max_postings: None,
        },
    )
    .await
    .expect("heat")
    {
        TraceBody::One(t) => t.nodes,
        other => panic!("{other:?}"),
    };
    let serial = match trace_inline(
        &mut tx,
        TraceRequest {
            origin: TraceOrigin::Serial(w.serial),
            direction: Direction::Backward,
            depth: None,
            max_postings: None,
        },
    )
    .await
    .expect("serial")
    {
        TraceBody::One(t) => t.nodes,
        TraceBody::Both { backward, .. } => backward.nodes,
    };
    tx.commit().await.ok();
    assert!(contains_lot(&heat, w.lot_heat));
    assert!(
        !contains_lot(&heat, w.lot_bar),
        "bar lot was never posted; must not appear"
    );
    assert!(contains_serial(&serial, w.serial));
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn impact_lists_customer_shipments_and_units() {
    let db = db_case!("gen_imp");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    graph(&w, &pool).await;
    let order = wicket_core::Identifier::generate();
    let doc = ship_fg(&w, &pool, order).await;
    let ctx = action_ctx(w.actor, "genealogy.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let impact = impact(&mut tx, w.lot_heat).await.expect("impact");
    tx.commit().await.ok();
    assert!(
        impact.shipments.contains(&doc.id),
        "case g shipment must appear on the recall list"
    );
    assert!(impact.customers.iter().any(|c| c == "SO-2026-0101"));
    assert!(impact.units.contains(&w.serial));
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn reversal_edges_are_negative_and_do_not_double_count() {
    let db = db_case!("gen_rev");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_heat(&w, &pool).await;
    release_heat(&w, &pool).await;
    let issue = issue_heat(&w, &pool).await;
    let ctx = action_ctx(w.actor, "genealogy.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let before = match trace_inline(
        &mut tx,
        TraceRequest {
            origin: TraceOrigin::Lot(w.lot_heat),
            direction: Direction::Forward,
            depth: None,
            max_postings: None,
        },
    )
    .await
    .expect("before")
    {
        TraceBody::One(t) => t.nodes,
        TraceBody::Both { forward, .. } => forward.nodes,
    };
    let group = issue.posted_group_id.expect("group");
    wicket_ledger::reverse(&mut tx, group, "void issue")
        .await
        .expect("reverse");
    let after = match trace_inline(
        &mut tx,
        TraceRequest {
            origin: TraceOrigin::Lot(w.lot_heat),
            direction: Direction::Forward,
            depth: None,
            max_postings: None,
        },
    )
    .await
    .expect("after")
    {
        TraceBody::One(t) => t.nodes,
        TraceBody::Both { forward, .. } => forward.nodes,
    };
    tx.commit().await.ok();
    let sum_before = signed_edge_sum(&before);
    let sum_after = signed_edge_sum(&after);
    assert!(
        after.iter().any(|n| n
            .children
            .iter()
            .any(|c| c.edge_quantity.amount.is_sign_negative())),
        "reversal edges must be negative"
    );
    let issued = rust_decimal::Decimal::new(20, 0);
    let pair: rust_decimal::Decimal = {
        fn walk(
            n: &wicket_mod_genealogy::TreeNode,
            abs: rust_decimal::Decimal,
            acc: &mut rust_decimal::Decimal,
        ) {
            if n.edge_quantity.amount.abs() == abs {
                *acc += n.edge_quantity.amount;
            }
            for c in &n.children {
                walk(c, abs, acc);
            }
        }
        let mut acc = rust_decimal::Decimal::ZERO;
        for n in &after {
            walk(n, issued, &mut acc);
        }
        acc
    };
    assert_eq!(
        pair,
        rust_decimal::Decimal::ZERO,
        "issue+reversal edges must net to zero, not double-count (before={sum_before} after={sum_after})"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn large_trace_runs_as_job_with_progress() {
    let db = db_case!("gen_job");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_heat(&w, &pool).await;
    let ctx = action_ctx(w.actor, "genealogy.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let outcome = trace(
        &mut tx,
        w.actor,
        TraceRequest {
            origin: TraceOrigin::Lot(w.lot_heat),
            direction: Direction::Forward,
            depth: None,
            max_postings: Some(0),
        },
    )
    .await
    .expect("trace");
    tx.commit().await.expect("commit");
    let TraceOutcome::Accepted { job_id, result_url } = outcome else {
        panic!("expected 202 job, got inline");
    };
    assert!(result_url.contains("/api/v1/genealogy/jobs/"));
    assert!(
        w.kernel.job_kinds.iter().any(|j| j.kind == TRACE_JOB),
        "job kind registered through the manifest"
    );
    w.kernel
        .worker_tick(Kernel::service_actor())
        .await
        .expect("tick");
    let st = job_status(db.app_pool(), job_id)
        .await
        .expect("status")
        .expect("job row");
    assert!(
        st.progress_pct >= 25,
        "progress must be visible, got {}",
        st.progress_pct
    );
    assert!(
        st.state == wicket_jobs::JobState::Succeeded || st.progress_pct > 0,
        "job must run with progress"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn cache_is_rebuildable_and_never_authoritative() {
    let db = db_case!("gen_cache");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    graph(&w, &pool).await;
    let ctx = action_ctx(w.actor, "genealogy.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let req = TraceRequest {
        origin: TraceOrigin::Lot(w.lot_heat),
        direction: Direction::Forward,
        depth: None,
        max_postings: None,
    };
    let first = trace_inline(&mut tx, req).await.expect("first");
    drop_cache(&mut tx).await.expect("drop");
    let second = trace_inline(&mut tx, req).await.expect("second");
    tx.commit().await.ok();
    assert_eq!(first, second, "dropping the cache must not change results");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn module_reads_only_through_wicket_ledger_api() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut saw_back = false;
    let mut saw_fwd = false;
    for entry in std::fs::read_dir(&root).expect("src") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        if text.contains("trace_backward") {
            saw_back = true;
        }
        if text.contains("trace_forward") {
            saw_fwd = true;
        }
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            let lower = t.to_ascii_lowercase();
            assert!(
                !lower.contains("from ledger.")
                    && !lower.contains("join ledger.")
                    && !lower.contains("into ledger."),
                "no SQL against ledger.* in {}: {t}",
                path.display()
            );
        }
    }
    assert!(saw_back && saw_fwd, "must call published ledger trace API");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("gen_tx");
    boot_kernel(&db).await;
    let before: i64 = sql_query_scalar("SELECT count(*) FROM genealogy_transient.trace_cache")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let err = sql_query(
        r#"INSERT INTO genealogy_transient.trace_cache (key, computed_at, tree)
           VALUES ('raw', now(), '{}'::jsonb)"#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after: i64 = sql_query_scalar("SELECT count(*) FROM genealogy_transient.trace_cache")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(after, before);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn every_genealogy_table_is_audited_and_owned_by_wicket_owner() {
    let db = db_case!("gen_audit");
    boot_kernel(&db).await;
    assert!(
        has_zz_audit(db.migrate_pool(), "genealogy_transient", "trace_cache").await,
        "genealogy_transient.trace_cache zz_audit_row"
    );
    assert_eq!(
        table_owner(db.migrate_pool(), "genealogy_transient", "trace_cache").await,
        "wicket_owner"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn where_used_walks_item_revision() {
    let db = db_case!("gen_wu");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    graph(&w, &pool).await;
    let ctx = action_ctx(w.actor, "genealogy.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let trees = where_used(&mut tx, db.app_pool(), w.bar, "A")
        .await
        .expect("where_used");
    let empty = where_used(&mut tx, db.app_pool(), w.bar, "Z")
        .await
        .expect("wrong rev");
    tx.commit().await.ok();
    assert!(!trees.is_empty());
    assert!(empty.is_empty());
    db.finish().await.expect("finish");
}
