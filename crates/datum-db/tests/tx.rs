//! Tx, pool hooks, D3 §10 rows c and d.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies
)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use datum_db::{DATUM_SETTINGS, Error, Tx, WritePool, retry_serializable};
use datum_test::db_case;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;

use common::{
    app_url, connect_app_raw, ctx_named, install_require_context_stub, pg_code, test_ctx,
    write_pool,
};

#[tokio::test]
async fn tx_actor_bound_to_transaction_id() {
    let db = db_case!("tx_actor");
    let write = WritePool::new(db.app_pool().clone());
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let stored = tx.setting("datum.txid").await.expect("txid guc");
    let live = tx.pg_txid().await.expect("pg_current_xact_id");
    assert_eq!(stored, live, "datum.txid must equal pg_current_xact_id()");
    let actor = tx.setting("datum.actor_id").await.expect("actor_id");
    assert_eq!(actor, ctx.actor.id.as_uuid().to_string());
    tx.commit().await.expect("commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn connect_sets_utc_and_application_name() {
    let db = db_case!("connect_utc");
    let pool = datum_db::connect(&app_url(db.database()))
        .await
        .expect("connect");
    let row: (String, String) =
        sqlx::query_as("SELECT current_setting('TimeZone'), current_setting('application_name')")
            .fetch_one(&pool)
            .await
            .expect("settings");
    assert_eq!(row.0.to_uppercase(), "UTC");
    assert_eq!(row.1, "datum");
    pool.close().await;
    db.finish().await.expect("finish");
}

/// All eighteen `datum.*` settings readable inside the transaction, none after commit.
#[tokio::test]
async fn tx_sets_every_setting() {
    let db = db_case!("every_setting");
    let write = write_pool(db.database(), 2).await;
    let mut ctx = test_ctx();
    ctx.actor_display = Some("Ada".into());
    ctx.acting_for = Some("for-1".into());
    ctx.session_id = Some("sess-1".into());
    ctx.request_id = Some("req-1".into());
    ctx.source_device = Some("dev-1".into());
    ctx.source_ip = Some("127.0.0.1".into());
    ctx.client_app = Some("test".into());
    ctx.reason = Some("because".into());
    ctx.doc_type = Some("wo".into());
    ctx.doc_id = Some("doc-1".into());
    ctx.esign_id = Some("es-1".into());
    ctx.config_version = Some("cfg-1".into());

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    assert_eq!(DATUM_SETTINGS.len(), 18);
    for name in DATUM_SETTINGS {
        let value = tx.setting(name).await.expect(name);
        assert!(
            !value.is_empty(),
            "{name} must be set inside the transaction"
        );
    }
    assert_eq!(
        tx.setting("datum.actor_id").await.unwrap(),
        ctx.actor.id.as_uuid().to_string()
    );
    assert_eq!(tx.setting("datum.actor_kind").await.unwrap(), "User");
    assert_eq!(tx.setting("datum.actor_display").await.unwrap(), "Ada");
    assert_eq!(tx.setting("datum.acting_for").await.unwrap(), "for-1");
    assert_eq!(tx.setting("datum.session_id").await.unwrap(), "sess-1");
    assert_eq!(tx.setting("datum.request_id").await.unwrap(), "req-1");
    assert_eq!(tx.setting("datum.source_kind").await.unwrap(), "test");
    assert_eq!(tx.setting("datum.source_device").await.unwrap(), "dev-1");
    assert_eq!(tx.setting("datum.source_ip").await.unwrap(), "127.0.0.1");
    assert_eq!(tx.setting("datum.client_app").await.unwrap(), "test");
    assert_eq!(tx.setting("datum.action").await.unwrap(), "test.begin");
    assert_eq!(tx.setting("datum.reason").await.unwrap(), "because");
    assert_eq!(tx.setting("datum.doc_type").await.unwrap(), "wo");
    assert_eq!(tx.setting("datum.doc_id").await.unwrap(), "doc-1");
    assert_eq!(tx.setting("datum.esign_id").await.unwrap(), "es-1");
    assert_eq!(tx.setting("datum.config_version").await.unwrap(), "cfg-1");
    let app_version = tx.setting("datum.app_version").await.unwrap();
    assert!(
        app_version.starts_with(env!("CARGO_PKG_VERSION")),
        "app_version={app_version}"
    );
    let txid = tx.setting("datum.txid").await.unwrap();
    assert_eq!(txid, tx.pg_txid().await.unwrap());
    tx.commit().await.expect("commit");

    let pool = datum_db::connect(&app_url(db.database()))
        .await
        .expect("probe pool");
    let mut probe = pool.begin().await.expect("probe");
    for name in DATUM_SETTINGS {
        let leaked: (Option<String>,) =
            sqlx::query_as("SELECT pg_catalog.current_setting($1, true)")
                .bind(*name)
                .fetch_one(&mut *probe)
                .await
                .expect("probe");
        assert!(
            leaked.0.as_deref().unwrap_or("").is_empty(),
            "{name} leaked after commit: {:?}",
            leaked.0
        );
    }
    probe.rollback().await.expect("rollback");
    pool.close().await;
    db.finish().await.expect("finish");
}

/// D3 §10 row **c**: a write on a raw pool connection (no `Tx::begin`) into a
/// table carrying the D3 trigger stub aborts with `42501` and the table is
/// unchanged.
#[tokio::test]
async fn write_without_context_aborts() {
    let db = db_case!("no_ctx");
    install_require_context_stub(db.migrate_pool()).await;
    let err = sqlx::query("INSERT INTO app.probe (id) VALUES (1)")
        .execute(db.app_pool())
        .await
        .expect_err("raw write must abort");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM app.probe")
        .fetch_one(db.migrate_pool())
        .await
        .expect("count");
    assert_eq!(n, 0, "table must be unchanged");
    db.finish().await.expect("finish");
}

/// D3 §10 row **d**: pool `max_connections(1)`, two sequential `Tx::begin` by
/// different actors; `current_setting('datum.actor_id')` differs per
/// transaction and is empty after commit.
#[tokio::test]
async fn pooled_connection_cannot_leak_actor() {
    let db = db_case!("leak_actor");
    let write = write_pool(db.database(), 1).await;
    let ctx_a = ctx_named("actor.a");
    let ctx_b = ctx_named("actor.b");
    assert_ne!(ctx_a.actor.id, ctx_b.actor.id);

    let mut tx = Tx::begin(&write, &ctx_a).await.expect("begin a");
    let a = tx.setting("datum.actor_id").await.expect("a");
    assert_eq!(a, ctx_a.actor.id.as_uuid().to_string());
    tx.commit().await.expect("commit a");

    let mut tx = Tx::begin(&write, &ctx_b).await.expect("begin b");
    let b = tx.setting("datum.actor_id").await.expect("b");
    assert_eq!(b, ctx_b.actor.id.as_uuid().to_string());
    assert_ne!(a, b);
    tx.commit().await.expect("commit b");

    let pool = datum_db::connect_with(
        &app_url(db.database()),
        PgPoolOptions::new().max_connections(1),
    )
    .await
    .expect("probe");
    let leaked: (Option<String>,) =
        sqlx::query_as("SELECT pg_catalog.current_setting('datum.actor_id', true)")
            .fetch_one(&pool)
            .await
            .expect("probe setting");
    assert!(
        leaked.0.as_deref().unwrap_or("").is_empty(),
        "actor_id leaked after commit: {:?}",
        leaked.0
    );
    pool.close().await;
    db.finish().await.expect("finish");
}

/// D3 §10 row **d**: session-level `datum.actor_id` set by hand on a raw
/// connection; the next write is refused by the `datum.txid` check.
#[tokio::test]
async fn session_level_actor_is_refused() {
    let db = db_case!("session_actor");
    install_require_context_stub(db.migrate_pool()).await;
    let mut conn = connect_app_raw(db.database()).await;
    sqlx::query("SELECT pg_catalog.set_config('datum.actor_id', $1, false)")
        .bind(datum_core::Identifier::generate().as_uuid().to_string())
        .execute(&mut conn)
        .await
        .expect("session-level actor_id");
    let err = sqlx::query("INSERT INTO app.probe (id) VALUES (1)")
        .execute(&mut conn)
        .await
        .expect_err("leaked actor must not write");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM app.probe")
        .fetch_one(db.migrate_pool())
        .await
        .expect("count");
    assert_eq!(n, 0);
    drop(conn);
    db.finish().await.expect("finish");
}

/// Addendum 3: startup-parameter defaults survive `RESET ALL` on pool release.
#[tokio::test]
async fn connection_defaults_survive_release() {
    let db = db_case!("conn_defaults_release");
    let pool = datum_db::connect_with(
        &app_url(db.database()),
        PgPoolOptions::new().max_connections(1),
    )
    .await
    .expect("connect");
    {
        let _conn = pool.acquire().await.expect("checkout 1");
    }
    {
        let mut conn = pool.acquire().await.expect("checkout 2 after release");
        let tz: (String,) = sqlx::query_as("SHOW timezone")
            .fetch_one(&mut *conn)
            .await
            .expect("timezone");
        let app: (String,) = sqlx::query_as("SHOW application_name")
            .fetch_one(&mut *conn)
            .await
            .expect("application_name");
        let idle: (String,) = sqlx::query_as("SHOW idle_in_transaction_session_timeout")
            .fetch_one(&mut *conn)
            .await
            .expect("idle_in_transaction_session_timeout");
        assert_eq!(tz.0.to_uppercase(), "UTC");
        assert_eq!(app.0, "datum");
        let idle_timeout = idle.0;
        assert!(
            idle_timeout == "15s" || idle_timeout == "15000ms",
            "idle_in_transaction_session_timeout={idle_timeout:?}"
        );
    }
    pool.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn after_release_resets_state() {
    let db = db_case!("after_release");
    let write = write_pool(db.database(), 1).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute("SELECT pg_catalog.set_config('datum.pool_probe', 'leaked', false)")
        .await
        .expect("session guc");
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin 2");
    let leaked = tx.setting("datum.pool_probe").await.expect("probe");
    assert!(
        leaked.is_empty(),
        "WritePool after_release must RESET ALL: {leaked:?}"
    );
    tx.commit().await.expect("commit 2");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn serialization_retry_reruns_on_40001() {
    let db = db_case!("ser_retry");
    sqlx::query("CREATE TABLE app.counters (id int PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    sqlx::query("INSERT INTO app.counters (id, n) VALUES (1, 0)")
        .execute(db.migrate_pool())
        .await
        .expect("seed");

    let write = Arc::new(write_pool(db.database(), 4).await);
    let attempts = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let bump = |write: Arc<WritePool>,
                attempts: Arc<AtomicU32>,
                barrier: Arc<tokio::sync::Barrier>| async move {
        retry_serializable(
            || {
                let write = Arc::clone(&write);
                let attempts = Arc::clone(&attempts);
                let barrier = Arc::clone(&barrier);
                async move {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    let ctx = ctx_named("ser.bump");
                    let mut tx = Tx::begin_serializable(&write, &ctx).await?;
                    let (n,): (i32,) = tx
                        .fetch_one(sqlx::query_as("SELECT n FROM app.counters WHERE id = 1"))
                        .await?;
                    if attempt < 2 {
                        barrier.wait().await;
                    }
                    tx.execute(
                        sqlx::query("UPDATE app.counters SET n = $1 WHERE id = 1").bind(n + 1),
                    )
                    .await?;
                    tx.commit().await?;
                    Ok(n + 1)
                }
            },
            16,
        )
        .await
    };

    let a = tokio::spawn(bump(
        Arc::clone(&write),
        Arc::clone(&attempts),
        Arc::clone(&barrier),
    ));
    let b = tokio::spawn(bump(write, attempts.clone(), barrier));
    let ra = a.await.expect("join a").expect("task a");
    let rb = b.await.expect("join b").expect("task b");
    assert!(ra == 1 || ra == 2, "ra={ra}");
    assert!(rb == 1 || rb == 2, "rb={rb}");
    assert_ne!(ra, rb);
    let final_n: i32 = sqlx::query_scalar("SELECT n FROM app.counters WHERE id = 1")
        .fetch_one(db.app_pool())
        .await
        .expect("final");
    assert_eq!(final_n, 2);
    assert!(
        attempts.load(Ordering::SeqCst) >= 3,
        "expected a retry, attempts={}",
        attempts.load(Ordering::SeqCst)
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn refused_maps_42501() {
    let db = db_case!("refused");
    install_require_context_stub(db.migrate_pool()).await;
    let write = WritePool::new(db.app_pool().clone());
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    // Clear actor_id locally so the trigger sees a stale/empty context vs txid.
    tx.execute("SELECT pg_catalog.set_config('datum.actor_id', '', true)")
        .await
        .expect("clear actor");
    let err = tx
        .execute("INSERT INTO app.probe (id) VALUES (1)")
        .await
        .expect_err("must refuse");
    assert!(
        matches!(err, Error::Refused(ref s) if s.as_str() == "42501"),
        "got {err}"
    );
    let _ = tx.rollback().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn execute_and_fetch_helpers() {
    let db = db_case!("helpers");
    sqlx::query("CREATE TABLE app.helpers (id int PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = WritePool::new(db.app_pool().clone());
    let mut tx = Tx::begin(&write, &test_ctx()).await.expect("begin");
    tx.execute("INSERT INTO app.helpers (id, n) VALUES (1, 7)")
        .await
        .expect("insert");
    let (n,): (i32,) = tx
        .fetch_one(sqlx::query_as("SELECT n FROM app.helpers WHERE id = 1"))
        .await
        .expect("fetch");
    assert_eq!(n, 7);
    tx.commit().await.expect("commit");
    db.finish().await.expect("finish");
}

#[allow(dead_code)]
fn _row_trait_used(row: &sqlx::postgres::PgRow) -> i32 {
    row.get(0)
}
