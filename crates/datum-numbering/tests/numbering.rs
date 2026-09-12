//! Named commit-mode tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_db::Tx;
use datum_numbering::{ResetPolicy, define, lot, next_number, serial, server_now};
use datum_test::db_case;
use sqlx::{Executor, Row};

use common::{migrate_and_install, test_ctx, write_pool, write_pool_wide};

#[tokio::test]
async fn abort_then_reallocate_returns_same_number() {
    let db = db_case!("abort_realloc");
    migrate_and_install(&db).await;
    let write = write_pool(&db).await;
    let ctx = test_ctx();

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin define");
    let seq = define(&mut tx, "WO", "WO-{yyyy}-{0000}", ResetPolicy::Yearly)
        .await
        .expect("define");
    tx.commit().await.expect("commit define");

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin abort");
    let first = next_number(&mut tx, seq.clone()).await.expect("alloc 1");
    tx.rollback().await.expect("rollback");

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin retry");
    let second = next_number(&mut tx, seq).await.expect("alloc 2");
    tx.commit().await.expect("commit retry");

    assert_eq!(
        first, second,
        "aborted allocation must not consume the number"
    );

    let n: i64 = db
        .app_pool()
        .fetch_one("SELECT count(*)::bigint FROM audit.event WHERE table_name = 'counter'")
        .await
        .expect("audit count")
        .try_get(0)
        .expect("n");
    assert_eq!(n, 0, "no audit row for numbering.counter");

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn concurrent_allocation_is_contiguous() {
    const N: usize = 8;
    let db = db_case!("concurrent");
    migrate_and_install(&db).await;
    let write = write_pool_wide(&db, (N as u32) + 2).await;
    let ctx = test_ctx();

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin define");
    let seq = define(&mut tx, "WO", "WO-{yyyy}-{0000}", ResetPolicy::Yearly)
        .await
        .expect("define");
    tx.commit().await.expect("commit define");

    let mut joins = Vec::new();
    for _ in 0..N {
        let write = write.clone();
        let ctx = ctx.clone();
        let seq = seq.clone();
        joins.push(tokio::spawn(async move {
            let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
            let n = next_number(&mut tx, seq).await.expect("alloc");
            tx.commit().await.expect("commit");
            n
        }));
    }
    let mut got = Vec::new();
    for j in joins {
        got.push(j.await.expect("join"));
    }
    got.sort();
    let mut nums: Vec<i64> = got
        .iter()
        .map(|s| s.rsplit('-').next().expect("suffix").parse().expect("int"))
        .collect();
    nums.sort();
    nums.dedup();
    assert_eq!(nums.len(), N, "duplicates in {got:?}");
    assert_eq!(nums[0], 1, "must start at 1: {got:?}");
    assert_eq!(nums[N - 1], N as i64, "must be contiguous: {got:?}");
    for window in nums.windows(2) {
        assert_eq!(window[1], window[0] + 1, "gap in {got:?}");
    }

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn period_rollover_uses_server_time() {
    let db = db_case!("period_now");
    migrate_and_install(&db).await;
    let write = write_pool(&db).await;
    let ctx = test_ctx();

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let stamp = server_now(&mut tx).await.expect("now");
    let seq = define(&mut tx, "WO", "WO-{yyyy}-{mm}-{0000}", ResetPolicy::Monthly)
        .await
        .expect("define");
    let number = next_number(&mut tx, seq).await.expect("alloc");
    tx.commit().await.expect("commit");

    let year = &stamp[..4];
    let month = &stamp[5..7];
    let expect_prefix = format!("WO-{year}-{month}-");
    assert!(
        number.starts_with(&expect_prefix),
        "number {number} must use server now {stamp}, not the client clock"
    );

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn format_templates_render_exactly() {
    let db = db_case!("formats");
    migrate_and_install(&db).await;
    let write = write_pool(&db).await;
    let ctx = test_ctx();

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let stamp = server_now(&mut tx).await.expect("now");
    let yyyy = &stamp[..4];
    let yy = &stamp[2..4];
    let mm = &stamp[5..7];

    let wo = define(&mut tx, "WO", "WO-{yyyy}-{0000}", ResetPolicy::Yearly)
        .await
        .expect("wo");
    assert_eq!(
        next_number(&mut tx, wo).await.expect("wo n"),
        format!("WO-{yyyy}-0001")
    );

    let inv = define(&mut tx, "INV", "INV-{00000}", ResetPolicy::Never)
        .await
        .expect("inv");
    assert_eq!(next_number(&mut tx, inv).await.expect("inv n"), "INV-00001");

    let po = define(&mut tx, "PO", "PO-{yy}-{mm}-{00}", ResetPolicy::Monthly)
        .await
        .expect("po");
    assert_eq!(
        next_number(&mut tx, po).await.expect("po n"),
        format!("PO-{yy}-{mm}-01")
    );

    let tiny = define(&mut tx, "TINY", "T-{0}", ResetPolicy::Never)
        .await
        .expect("tiny");
    for _ in 1..=9 {
        next_number(&mut tx, tiny.clone()).await.expect("tiny ok");
    }
    let overflow = next_number(&mut tx, tiny).await.expect_err("must overflow");
    assert!(
        matches!(
            overflow,
            datum_numbering::Error::PaddingOverflow { width: 1, .. }
        ),
        "got {overflow:?}"
    );
    tx.rollback().await.expect("rollback overflow tx");

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn lot_id_charset_and_length_enforced() {
    lot::validate("LOT-BAR-24-4412").expect("accepted");
    assert!(lot::validate("lot-bar-24-4412").is_err());
    assert!(lot::validate("LOT BAR").is_err());
    assert!(lot::validate("ABCDEFGHIJKLMNOPQRSTU").is_err());

    let db = db_case!("lot_ids");
    migrate_and_install(&db).await;
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");

    let id = lot::generate(&mut tx, "LOT-BAR-{yy}-{0000}")
        .await
        .expect("generate");
    lot::validate(&id).expect("generated");
    assert!(id.starts_with("LOT-BAR-"));
    assert!(id.len() <= 20);

    assert!(lot::generate(&mut tx, "lot-bar-{0000}").await.is_err());
    assert!(lot::generate(&mut tx, "LOT BAR-{0000}").await.is_err());
    assert!(
        lot::generate(&mut tx, "ABCDEFGHIJKLMNOPQRSTU")
            .await
            .is_err()
    );

    let sn = serial::generate(&mut tx, "SN-450-{000000}")
        .await
        .expect("serial");
    serial::validate(&sn).expect("serial charset");

    tx.commit().await.expect("commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn counter_is_audit_exempt() {
    let db = db_case!("exempt");
    migrate_and_install(&db).await;

    sqlx::Executor::execute(
        db.migrate_pool(),
        "CREATE TABLE app.numbering_probe (id uuid PRIMARY KEY, n int NOT NULL)",
    )
    .await
    .expect("probe table");

    let probe_audited: bool = db
        .app_pool()
        .fetch_one(
            "SELECT EXISTS (
               SELECT 1 FROM pg_trigger t
               JOIN pg_class c ON c.oid = t.tgrelid
               JOIN pg_namespace n ON n.oid = c.relnamespace
               WHERE n.nspname = 'app' AND c.relname = 'numbering_probe'
                 AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
             )",
        )
        .await
        .expect("probe trigger")
        .try_get(0)
        .expect("bool");
    assert!(
        probe_audited,
        "event trigger must attach zz_audit_row to app tables"
    );

    let counter_audited: bool = db
        .app_pool()
        .fetch_one(
            "SELECT EXISTS (
               SELECT 1 FROM pg_trigger t
               JOIN pg_class c ON c.oid = t.tgrelid
               JOIN pg_namespace n ON n.oid = c.relnamespace
               WHERE n.nspname = 'numbering' AND c.relname = 'counter'
                 AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
             )",
        )
        .await
        .expect("counter trigger")
        .try_get(0)
        .expect("bool");
    assert!(
        !counter_audited,
        "numbering.counter must not carry zz_audit_row"
    );

    let reason: String = db
        .app_pool()
        .fetch_one(
            "SELECT reason FROM audit.exempt
              WHERE nspname = 'numbering' AND relname = 'counter'",
        )
        .await
        .expect("exempt row")
        .try_get(0)
        .expect("reason");
    assert!(
        reason.contains("a counter went up")
            && reason.contains("evidenced by the audited document"),
        "D3 exemption reason missing: {reason}"
    );

    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let seq = define(&mut tx, "WO", "WO-{0000}", ResetPolicy::Never)
        .await
        .expect("define");
    let _ = next_number(&mut tx, seq).await.expect("alloc");
    tx.commit().await.expect("commit");

    let n: i64 = db
        .app_pool()
        .fetch_one("SELECT count(*)::bigint FROM audit.event WHERE table_name = 'counter'")
        .await
        .expect("events")
        .try_get(0)
        .expect("n");
    assert_eq!(n, 0, "allocation must write no audit row");

    db.finish().await.expect("finish");
}
