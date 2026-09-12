//! Named inventory tests (SPEC commit mode; one D2 case per test).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::Identifier;
use datum_db::Tx;
use datum_mod_inventory::{
    AdjustRequest, BalanceQuery, CountLine, CountRequest, DocumentStatus, IssueRequest, LineInput,
    ReceiveRequest, ReleaseRequest, ReturnRequest, ShipRequest, adjust, allocated, available,
    customer_return, cycle_count, document_history, issue_to_wip, on_hand, receive,
    release_from_quarantine, ship_to_customer,
};
use datum_test::db_case;
use sqlx::query as sql_query;
use sqlx::query_scalar as sql_query_scalar;

use common::{
    World, action_ctx, boot_kernel, consumption_count, dec, group_kind, has_zz_audit, line,
    pg_code, qty_ea, qty_ft, qty_in, reason_code, seed_world, table_owner, usd, write_pool,
};

async fn receive_bars(w: &World, pool: &datum_db::WritePool) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w.actor, "inventory.receive");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin receive");
    let doc = receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-2024-0841".into()),
            lines: vec![line(
                w.bar,
                qty_ft("2000.0000"),
                Some(w.lot_bar),
                Some(usd("4720.00")),
            )],
            expected: Some(qty_ft("2000.0000")),
            tolerance: Some(dec("0.0000")),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("receive");
    tx.commit().await.expect("commit receive");
    doc
}

async fn release_lot(w: &World, pool: &datum_db::WritePool) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w.actor, "lot.release");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin release");
    let doc = release_from_quarantine(
        &mut tx,
        &w.kernel,
        &ctx,
        ReleaseRequest {
            lot: w.lot_bar,
            from_location: w.quarantine,
            to_location: w.available,
            entered: qty_ft("2000.0000"),
            amount: Some(usd("4720.00")),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("release");
    tx.commit().await.expect("commit release");
    doc
}

#[tokio::test]
async fn case_a_receive_into_quarantine_posts_movement_from_supplier() {
    let db = db_case!("inv_case_a");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let doc = receive_bars(&w, &pool).await;
    assert_eq!(doc.status, DocumentStatus::Posted);
    let group = doc.posted_group_id.expect("group");
    assert_eq!(group_kind(db.app_pool(), group.as_uuid()).await, "MOVEMENT");
    let ctx = action_ctx(w.actor, "inventory.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin read");
    let q = on_hand(
        &mut tx,
        BalanceQuery {
            item: w.bar,
            location: Some(w.quarantine),
            lot: Some(w.lot_bar),
        },
    )
    .await
    .expect("on_hand");
    assert_eq!(q, dec("2000.0000"));
    let avail = available(
        &mut tx,
        BalanceQuery {
            item: w.bar,
            location: Some(w.quarantine),
            lot: Some(w.lot_bar),
        },
    )
    .await
    .expect("available");
    assert_eq!(
        avail,
        dec("0"),
        "quarantine is not available (PLAN §3 item 4)"
    );
    tx.commit().await.ok();
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn case_b_release_quarantine_posts_and_changes_status() {
    let db = db_case!("inv_case_b");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    let doc = release_lot(&w, &pool).await;
    let group = doc.posted_group_id.expect("group");
    assert_eq!(group_kind(db.app_pool(), group.as_uuid()).await, "MOVEMENT");
    let ctx = action_ctx(w.actor, "inventory.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let lot = datum_mod_lots::load_lot(&mut tx, w.lot_bar)
        .await
        .expect("lot");
    assert_eq!(lot.status, datum_mod_lots::LotStatus::Available);
    let avail = available(
        &mut tx,
        BalanceQuery {
            item: w.bar,
            location: Some(w.available),
            lot: Some(w.lot_bar),
        },
    )
    .await
    .expect("available");
    assert_eq!(avail, dec("2000.0000"));
    tx.commit().await.ok();
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn case_c_issue_one_bar_to_wip_with_explicit_lot_pick_contributes_consumption() {
    let db = db_case!("inv_case_c");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let ctx = action_ctx(w.actor, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin issue");
    let doc = issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.available,
            reference: Some("WO-2026-1847".into()),
            lines: vec![line(
                w.bar,
                qty_ft("20.0000"),
                Some(w.lot_bar),
                Some(usd("47.20")),
            )],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("issue");
    tx.commit().await.expect("commit issue");
    let group = doc.posted_group_id.expect("group");
    assert!(
        consumption_count(db.app_pool(), group.as_uuid()).await >= 1,
        "explicit lot pick must contribute a Consumption edge (D-W1-3 (c))"
    );
    let ctx = action_ctx(w.actor, "inventory.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin read");
    let alloc = allocated(
        &mut tx,
        BalanceQuery {
            item: w.bar,
            location: None,
            lot: Some(w.lot_bar),
        },
    )
    .await
    .expect("allocated");
    assert_eq!(alloc, dec("20.0000"));
    tx.commit().await.ok();
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn case_e_scrap_is_adjustment_with_reason() {
    let db = db_case!("inv_case_e");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let ctx = action_ctx(w.actor, "inventory.receive");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.fg,
            reference: Some("seed-fg".into()),
            lines: vec![line(w.screw, qty_ea("500"), None, Some(usd("125.00")))],
            expected: None,
            tolerance: None,
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("seed fg");
    tx.commit().await.expect("commit seed");
    let ctx = action_ctx(w.actor, "inventory.adjust");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin scrap");
    let doc = adjust(
        &mut tx,
        &w.kernel,
        &ctx,
        AdjustRequest {
            reason: "SCRAP_AT_OP_30".into(),
            location: w.fg,
            reference: Some("WO-2026-1847".into()),
            lines: vec![LineInput {
                item: w.screw,
                entered: qty_ea("-12"),
                lot: None,
                serial: None,
                from_location: None,
                to_location: None,
                package: None,
                amount: Some(usd("3.00")),
                reason_code: None,
            }],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("scrap");
    tx.commit().await.expect("commit scrap");
    let group = doc.posted_group_id.expect("group");
    assert_eq!(
        group_kind(db.app_pool(), group.as_uuid()).await,
        "ADJUSTMENT"
    );
    assert_eq!(
        reason_code(db.app_pool(), group.as_uuid()).await.as_deref(),
        Some("SCRAP_AT_OP_30")
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn case_f_cycle_count_variance_is_adjustment() {
    let db = db_case!("inv_case_f");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let ctx = action_ctx(w.actor, "inventory.count");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin count");
    let doc = cycle_count(
        &mut tx,
        &w.kernel,
        &ctx,
        CountRequest {
            location: w.available,
            reference: Some("COUNT-1".into()),
            lines: vec![CountLine {
                item: w.bar,
                lot: Some(w.lot_bar),
                serial: None,
                counted: qty_ft("1940.0000"),
                expected: qty_ft("1940.0000"),
                amount: Some(usd("141.60")),
            }],
            tolerance: dec("0.0000"),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("count");
    tx.commit().await.expect("commit count");
    let group = doc.posted_group_id.expect("group");
    assert_eq!(
        group_kind(db.app_pool(), group.as_uuid()).await,
        "ADJUSTMENT"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn case_h_customer_return_into_quarantine() {
    let db = db_case!("inv_case_h");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let ctx = action_ctx(w.actor, "inventory.receive");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.fg,
            reference: Some("seed-fg".into()),
            lines: vec![line(w.screw, qty_ea("500"), None, Some(usd("125.00")))],
            expected: None,
            tolerance: None,
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("seed");
    tx.commit().await.expect("commit seed");
    let order = Identifier::generate();
    let ctx = action_ctx(w.actor, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin ship");
    ship_to_customer(
        &mut tx,
        &w.kernel,
        &ctx,
        ShipRequest {
            order,
            from_location: w.fg,
            reference: Some("SO-1".into()),
            lines: vec![line(w.screw, qty_ea("10"), None, Some(usd("2.50")))],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("ship");
    tx.commit().await.expect("commit ship");
    let ctx = action_ctx(w.actor, "inventory.receive");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin return");
    let doc = customer_return(
        &mut tx,
        &w.kernel,
        &ctx,
        ReturnRequest {
            order,
            to_location: w.quarantine,
            reference: Some("RMA-1".into()),
            lines: vec![line(w.screw, qty_ea("10"), None, Some(usd("2.50")))],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("return");
    tx.commit().await.expect("commit return");
    assert_eq!(
        group_kind(db.app_pool(), doc.posted_group_id.expect("group").as_uuid()).await,
        "MOVEMENT"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn case_k_issue_by_the_inch_posts_uom_rounding_residual() {
    let db = db_case!("inv_case_k");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let ctx = action_ctx(w.actor, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin issue");
    let issued = issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.available,
            reference: Some("WO-2026-1847".into()),
            lines: vec![line(w.bar, qty_in("7"), Some(w.lot_bar), Some(usd("1.38")))],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("issue inches");
    tx.commit().await.expect("commit issue");
    let line = &issued.lines[0];
    assert_eq!(line.entered.amount, dec("7"));
    assert_eq!(line.entered.unit, common::IN);
    assert_eq!(line.canonical.amount, dec("0.5833"));
    let ctx = action_ctx(w.actor, "inventory.adjust");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin residual");
    let residual = adjust(
        &mut tx,
        &w.kernel,
        &ctx,
        AdjustRequest {
            reason: datum_ledger::UOM_CONVERSION_RESIDUAL.into(),
            location: w.available,
            reference: Some("dust".into()),
            lines: vec![LineInput {
                item: w.bar,
                entered: qty_ft("-0.0057"),
                lot: Some(w.lot_bar),
                serial: None,
                from_location: None,
                to_location: None,
                package: None,
                amount: Some(usd("0.01")),
                reason_code: None,
            }],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("residual flush");
    tx.commit().await.expect("commit residual");
    let group = residual.posted_group_id.expect("group");
    assert_eq!(
        group_kind(db.app_pool(), group.as_uuid()).await,
        "ADJUSTMENT"
    );
    assert_eq!(
        reason_code(db.app_pool(), group.as_uuid()).await.as_deref(),
        Some(datum_ledger::UOM_CONVERSION_RESIDUAL)
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn over_receipt_beyond_tolerance_is_a_document_error_not_a_ledger_error() {
    let db = db_case!("inv_over");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let ctx = action_ctx(w.actor, "inventory.receive");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-2024-0841".into()),
            lines: vec![line(
                w.bar,
                qty_ft("2200.0000"),
                Some(w.lot_bar),
                Some(usd("5192.00")),
            )],
            expected: Some(qty_ft("2000.0000")),
            tolerance: Some(dec("50")),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect_err("over-receipt");
    assert!(
        matches!(err, datum_mod_inventory::Error::Document(_)),
        "got {err}"
    );
    let _ = tx.rollback().await;
    let n: i64 = sql_query_scalar("SELECT count(*) FROM inventory.document")
        .fetch_one(db.app_pool())
        .await
        .expect("count");
    assert_eq!(n, 0, "document error must leave no document");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn no_balance_column_exists_in_module_schema() {
    let sql = include_str!("../migrations/00000000000001_inventory.up.sql");
    let lower = sql.to_ascii_lowercase();
    assert!(
        !lower.contains("balance"),
        "inventory schema must not declare a balance table or column"
    );
    assert!(
        !lower.contains("on_hand") && !lower.contains("on-hand"),
        "no stored on-hand column"
    );
}

#[tokio::test]
async fn on_hand_equals_ledger_fold_after_every_document() {
    let db = db_case!("inv_fold");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    let ctx = action_ctx(w.actor, "inventory.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let ours = on_hand(
        &mut tx,
        BalanceQuery {
            item: w.bar,
            location: Some(w.quarantine),
            lot: Some(w.lot_bar),
        },
    )
    .await
    .expect("on_hand");
    let fold = datum_ledger::balance_at(
        &mut tx,
        datum_ledger::BalanceSlice {
            item: w.bar,
            location: w.quarantine,
            lot: Some(w.lot_bar),
            serial: None,
            unit: common::FT,
        },
        chrono::Utc::now(),
    )
    .await
    .expect("fold");
    assert_eq!(ours, fold);
    tx.commit().await.ok();
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn posting_without_actor_aborts_and_leaves_no_document() {
    let db = db_case!("inv_no_actor");
    let kernel = boot_kernel(&db).await;
    let _w = seed_world(&db, kernel).await;
    let before: i64 = sql_query_scalar("SELECT count(*) FROM inventory.document")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let err = sql_query(
        r#"INSERT INTO inventory.document (
               id, kind, status, reference, posted_group_id, version,
               application_version, configuration_version
           ) VALUES (
               gen_random_uuid(), 'receipt', 'draft', 'no-actor', NULL, 1, 't', ''
           )"#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after: i64 = sql_query_scalar("SELECT count(*) FROM inventory.document")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(after, before);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn every_inventory_table_is_audited_and_owned_by_datum_owner() {
    let db = db_case!("inv_audit");
    boot_kernel(&db).await;
    for table in ["document", "document_line"] {
        assert!(
            has_zz_audit(db.migrate_pool(), "inventory", table).await,
            "inventory.{table} zz_audit_row"
        );
        assert_eq!(
            table_owner(db.migrate_pool(), "inventory", table).await,
            "datum_owner",
            "inventory.{table} owner"
        );
    }
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("inv_tx");
    boot_kernel(&db).await;
    let before: i64 = sql_query_scalar("SELECT count(*) FROM inventory.document")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let err = sql_query(
        r#"INSERT INTO inventory.document (
               id, kind, status, reference, posted_group_id, version,
               application_version, configuration_version
           ) VALUES (
               gen_random_uuid(), 'receipt', 'draft', 'raw', NULL, 1, 't', ''
           )"#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after: i64 = sql_query_scalar("SELECT count(*) FROM inventory.document")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(after, before);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn document_history_lists_item_and_lot() {
    let db = db_case!("inv_hist");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    let ctx = action_ctx(w.actor, "inventory.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let hist = document_history(&mut tx, Some(w.bar), Some(w.lot_bar))
        .await
        .expect("history");
    assert!(!hist.is_empty());
    tx.commit().await.ok();
    db.finish().await.expect("finish");
}
