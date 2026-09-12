//! Named production-min tests (SPEC commit mode).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::PostingError;
use datum_db::Tx;
use datum_ledger::{TraceStart, trace_forward};
use datum_mod_production_min::{
    CompleteRequest, CreateWorkOrder, FinishedLotTemplate, Status, complete, create, load,
    load_completion,
};
use datum_test::db_case;
use sqlx::query as sql_query;
use sqlx::query_scalar as sql_query_scalar;

use common::{
    action_ctx, boot_kernel, complete_wo, create_wo, edge_ctx, has_zz_audit, issue_bar, pg_code,
    qty_ea, receive_bars, release_lot, release_wo, seed_world, start_wo, table_owner, write_pool,
};

#[tokio::test]
async fn release_allocates_gap_free_number_late() {
    let db = db_case!("prod_num");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let a = create_wo(&w, &pool).await;
    assert!(
        a.number.is_none(),
        "number is allocated at release, not create"
    );
    let ctx = action_ctx(w.actor, "production.create");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin b");
    let b = create(
        &mut tx,
        &w.kernel,
        CreateWorkOrder {
            item: w.screw,
            quantity_ordered: qty_ea("10"),
            revision: "C".into(),
        },
    )
    .await
    .expect("create b");
    tx.commit().await.expect("commit b");
    let a = release_wo(&w, &pool, a.id).await;
    let b = release_wo(&w, &pool, b.id).await;
    let na = a.number.expect("a number");
    let nb = b.number.expect("b number");
    assert!(na.starts_with("WO-"), "got {na}");
    assert!(nb.starts_with("WO-"), "got {nb}");
    let ta = trailing_int(&na);
    let tb = trailing_int(&nb);
    assert_eq!(tb, ta + 1, "gap-free: {na} then {nb}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn release_creates_wip_location_once() {
    let db = db_case!("prod_wip");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let wo = create_wo(&w, &pool).await;
    let released = release_wo(&w, &pool, wo.id).await;
    let wip = released.wip_location.expect("wip");
    let ctx = action_ctx(w.actor, "locations.edit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let again = datum_mod_locations::ensure_wip(&mut tx, wo.id)
        .await
        .expect("ensure");
    tx.commit().await.ok();
    assert_eq!(again, wip, "ensure_wip is idempotent per work order");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn complete_posts_priced_transformation_case_d() {
    let db = db_case!("prod_case_d");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let wo = create_wo(&w, &pool).await;
    let wo = release_wo(&w, &pool, wo.id).await;
    issue_bar(&w, &pool, wo.id).await;
    start_wo(&w, &pool, wo.id).await;
    let c = complete_wo(&w, &pool, wo.id).await;
    let kind: String =
        sql_query_scalar("SELECT kind::text FROM ledger.posting_group WHERE group_id = $1")
            .bind(c.group_id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("kind");
    assert_eq!(kind, "TRANSFORMATION");
    let wo_id: Option<uuid::Uuid> =
        sql_query_scalar("SELECT work_order_id FROM ledger.posting_group WHERE group_id = $1")
            .bind(c.group_id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("wo");
    assert_eq!(
        wo_id,
        Some(wo.id.as_uuid()),
        "work_order_id required on TRANSFORMATION"
    );
    let var: i64 = sql_query_scalar(
        "SELECT count(*) FROM ledger.posting
          WHERE group_id = $1 AND account = 'MFG_VARIANCE'",
    )
    .bind(c.group_id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("variance");
    assert!(var >= 1, "difference named as MFG_VARIANCE");
    let n = sql_query_scalar::<_, i64>(
        "SELECT count(*) FROM audit.event
          WHERE action = 'production.complete' AND table_name = 'work_order'
            AND doc_id = $1",
    )
    .bind(wo.id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("audit");
    assert!(
        n >= 1,
        "audit rows carry action production.complete with the work order as doc_id"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn complete_contributes_produced_lineage_edges() {
    let db = db_case!("prod_lineage");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let wo = create_wo(&w, &pool).await;
    let wo = release_wo(&w, &pool, wo.id).await;
    issue_bar(&w, &pool, wo.id).await;
    start_wo(&w, &pool, wo.id).await;
    let c = complete_wo(&w, &pool, wo.id).await;
    let ctx = action_ctx(w.actor, "production.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let fwd = trace_forward(&mut tx, TraceStart::Lot(w.lot_bar))
        .await
        .expect("forward");
    tx.commit().await.ok();
    assert!(
        tree_has_lot(&fwd, c.finished_lot),
        "forward trace from the bar lot must reach the finished lot"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn complete_without_issued_material_is_lineage_required_error() {
    let db = db_case!("prod_nolineage");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let wo = create_wo(&w, &pool).await;
    let wo = release_wo(&w, &pool, wo.id).await;
    start_wo(&w, &pool, wo.id).await;
    let ctx = edge_ctx(&w.kernel, w.actor, wo.id, "complete");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = complete(
        &mut tx,
        &w.kernel,
        &ctx,
        CompleteRequest {
            work_order: wo.id,
            to_location: w.fg,
            good: qty_ea("500"),
            scrap: qty_ea("0"),
            finished_lot: FinishedLotTemplate {
                number: Some("LOT-WO-1847".into()),
                template: None,
                serial_template: None,
            },
        },
    )
    .await
    .expect_err("lineage");
    assert!(
        matches!(
            err,
            datum_mod_production_min::Error::Ledger(datum_ledger::Error::Posting(
                PostingError::LineageRequired(_)
            ))
        ),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn complete_creates_finished_lot_with_kernel_identifier() {
    let db = db_case!("prod_lot");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let wo = create_wo(&w, &pool).await;
    let wo = release_wo(&w, &pool, wo.id).await;
    issue_bar(&w, &pool, wo.id).await;
    start_wo(&w, &pool, wo.id).await;
    let c = complete_wo(&w, &pool, wo.id).await;
    let ctx = action_ctx(w.actor, "lots.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let lot = datum_mod_lots::load_lot(&mut tx, c.finished_lot)
        .await
        .expect("lot");
    tx.commit().await.ok();
    assert_eq!(lot.number, "LOT-WO-1847");
    datum_numbering::lot::validate(&lot.number).expect("kernel identifier");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn abort_mid_completion_leaves_no_group_no_lot_no_transition() {
    let db = db_case!("prod_abort");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let wo = create_wo(&w, &pool).await;
    let wo = release_wo(&w, &pool, wo.id).await;
    start_wo(&w, &pool, wo.id).await;
    let lots_before: i64 = sql_query_scalar("SELECT count(*) FROM lots.lot WHERE item_id = $1")
        .bind(w.screw.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("lots before");
    let groups_before: i64 =
        sql_query_scalar("SELECT count(*) FROM ledger.posting_group WHERE work_order_id = $1")
            .bind(wo.id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("groups before");
    let ctx = edge_ctx(&w.kernel, w.actor, wo.id, "complete");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = complete(
        &mut tx,
        &w.kernel,
        &ctx,
        CompleteRequest {
            work_order: wo.id,
            to_location: w.fg,
            good: qty_ea("500"),
            scrap: qty_ea("0"),
            finished_lot: FinishedLotTemplate {
                number: Some("LOT-WO-1847".into()),
                template: None,
                serial_template: None,
            },
        },
    )
    .await
    .expect_err("abort");
    let _ = err;
    tx.rollback().await.expect("rollback");
    let lots_after: i64 = sql_query_scalar("SELECT count(*) FROM lots.lot WHERE item_id = $1")
        .bind(w.screw.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("lots after");
    let groups_after: i64 =
        sql_query_scalar("SELECT count(*) FROM ledger.posting_group WHERE work_order_id = $1")
            .bind(wo.id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("groups after");
    let ctx = action_ctx(w.actor, "production.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin load");
    let live = load(&mut tx, wo.id).await.expect("load");
    let completion = load_completion(&mut tx, wo.id).await.expect("completion");
    tx.commit().await.ok();
    assert_eq!(lots_after, lots_before, "no finished lot");
    assert_eq!(groups_after, groups_before, "no group");
    assert_ne!(live.status, Status::Completed, "no complete transition");
    assert!(completion.is_none());
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn every_production_table_is_audited_and_owned_by_datum_owner() {
    let db = db_case!("prod_audit");
    boot_kernel(&db).await;
    for table in ["work_order", "completion", "issue_line"] {
        assert!(
            has_zz_audit(db.migrate_pool(), "production_min", table).await,
            "production_min.{table} zz_audit_row"
        );
        assert_eq!(
            table_owner(db.migrate_pool(), "production_min", table).await,
            "datum_owner",
            "production_min.{table} owner"
        );
    }
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("prod_tx");
    boot_kernel(&db).await;
    let before: i64 = sql_query_scalar("SELECT count(*) FROM production_min.work_order")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let err = sql_query(
        r#"INSERT INTO production_min.work_order (
               id, number, item_id,
               quantity_ordered_amount, quantity_ordered_uom_id, quantity_ordered_dimension,
               revision, status, version, application_version, configuration_version
           ) VALUES (
               gen_random_uuid(), NULL, gen_random_uuid(),
               1, 1, 'Count', 'C', 'draft', 1, 't', ''
           )"#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after: i64 = sql_query_scalar("SELECT count(*) FROM production_min.work_order")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(after, before);
    db.finish().await.expect("finish");
}

fn trailing_int(s: &str) -> i64 {
    let digits: String = s
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse().expect("digits")
}

fn tree_has_lot(nodes: &[datum_ledger::Node], lot: datum_core::LotId) -> bool {
    nodes
        .iter()
        .any(|n| n.lot == Some(lot) || tree_has_lot(&n.children, lot))
}
