//! Named items tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::ItemId;
use datum_db::Tx;
use datum_ledger::load_stock_item;
use datum_test::db_case;
use items::domain::number_is_valid;
use items::{Kind, ListFilter, Status, UpdateItem, create, get, list, obsolete, release, update};
use sqlx::query as sql_query;

use common::{
    EA, MM, actor_with_item_perms, bar, boot_kernel, count_audit_action, create_ctx, edge_ctx,
    has_zz_audit, pg_code, post_one_receipt, screw, table_owner, update_ctx, write_pool,
};

#[tokio::test]
async fn create_item_writes_ledger_registry() {
    let db = db_case!("items_reg");
    let kernel = boot_kernel(&db).await;
    let write = write_pool(&db);
    let actor = actor_with_item_perms(&write).await;
    let mut tx = Tx::begin(&write, &create_ctx(actor)).await.expect("begin");
    let item = create(&mut tx, &kernel, screw()).await.expect("create");
    let stock = load_stock_item(&mut tx, item.id).await.expect("registry");
    tx.commit().await.expect("commit");
    let loaded = get(db.app_pool(), item.id).await.expect("get");
    assert_eq!(loaded.number, "MDS-450-M4x12");
    assert_eq!(item.number, "MDS-450-M4x12");
    assert_eq!(item.revision, "C");
    assert_eq!(item.kind, Kind::Make);
    assert_eq!(item.status, Status::Draft);
    assert_eq!(stock.item, item.id);
    assert_eq!(stock.stock_uom, EA);
    assert_eq!(stock.stock_scale, 0);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn item_number_charset_enforced() {
    assert!(number_is_valid("MDS-450-M4x12"));
    assert!(!number_is_valid("HAS SPACE"));
    assert!(!number_is_valid("bad_underscore"));
    assert!(!number_is_valid(&"X".repeat(41)));

    let db = db_case!("items_num");
    let kernel = boot_kernel(&db).await;
    let write = write_pool(&db);
    let actor = actor_with_item_perms(&write).await;
    let mut bad = screw();
    bad.number = "HAS SPACE".into();
    let mut tx = Tx::begin(&write, &create_ctx(actor)).await.expect("begin");
    let err = create(&mut tx, &kernel, bad).await.expect_err("charset");
    assert!(matches!(err, items::Error::InvalidNumber), "got {err:?}");
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn stock_unit_immutable_after_first_posting() {
    let db = db_case!("items_imm");
    let kernel = boot_kernel(&db).await;
    let write = write_pool(&db);
    let actor = actor_with_item_perms(&write).await;
    let mut tx = Tx::begin(&write, &create_ctx(actor)).await.expect("begin");
    let item = create(&mut tx, &kernel, screw()).await.expect("create");
    post_one_receipt(&mut tx, item.id).await;
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &update_ctx(actor)).await.expect("begin2");
    let err = update(
        &mut tx,
        item.id,
        UpdateItem {
            version: item.version,
            revision: None,
            description: None,
            kind: None,
            stock_uom: Some(MM),
            stock_scale: None,
            residual_tolerance: None,
            cost_method: None,
            standard: None,
        },
    )
    .await
    .expect_err("immutable");
    assert!(
        matches!(err, items::Error::StockMeasureImmutable),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn release_transitions_and_audits() {
    let db = db_case!("items_rel");
    let kernel = boot_kernel(&db).await;
    let write = write_pool(&db);
    let actor = actor_with_item_perms(&write).await;
    let mut tx = Tx::begin(&write, &create_ctx(actor)).await.expect("begin");
    let item = create(&mut tx, &kernel, screw()).await.expect("create");
    tx.commit().await.expect("commit");

    let ctx = edge_ctx(&kernel, actor, item.id, "release");
    let cfg = ctx.config_version.as_deref().unwrap_or("");
    assert!(!cfg.is_empty(), "config_version must be non-empty");
    assert_eq!(
        cfg, kernel.profile.spec_version,
        "config_version equals the profile spec"
    );
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin rel");
    let released = release(&mut tx, &kernel, &ctx, item.id)
        .await
        .expect("release");
    tx.commit().await.expect("commit rel");
    assert_eq!(released.status, Status::Released);

    let n = count_audit_action(db.app_pool(), "items.release", "item").await;
    assert_eq!(
        n, 1,
        "one audit row with action items.release on items.item"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn obsolete_item_cannot_be_released_again() {
    let db = db_case!("items_obs");
    let kernel = boot_kernel(&db).await;
    let write = write_pool(&db);
    let actor = actor_with_item_perms(&write).await;
    let mut tx = Tx::begin(&write, &create_ctx(actor)).await.expect("begin");
    let item = create(&mut tx, &kernel, screw()).await.expect("create");
    tx.commit().await.expect("commit");

    let ctx = edge_ctx(&kernel, actor, item.id, "release");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin rel");
    release(&mut tx, &kernel, &ctx, item.id)
        .await
        .expect("release");
    tx.commit().await.expect("commit rel");

    let ctx = edge_ctx(&kernel, actor, item.id, "obsolete");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin obs");
    let gone = obsolete(&mut tx, &kernel, &ctx, item.id)
        .await
        .expect("obsolete");
    tx.commit().await.expect("commit obs");
    assert_eq!(gone.status, Status::Obsolete);

    let ctx = edge_ctx(&kernel, actor, item.id, "release");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin again");
    let err = release(&mut tx, &kernel, &ctx, item.id)
        .await
        .expect_err("no re-release");
    assert!(
        matches!(
            err,
            items::Error::InvalidTransition { ref edge, ref status }
                if edge == "release" && status == "obsolete"
        ),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn optimistic_version_conflict_is_typed() {
    let db = db_case!("items_ver");
    let kernel = boot_kernel(&db).await;
    let write = write_pool(&db);
    let actor = actor_with_item_perms(&write).await;
    let mut tx = Tx::begin(&write, &create_ctx(actor)).await.expect("begin");
    let item = create(&mut tx, &kernel, screw()).await.expect("create");
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &update_ctx(actor)).await.expect("begin2");
    let err = update(
        &mut tx,
        item.id,
        UpdateItem {
            version: item.version + 9,
            revision: Some("D".into()),
            description: None,
            kind: None,
            stock_uom: None,
            stock_scale: None,
            residual_tolerance: None,
            cost_method: None,
            standard: None,
        },
    )
    .await
    .expect_err("conflict");
    assert!(matches!(err, items::Error::VersionConflict), "got {err:?}");
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn list_paginates_stably() {
    let db = db_case!("items_list");
    let kernel = boot_kernel(&db).await;
    let write = write_pool(&db);
    let actor = actor_with_item_perms(&write).await;
    let mut tx = Tx::begin(&write, &create_ctx(actor)).await.expect("begin");
    let a = create(&mut tx, &kernel, screw()).await.expect("a");
    let b = create(&mut tx, &kernel, bar()).await.expect("b");
    let mut c_new = screw();
    c_new.number = "G-1422".into();
    c_new.description = "Gage placeholder item".into();
    let c = create(&mut tx, &kernel, c_new).await.expect("c");
    tx.commit().await.expect("commit");

    let page1 = list(
        db.app_pool(),
        ListFilter {
            limit: Some(2),
            ..ListFilter::default()
        },
    )
    .await
    .expect("page1");
    assert_eq!(page1.data.len(), 2);
    assert!(page1.has_more);
    assert!(page1.next_cursor.is_some());
    let cursor = ItemId::from_uuid(
        uuid::Uuid::parse_str(page1.next_cursor.as_deref().unwrap()).expect("cursor uuid"),
    );
    let page2 = list(
        db.app_pool(),
        ListFilter {
            limit: Some(2),
            cursor: Some(cursor),
            ..ListFilter::default()
        },
    )
    .await
    .expect("page2");
    assert_eq!(page2.data.len(), 1);
    assert!(!page2.has_more);
    let ids1: Vec<_> = page1.data.iter().map(|i| i.id).collect();
    let ids2: Vec<_> = page2.data.iter().map(|i| i.id).collect();
    assert!(ids1.iter().all(|id| !ids2.contains(id)));
    let mut all = ids1;
    all.extend(ids2);
    let mut expected = [a.id, b.id, c.id];
    expected.sort();
    all.sort();
    assert_eq!(all, expected);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn every_items_table_is_audited_and_owned_by_datum_owner() {
    let db = db_case!("items_own");
    boot_kernel(&db).await;
    for table in ["item", "item_revision_history"] {
        assert!(
            has_zz_audit(db.migrate_pool(), "items", table).await,
            "items.{table} missing zz_audit_row"
        );
        assert_eq!(
            table_owner(db.migrate_pool(), "items", table).await,
            "datum_owner",
            "items.{table} owner"
        );
    }
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("items_tx");
    boot_kernel(&db).await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM items.item")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let err = sql_query(
        r#"INSERT INTO items.item (
               id, number, revision, description, kind,
               stock_uom_id, stock_scale, residual_tolerance, cost_method, status
           ) VALUES (
               gen_random_uuid(), 'RAW-WRITE-1', 'A', 'raw', 'make',
               1, 0, 0, 'FIFO', 'draft'
           )"#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM items.item")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(after, before);
    db.finish().await.expect("finish");
}
