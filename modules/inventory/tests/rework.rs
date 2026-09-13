//! Named rework tests (mod-inventory-c1).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use sqlx::query_scalar as sql_query_scalar;
use wicket_core::Identifier;
use wicket_db::Tx;
use wicket_mod_inventory::{
    ADJUSTED, AdjustRequest, BalanceQuery, CountLine, CountRequest, DOC_TYPE, IssueRequest,
    LineInput, MoveRequest, RECEIPT_POSTED, ReceiveRequest, ReleaseRequest, ReturnRequest,
    ShipRequest, adjust, available, customer_return, cycle_count, error_code, http_status,
    issue_to_wip, move_stock, on_hand, receive, release_from_quarantine, reverse_posted_issue,
    ship_to_customer, void_document,
};
use wicket_module::Profile;
use wicket_statemachine::DocRef;
use wicket_test::db_case;

use common::{
    World, action_ctx, boot_kernel, boot_kernel_with, consumption_count, dec, group_kind, line,
    qty_ea, qty_ft, qty_in, receive_bars, release_lot, residual_children_of, residual_group_for,
    seed_world, usd, write_pool,
};

async fn seed_fg_screws(w: &World, pool: &wicket_db::WritePool) {
    let ctx = action_ctx(w, "inventory.receive");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin");
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
    tx.commit().await.expect("commit");
}

fn assert_idempotency_conflict(err: wicket_mod_inventory::Error) {
    assert!(
        matches!(err, wicket_mod_inventory::Error::IdempotencyConflict),
        "got {err}"
    );
    assert_eq!(error_code(&err), "IDEMPOTENCY_CONFLICT");
    assert_eq!(http_status(&err), 409);
}

#[tokio::test]
async fn lot_less_move_without_line_amount_posts_from_cost_layers() {
    let db = db_case!("inv_r1_move");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.move");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let doc = move_stock(
        &mut tx,
        &w.kernel,
        &ctx,
        wicket_mod_inventory::MoveRequest {
            from_location: w.fg,
            to_location: w.available,
            reference: Some("lot-less-move".into()),
            lines: vec![LineInput {
                item: w.screw,
                entered: qty_ea("10"),
                lot: None,
                serial: None,
                from_location: None,
                to_location: None,
                package: None,
                amount: None,
                reason_code: None,
            }],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("move");
    tx.commit().await.expect("commit");
    assert_eq!(doc.status, wicket_mod_inventory::DocumentStatus::Posted);
    let group = doc.posted_group_id.expect("group");
    assert_eq!(group_kind(db.app_pool(), group.as_uuid()).await, "MOVEMENT");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn lot_less_issue_without_line_amount_posts_from_cost_layers() {
    let db = db_case!("inv_r1_issue");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let doc = issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.fg,
            reference: Some("lot-less-issue".into()),
            lines: vec![LineInput {
                item: w.screw,
                entered: qty_ea("5"),
                lot: None,
                serial: None,
                from_location: None,
                to_location: None,
                package: None,
                amount: None,
                reason_code: None,
            }],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("issue");
    tx.commit().await.expect("commit");
    assert!(doc.posted_group_id.is_some());
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn one_ledger_group_per_movement_uses_transition_sink() {
    let db = db_case!("inv_r2_group");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let before: i64 = sql_query_scalar("SELECT count(*) FROM ledger.posting_group")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let ctx = action_ctx(&w, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.available,
            reference: Some("one-group".into()),
            lines: vec![line(w.bar, qty_ft("1.0000"), Some(w.lot_bar), None)],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("issue");
    tx.commit().await.expect("commit");
    let after: i64 = sql_query_scalar("SELECT count(*) FROM ledger.posting_group")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(
        after - before,
        1,
        "exactly one posting group for the movement"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_receive_replay_and_conflict() {
    let db = db_case!("inv_r3_recv");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let key = uuid::Uuid::now_v7();
    let ctx = action_ctx(&w, "inventory.receive");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let first = receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-1".into()),
            lines: vec![line(
                w.bar,
                qty_ft("10"),
                Some(w.lot_bar),
                Some(usd("23.60")),
            )],
            expected: None,
            tolerance: None,
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let replay = receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-1".into()),
            lines: vec![line(
                w.bar,
                qty_ft("10"),
                Some(w.lot_bar),
                Some(usd("23.60")),
            )],
            expected: None,
            tolerance: None,
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("replay");
    tx.commit().await.ok();
    assert_eq!(first.id, replay.id);
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-1".into()),
            lines: vec![line(
                w.bar,
                qty_ft("11"),
                Some(w.lot_bar),
                Some(usd("25.96")),
            )],
            expected: None,
            tolerance: None,
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_issue_conflict() {
    let db = db_case!("inv_r3_issue");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let key = uuid::Uuid::now_v7();
    let ctx = action_ctx(&w, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.available,
            reference: Some("WO-1".into()),
            lines: vec![line(w.bar, qty_ft("1.0000"), Some(w.lot_bar), None)],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.available,
            reference: Some("WO-1".into()),
            lines: vec![line(w.bar, qty_ft("2.0000"), Some(w.lot_bar), None)],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_move_conflict() {
    let db = db_case!("inv_r3_move");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let key = uuid::Uuid::now_v7();
    let ctx = action_ctx(&w, "inventory.move");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    move_stock(
        &mut tx,
        &w.kernel,
        &ctx,
        MoveRequest {
            from_location: w.fg,
            to_location: w.available,
            reference: Some("move-1".into()),
            lines: vec![line(w.screw, qty_ea("10"), None, None)],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = move_stock(
        &mut tx,
        &w.kernel,
        &ctx,
        MoveRequest {
            from_location: w.fg,
            to_location: w.available,
            reference: Some("move-1".into()),
            lines: vec![line(w.screw, qty_ea("11"), None, None)],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_adjust_conflict() {
    let db = db_case!("inv_r3_adj");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let key = uuid::Uuid::now_v7();
    let ctx = action_ctx(&w, "inventory.adjust");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    adjust(
        &mut tx,
        &w.kernel,
        &ctx,
        AdjustRequest {
            reason: "SCRAP_AT_OP_30".into(),
            location: w.fg,
            reference: Some("WO-1".into()),
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
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = adjust(
        &mut tx,
        &w.kernel,
        &ctx,
        AdjustRequest {
            reason: "SCRAP_AT_OP_30".into(),
            location: w.fg,
            reference: Some("WO-1".into()),
            lines: vec![LineInput {
                item: w.screw,
                entered: qty_ea("-11"),
                lot: None,
                serial: None,
                from_location: None,
                to_location: None,
                package: None,
                amount: Some(usd("2.75")),
                reason_code: None,
            }],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_cycle_count_conflict() {
    let db = db_case!("inv_r3_count");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let key = uuid::Uuid::now_v7();
    let ctx = action_ctx(&w, "inventory.count");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    cycle_count(
        &mut tx,
        &w.kernel,
        &ctx,
        CountRequest {
            location: w.fg,
            reference: Some("COUNT-1".into()),
            lines: vec![CountLine {
                item: w.screw,
                lot: None,
                serial: None,
                counted: qty_ea("512"),
                expected: qty_ea("512"),
                amount: None,
            }],
            tolerance: dec("0"),
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = cycle_count(
        &mut tx,
        &w.kernel,
        &ctx,
        CountRequest {
            location: w.fg,
            reference: Some("COUNT-1".into()),
            lines: vec![CountLine {
                item: w.screw,
                lot: None,
                serial: None,
                counted: qty_ea("500"),
                expected: qty_ea("500"),
                amount: None,
            }],
            tolerance: dec("0"),
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_release_from_quarantine_conflict() {
    let db = db_case!("inv_r3_rel");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    let key = uuid::Uuid::now_v7();
    let ctx = action_ctx(&w, "lot.release");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    release_from_quarantine(
        &mut tx,
        &w.kernel,
        &ctx,
        ReleaseRequest {
            lot: w.lot_bar,
            from_location: w.quarantine,
            to_location: w.available,
            entered: qty_ft("2000.0000"),
            amount: Some(usd("4720.00")),
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = release_from_quarantine(
        &mut tx,
        &w.kernel,
        &ctx,
        ReleaseRequest {
            lot: w.lot_bar,
            from_location: w.quarantine,
            to_location: w.available,
            entered: qty_ft("1999.0000"),
            amount: Some(usd("4717.64")),
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_customer_return_conflict() {
    let db = db_case!("inv_r3_ret");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let order = wicket_core::Identifier::generate();
    let ctx = action_ctx(&w, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
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
    tx.commit().await.expect("commit");
    let key = uuid::Uuid::now_v7();
    let ctx = action_ctx(&w, "inventory.receive");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    customer_return(
        &mut tx,
        &w.kernel,
        &ctx,
        ReturnRequest {
            order,
            to_location: w.quarantine,
            reference: Some("RMA-1".into()),
            lines: vec![line(w.screw, qty_ea("10"), None, Some(usd("2.50")))],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = customer_return(
        &mut tx,
        &w.kernel,
        &ctx,
        ReturnRequest {
            order,
            to_location: w.quarantine,
            reference: Some("RMA-1".into()),
            lines: vec![line(w.screw, qty_ea("9"), None, Some(usd("2.25")))],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn idempotency_ship_conflict() {
    let db = db_case!("inv_r3_ship");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let key = uuid::Uuid::now_v7();
    let order = wicket_core::Identifier::generate();
    let ctx = action_ctx(&w, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    ship_to_customer(
        &mut tx,
        &w.kernel,
        &ctx,
        ShipRequest {
            order,
            from_location: w.fg,
            reference: Some("SO-1".into()),
            lines: vec![line(w.screw, qty_ea("10"), None, Some(usd("2.50")))],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect("first");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = ship_to_customer(
        &mut tx,
        &w.kernel,
        &ctx,
        ShipRequest {
            order,
            from_location: w.fg,
            reference: Some("SO-1".into()),
            lines: vec![line(w.screw, qty_ea("11"), None, Some(usd("2.75")))],
            idempotency_key: Some(key),
        },
    )
    .await
    .expect_err("conflict");
    assert_idempotency_conflict(err);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn available_excludes_quarantine_unless_lot_is_named() {
    let db = db_case!("inv_r4_avail");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let all = available(
        &mut tx,
        BalanceQuery {
            item: w.bar,
            location: None,
            lot: None,
        },
    )
    .await
    .expect("available");
    assert_eq!(all, dec("0"));
    let named = available(
        &mut tx,
        BalanceQuery {
            item: w.bar,
            location: Some(w.quarantine),
            lot: Some(w.lot_bar),
        },
    )
    .await
    .expect("named");
    assert_eq!(
        named,
        dec("0"),
        "quarantine lot named but not available status"
    );
    tx.commit().await.ok();
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn issue_refuses_quarantined_lot_with_typed_error() {
    let db = db_case!("inv_r4_issue_lot");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let err = issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.quarantine,
            reference: None,
            lines: vec![line(w.bar, qty_ft("1"), Some(w.lot_bar), None)],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect_err("quarantine");
    assert!(matches!(err, wicket_mod_inventory::Error::LotNotIssuable));
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn issue_reversal_restores_consumption() {
    let db = db_case!("inv_r5_rev");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    receive_bars(&w, &pool).await;
    release_lot(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.issue");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let doc = issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        IssueRequest {
            work_order: w.wo,
            from_location: w.available,
            reference: Some("WO-1".into()),
            lines: vec![line(w.bar, qty_in("7"), Some(w.lot_bar), Some(usd("1.38")))],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("issue");
    tx.commit().await.expect("commit");
    let group = doc.posted_group_id.expect("group");
    let before = consumption_count(db.app_pool(), group.as_uuid()).await;
    assert!(before >= 1);
    let children = residual_children_of(&pool, &ctx, group).await;
    assert_eq!(
        children.len(),
        1,
        "issue movement must have one residual child"
    );
    let residual = children[0].as_uuid();
    assert_eq!(
        residual,
        residual_group_for(db.app_pool(), group.as_uuid()).await
    );
    assert_eq!(group_kind(db.app_pool(), residual).await, "ADJUSTMENT");
    assert_eq!(
        common::reason_code(db.app_pool(), residual)
            .await
            .as_deref(),
        Some(wicket_ledger::UOM_CONVERSION_RESIDUAL)
    );
    let tag: String =
        sql_query_scalar("SELECT source_kind FROM ledger.posting_group WHERE group_id = $1")
            .bind(residual)
            .fetch_one(db.app_pool())
            .await
            .expect("source_kind");
    assert_eq!(tag, common::residual_parent_tag(group.as_uuid()));
    common::assert_group_conserves(db.app_pool(), group.as_uuid()).await;
    common::assert_group_conserves(db.app_pool(), residual).await;
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let rev = reverse_posted_issue(&mut tx, &w.kernel, &ctx, doc.id)
        .await
        .expect("reverse");
    tx.commit().await.expect("commit");
    assert_eq!(group_kind(db.app_pool(), rev.as_uuid()).await, "REVERSAL");
    let cons = consumption_count(db.app_pool(), rev.as_uuid()).await;
    assert!(cons >= 1, "reversal restores consumption edges");
    common::assert_group_conserves(db.app_pool(), group.as_uuid()).await;
    common::assert_group_conserves(db.app_pool(), residual).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn cycle_count_over_and_under_adjust_on_hand_with_correct_sign() {
    let db = db_case!("inv_r6_count");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.count");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let before: i64 = sql_query_scalar(
        "SELECT count(*) FROM ledger.posting_group WHERE reason_code LIKE 'CYCLE_COUNT%'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("before");
    cycle_count(
        &mut tx,
        &w.kernel,
        &ctx,
        CountRequest {
            location: w.fg,
            reference: Some("over".into()),
            lines: vec![CountLine {
                item: w.screw,
                lot: None,
                serial: None,
                counted: qty_ea("512"),
                expected: qty_ea("512"),
                amount: None,
            }],
            tolerance: dec("0"),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("over count");
    tx.commit().await.expect("commit");
    let ctx = action_ctx(&w, "inventory.view");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let on = on_hand(
        &mut tx,
        BalanceQuery {
            item: w.screw,
            location: Some(w.fg),
            lot: None,
        },
    )
    .await
    .expect("on_hand");
    assert_eq!(on, dec("512"));
    tx.commit().await.ok();
    let mut tx = Tx::begin(&pool, &action_ctx(&w, "inventory.count"))
        .await
        .expect("begin");
    cycle_count(
        &mut tx,
        &w.kernel,
        &action_ctx(&w, "inventory.count"),
        CountRequest {
            location: w.fg,
            reference: Some("under".into()),
            lines: vec![CountLine {
                item: w.screw,
                lot: None,
                serial: None,
                counted: qty_ea("500"),
                expected: qty_ea("500"),
                amount: None,
            }],
            tolerance: dec("0"),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("under");
    tx.commit().await.expect("commit");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let on = on_hand(
        &mut tx,
        BalanceQuery {
            item: w.screw,
            location: Some(w.fg),
            lot: None,
        },
    )
    .await
    .expect("on_hand");
    assert_eq!(on, dec("500"));
    let after: i64 = sql_query_scalar(
        "SELECT count(*) FROM ledger.posting_group WHERE reason_code LIKE 'CYCLE_COUNT%'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("after");
    assert_eq!(after - before, 2);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn void_document_updates_status_and_is_audited() {
    let db = db_case!("inv_r6_void");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let doc = receive_bars(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.void");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let voided = void_document(&mut tx, &w.kernel, &ctx, doc.id)
        .await
        .expect("void");
    tx.commit().await.expect("commit");
    assert_eq!(voided.status, wicket_mod_inventory::DocumentStatus::Voided);
    let n: i64 = sql_query_scalar(
        "SELECT count(*) FROM audit.event WHERE table_name = 'document' AND op = 'UPDATE'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("audit");
    assert!(n >= 1);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn lot_less_receipt_publishes_receipt_posted_event() {
    let db = db_case!("inv_c3_evt_receipt");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    let ctx = action_ctx(&w, "inventory.receive");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let doc = receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.fg,
            reference: Some("lot-less-evt".into()),
            lines: vec![line(w.screw, qty_ea("25"), None, Some(usd("6.25")))],
            expected: None,
            tolerance: None,
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("receive");
    tx.commit().await.expect("commit");
    let name: String = sql_query_scalar("SELECT name FROM app.event WHERE doc_id = $1 LIMIT 1")
        .bind(doc.id.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("outbox row");
    assert_eq!(name, RECEIPT_POSTED);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn adjust_publishes_adjusted_event() {
    let db = db_case!("inv_c3_evt_adjust");
    let kernel = boot_kernel(&db).await;
    let w = seed_world(&db, kernel).await;
    let pool = write_pool(&db);
    seed_fg_screws(&w, &pool).await;
    let ctx = action_ctx(&w, "inventory.adjust");
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let doc = adjust(
        &mut tx,
        &w.kernel,
        &ctx,
        AdjustRequest {
            reason: "SCRAP_AT_OP_30".into(),
            location: w.fg,
            reference: Some("adj-evt".into()),
            lines: vec![LineInput {
                item: w.screw,
                entered: qty_ea("-2"),
                lot: None,
                serial: None,
                from_location: None,
                to_location: None,
                package: None,
                amount: Some(usd("0.50")),
                reason_code: None,
            }],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("adjust");
    tx.commit().await.expect("commit");
    let name: String = sql_query_scalar("SELECT name FROM app.event WHERE doc_id = $1 LIMIT 1")
        .bind(doc.id.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("outbox row");
    assert_eq!(name, ADJUSTED);
    db.finish().await.expect("finish");
}

async fn assert_config_version_on_profile(db: &wicket_test::TestDb, profile: Profile) {
    let spec = profile.spec_version.clone();
    let kernel = boot_kernel_with(db, profile).await;
    assert_eq!(kernel.profile.spec_version, spec);
    let w = seed_world(db, kernel).await;
    let doc = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::generate(),
    };
    let ctx = w.kernel.transition_context(w.actor, &doc, "receive");
    let cfg = ctx.config_version.as_deref().unwrap_or("");
    assert!(
        !cfg.is_empty(),
        "transition context config_version must be non-empty"
    );
    assert_eq!(
        cfg, spec,
        "transition context config_version equals the profile spec"
    );
    let pool = write_pool(db);
    let ctx = action_ctx(&w, "inventory.receive");
    assert_eq!(ctx.config_version.as_deref(), Some(spec.as_str()));
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let receipt = receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-cfg".into()),
            lines: vec![line(
                w.bar,
                qty_ft("10"),
                Some(w.lot_bar),
                Some(usd("23.60")),
            )],
            expected: None,
            tolerance: None,
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("receive");
    tx.commit().await.expect("commit");
    assert!(
        !receipt.configuration_version.is_empty(),
        "receipt configuration_version must be non-empty"
    );
    assert_eq!(
        receipt.configuration_version, spec,
        "receipt configuration_version equals the profile spec"
    );
}

#[tokio::test]
async fn config_version_plain_shop_transition_and_receipt() {
    let db = db_case!("inv_cfg_plain");
    assert_config_version_on_profile(&db, Profile::plain_shop().unwrap()).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn config_version_regulated_device_transition_and_receipt() {
    let db = db_case!("inv_cfg_reg");
    assert_config_version_on_profile(&db, Profile::regulated_device().unwrap()).await;
    db.finish().await.expect("finish");
}
