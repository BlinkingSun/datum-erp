//! Customer-facing installation-qualification tests. Names are stable (PLAN §7).

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies
)]

mod common;

use std::fs;

use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;
use wicket_db::Tx;
use wicket_test::db_case;

use common::{
    as_owner, bootstrap_pool, ctx_named, migrate_and_install, pg_code, pg_code_db, tamper_action,
    test_ctx, write_pool,
};

#[tokio::test]
async fn author_forgets_audit_still_audited() {
    let db = db_case!("iq_forget");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.forget_me (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create table");

    let attached: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
          SELECT 1 FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = 'app' AND c.relname = 'forget_me'
            AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
        )
        "#,
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("trigger catalogue");
    assert!(
        attached,
        "zz_audit_row must be attached by audit_attach, not the test"
    );

    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let id = Uuid::now_v7();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(
        sqlx::query("INSERT INTO app.forget_me (id, n) VALUES ($1, $2)")
            .bind(id)
            .bind(1),
    )
    .await
    .expect("insert");
    tx.commit().await.expect("commit");

    let row = sqlx::query(
        r#"
        SELECT row_key, changed_columns, actor_id, at, app_version, table_name, op
          FROM audit.event
         WHERE table_name = 'forget_me'
        "#,
    )
    .fetch_all(db.app_pool())
    .await
    .expect("audit rows");
    assert_eq!(row.len(), 1, "exactly one audit row");
    let r = &row[0];
    let key: serde_json::Value = r.get("row_key");
    assert_eq!(key["id"], serde_json::json!(id.to_string()));
    let changed: Option<Vec<String>> = r.get("changed_columns");
    assert!(changed.is_none(), "INSERT has no changed_columns");
    let actor: Uuid = r.get("actor_id");
    assert_eq!(actor, ctx.actor.id.as_uuid());
    let at: chrono::DateTime<Utc> = r.get("at");
    assert!(at <= Utc::now());
    let app_version: String = r.get("app_version");
    assert!(
        !app_version.is_empty(),
        "app_version stamped from Tx::begin"
    );
    let op: String = r.get("op");
    assert_eq!(op, "INSERT");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn app_cannot_insert_audit_event() {
    let db = db_case!("iq_ins");
    migrate_and_install(&db).await;
    let err = sqlx::query("INSERT INTO audit.event (actor_id, actor_kind, actor_display, source_kind, action, at, stmt_at, xid) VALUES (gen_random_uuid(), 'user', 'x', 'ui', 'x', now(), clock_timestamp(), pg_current_xact_id())")
        .execute(db.app_pool())
        .await
        .expect_err("insert must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn app_cannot_update_audit_event() {
    let db = db_case!("iq_upd");
    migrate_and_install(&db).await;
    let err = sqlx::query("UPDATE audit.event SET action = 'forged'")
        .execute(db.app_pool())
        .await
        .expect_err("update must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn app_cannot_delete_audit_event() {
    let db = db_case!("iq_del");
    migrate_and_install(&db).await;
    let err = sqlx::query("DELETE FROM audit.event")
        .execute(db.app_pool())
        .await
        .expect_err("delete must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn log_event_cannot_forge_row_change() {
    let db = db_case!("iq_forge");
    migrate_and_install(&db).await;
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query(
        "SELECT audit.log_event('kernel.record', 'forged', '', '', '', '', '{}'::jsonb)",
    ))
    .await
    .expect("log_event");
    tx.commit().await.expect("commit");

    let row: (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT op, table_name, source_kind FROM audit.event WHERE action = 'forged' OR action = 'kernel.record' ORDER BY stmt_at DESC LIMIT 1",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("row");
    assert!(row.0.is_none(), "op must be null on app_event");
    assert!(row.1.is_none(), "table_name must be null on app_event");
    assert_eq!(row.2, "app_event");

    let can_op: bool = sqlx::query_scalar(
        "SELECT has_column_privilege('wicket_audit_event', 'audit.event', 'op', 'INSERT')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("col priv");
    assert!(!can_op, "wicket_audit_event must not INSERT op");

    let boot = bootstrap_pool(db.database()).await;
    {
        let mut conn = boot.acquire().await.expect("boot conn");
        sqlx::query("SET ROLE wicket_audit_event")
            .execute(&mut *conn)
            .await
            .expect("set role");
        let err = sqlx::query(
            r#"
            INSERT INTO audit.event (
              actor_id, actor_kind, actor_display, source_kind, action,
              at, stmt_at, xid, op, table_name, old_row, new_row
            ) VALUES (
              gen_random_uuid(), 'user', 'x', 'ui', 'forged-row',
              now(), clock_timestamp(), pg_current_xact_id(),
              'INSERT', 'forged', '{}'::jsonb, '{}'::jsonb
            )
            "#,
        )
        .execute(&mut *conn)
        .await
        .expect_err("forging INSERT of row-change columns must fail");
        assert_eq!(pg_code(&err), "42501", "err={err}");

        let err = sqlx::query(
            r#"
            INSERT INTO audit.event (
              actor_id, actor_kind, actor_display, source_kind, action,
              at, stmt_at, xid
            ) VALUES (
              gen_random_uuid(), 'user', 'x', 'ui', 'forged-shape',
              now(), clock_timestamp(), pg_current_xact_id()
            )
            "#,
        )
        .execute(&mut *conn)
        .await
        .expect_err("event_shape must reject app_event-shaped row with source_kind ui");
        assert_eq!(pg_code(&err), "23514", "err={err}");
        sqlx::query("RESET ROLE")
            .execute(&mut *conn)
            .await
            .expect("reset role");
    }
    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn tamper_breaks_verify_at_seq() {
    let db = db_case!("iq_tamper");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("insert");
    tx.commit().await.expect("commit");

    let head = wicket_audit::head(db.app_pool())
        .await
        .expect("head")
        .expect("sealed");
    tamper_action(db.database(), "probe").await;

    let bad = wicket_audit::verify(db.app_pool(), 1, head.seq)
        .await
        .expect("verify");
    assert_eq!(bad, Some(head.seq), "verify must fail at the tampered seq");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn reseal_diverges_from_anchor() {
    let db = db_case!("iq_reseal");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("insert");
    tx.commit().await.expect("commit");

    let head = wicket_audit::head(db.app_pool())
        .await
        .expect("head")
        .expect("sealed");
    wicket_audit::anchor::record(
        db.app_pool(),
        head.seq,
        &head.hash,
        "qa-logbook",
        Some("iq"),
    )
    .await
    .expect("anchor");

    tamper_action(db.database(), "probe").await;

    sqlx::query("SELECT audit.reseal_from($1)")
        .bind(head.seq)
        .execute(db.migrate_pool())
        .await
        .expect("reseal");

    let new_head = wicket_audit::head(db.app_pool())
        .await
        .expect("head2")
        .expect("sealed");
    assert_eq!(new_head.seq, head.seq);
    assert_ne!(
        new_head.hash, head.hash,
        "re-seal must change the head hash"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn aborted_tx_leaves_no_seal_and_same_head() {
    let db = db_case!("iq_abort");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = write_pool(&db).await;
    let before = wicket_audit::head(db.app_pool()).await.expect("head");

    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("insert");
    tx.rollback().await.expect("rollback");

    let after = wicket_audit::head(db.app_pool()).await.expect("head after");
    match (before, after) {
        (None, None) => {}
        (Some(a), Some(b)) => {
            assert_eq!(a.seq, b.seq);
            assert_eq!(a.hash, b.hash);
        }
        other => panic!("head changed across abort: {other:?}"),
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM audit.tx_seal")
        .fetch_one(db.app_pool())
        .await
        .expect("seals");
    assert_eq!(n, 0, "aborted transaction must leave no seal");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn export_bundle_is_self_describing() {
    let db = db_case!("iq_export");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.work_order (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let doc = Uuid::now_v7();
    let write = write_pool(&db).await;
    let mut ctx = ctx_named("work_order.create");
    ctx.doc_type = Some("work_order".into());
    ctx.doc_id = Some(doc.to_string());
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.work_order (id, n) VALUES ($1, 1)").bind(doc))
        .await
        .expect("insert");
    tx.commit().await.expect("commit");

    let selector = wicket_audit::export::Selector {
        doc_type: Some("work_order".into()),
        doc_id: Some(doc),
        from: None,
        to: None,
    };
    let a = std::env::temp_dir().join(format!("wicket-audit-export-a-{doc}"));
    let b = std::env::temp_dir().join(format!("wicket-audit-export-b-{doc}"));
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
    let ma = wicket_audit::export::bundle(db.app_pool(), &selector, &a)
        .await
        .expect("bundle a");
    let mb = wicket_audit::export::bundle(db.app_pool(), &selector, &b)
        .await
        .expect("bundle b");
    assert_ne!(ma.export_id, mb.export_id);
    assert_eq!(ma.row_count, 1);
    assert!(ma.files["report.pdf"]["absent"].as_bool().unwrap());

    for name in [
        "events.ndjson",
        "events.csv",
        "seals.ndjson",
        "dictionary.md",
    ] {
        let fa = fs::read(a.join(name)).expect(name);
        let fb = fs::read(b.join(name)).expect(name);
        assert_eq!(fa, fb, "{name} must be byte-stable across two runs");
    }

    let ndjson = fs::read_to_string(a.join("events.ndjson")).expect("ndjson");
    let mut lines = 0usize;
    for line in ndjson.lines() {
        assert!(line.starts_with('{') && line.ends_with('}'), "ndjson line");
        assert!(line.contains("event_id"), "std parse: event_id");
        assert!(line.contains("work_order"), "std parse: doc");
        lines += 1;
    }
    assert_eq!(lines, 1);

    let csv = fs::read_to_string(a.join("events.csv")).expect("csv");
    let mut csv_lines = csv.lines();
    let header = csv_lines.next().expect("header");
    assert!(header.starts_with("event_id,"), "csv header");
    let data = csv_lines.next().expect("row");
    assert!(data.contains(&doc.to_string()), "csv row carries doc id");
    assert!(csv_lines.next().is_none());

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn require_context_refuses_missing_actor() {
    let db = db_case!("iq_noactor");
    migrate_and_install(&db).await;
    let err = sqlx::query("SELECT audit.require_context()")
        .execute(db.app_pool())
        .await
        .expect_err("must refuse");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn require_context_refuses_foreign_txid() {
    let db = db_case!("iq_txid");
    migrate_and_install(&db).await;
    let mut tx = db.app_pool().begin().await.expect("begin");
    let actor = Uuid::now_v7().to_string();
    sqlx::query("SELECT pg_catalog.set_config('wicket.actor_id', $1, true)")
        .bind(&actor)
        .execute(&mut *tx)
        .await
        .expect("actor");
    sqlx::query("SELECT pg_catalog.set_config('wicket.txid', '1', true)")
        .execute(&mut *tx)
        .await
        .expect("txid");
    sqlx::query("SELECT pg_catalog.set_config('wicket.action', 'x', true)")
        .execute(&mut *tx)
        .await
        .expect("action");
    sqlx::query("SELECT pg_catalog.set_config('wicket.source_kind', 'ui', true)")
        .execute(&mut *tx)
        .await
        .expect("source");
    let err = sqlx::query("SELECT audit.require_context()")
        .execute(&mut *tx)
        .await
        .expect_err("must refuse");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn require_context_refuses_missing_action() {
    let db = db_case!("iq_noaction");
    migrate_and_install(&db).await;
    let mut tx = db.app_pool().begin().await.expect("begin");
    let actor = Uuid::now_v7().to_string();
    sqlx::query("SELECT pg_catalog.set_config('wicket.actor_id', $1, true)")
        .bind(&actor)
        .execute(&mut *tx)
        .await
        .expect("actor");
    sqlx::query(
        "SELECT pg_catalog.set_config('wicket.txid', pg_catalog.pg_current_xact_id()::text, true)",
    )
    .execute(&mut *tx)
    .await
    .expect("txid");
    sqlx::query("SELECT pg_catalog.set_config('wicket.source_kind', 'ui', true)")
        .execute(&mut *tx)
        .await
        .expect("source");
    let err = sqlx::query("SELECT audit.require_context()")
        .execute(&mut *tx)
        .await
        .expect_err("must refuse");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn scrub_redacts_listed_columns() {
    let db = db_case!("iq_scrub");
    migrate_and_install(&db).await;
    sqlx::query(
        "CREATE TABLE app.secrets (id uuid PRIMARY KEY, password_hash text NOT NULL, n int NOT NULL)",
    )
    .execute(db.migrate_pool())
    .await
    .expect("create");
    as_owner(
        db.migrate_pool(),
        "INSERT INTO audit.redact (relid, column_name, reason, decided_by)
         VALUES ('app.secrets'::regclass, 'password_hash', 'credential', 'iq')",
    )
    .await
    .expect("redact");

    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(
        sqlx::query("INSERT INTO app.secrets (id, password_hash, n) VALUES ($1, $2, 1)")
            .bind(Uuid::now_v7())
            .bind("s3cret-hash"),
    )
    .await
    .expect("insert");
    tx.commit().await.expect("commit");

    let new_row: serde_json::Value =
        sqlx::query_scalar("SELECT new_row FROM audit.event WHERE table_name = 'secrets'")
            .fetch_one(db.app_pool())
            .await
            .expect("new_row");
    assert_eq!(new_row["password_hash"], serde_json::json!("[redacted]"));
    assert_ne!(new_row["n"], serde_json::json!("[redacted]"));
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn transient_tables_carry_no_trigger() {
    let db = db_case!("iq_transient");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE transient.scratch (id int PRIMARY KEY)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let n: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM pg_trigger t
        JOIN pg_class c ON c.oid = t.tgrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'transient' AND c.relname = 'scratch'
          AND t.tgname LIKE 'zz_audit%' AND NOT t.tgisinternal
        "#,
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("triggers");
    assert_eq!(n, 0, "transient tables must carry no audit trigger");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn truncate_is_recorded_when_owner_truncates() {
    let db = db_case!("iq_trunc");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.trunc_me (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = wicket_db::WritePool::new(db.migrate_pool().clone());
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.trunc_me (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("insert");
    tx.execute(sqlx::query("TRUNCATE app.trunc_me"))
        .await
        .expect("truncate");
    tx.commit().await.expect("commit");

    let ops: Vec<String> = sqlx::query_scalar(
        "SELECT op FROM audit.event WHERE table_name = 'trunc_me' ORDER BY stmt_at",
    )
    .fetch_all(db.app_pool())
    .await
    .expect("ops");
    assert!(ops.iter().any(|o| o == "TRUNCATE"), "ops={ops:?}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn at_is_same_for_all_rows_of_one_transaction() {
    let db = db_case!("iq_at");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("i1");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 2)").bind(Uuid::now_v7()))
        .await
        .expect("i2");
    tx.commit().await.expect("commit");
    let times: Vec<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT at FROM audit.event WHERE table_name = 'probe'")
            .fetch_all(db.app_pool())
            .await
            .expect("at");
    assert_eq!(times.len(), 2);
    assert_eq!(times[0], times[1], "at is transaction_timestamp");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn stmt_at_orders_within_transaction() {
    let db = db_case!("iq_stmt");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("i1");
    tx.execute(sqlx::query("SELECT pg_sleep(0.002)"))
        .await
        .expect("sleep");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 2)").bind(Uuid::now_v7()))
        .await
        .expect("i2");
    tx.commit().await.expect("commit");
    let times: Vec<chrono::DateTime<Utc>> = sqlx::query_scalar(
        "SELECT stmt_at FROM audit.event WHERE table_name = 'probe' ORDER BY stmt_at",
    )
    .fetch_all(db.app_pool())
    .await
    .expect("stmt_at");
    assert_eq!(times.len(), 2);
    assert!(
        times[0] < times[1],
        "stmt_at must order within the transaction"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn seal_trigger_fires_at_commit_not_insert() {
    let db = db_case!("iq_seal");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("insert");
    let during: (i64,) = tx
        .fetch_one(sqlx::query_as(
            "SELECT count(*) FROM audit.tx_seal WHERE xid = pg_current_xact_id()",
        ))
        .await
        .expect("during");
    assert_eq!(during.0, 0, "seal must not exist before commit");
    match tx.commit().await {
        Ok(()) => {}
        Err(e) => panic!("commit failed: {e} sqlstate={}", pg_code_db(&e)),
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM audit.tx_seal")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(after, 1, "seal written at commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn migrate_login_cannot_drop_or_disable_audit_trigger() {
    let db = db_case!("iq_protect");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.protect_me (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");

    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(
        sqlx::query("INSERT INTO app.protect_me (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()),
    )
    .await
    .expect("insert");
    tx.commit().await.expect("commit");

    let migrate = db.migrate_pool();
    let who: String = sqlx::query_scalar("SELECT session_user")
        .fetch_one(migrate)
        .await
        .expect("session_user");
    assert_eq!(who, "wicket_migrate");
    let is_super: bool =
        sqlx::query_scalar("SELECT rolsuper FROM pg_roles WHERE rolname = session_user")
            .fetch_one(migrate)
            .await
            .expect("rolsuper");
    assert!(!is_super, "wicket_migrate must not be a superuser");

    let err = sqlx::query("DROP TRIGGER zz_audit_row ON app.protect_me")
        .execute(migrate)
        .await
        .expect_err("DROP TRIGGER must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");

    let err = sqlx::query("ALTER TABLE app.protect_me DISABLE TRIGGER zz_audit_row")
        .execute(migrate)
        .await
        .expect_err("DISABLE TRIGGER must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");

    let err = sqlx::query("DROP FUNCTION audit.row_change")
        .execute(migrate)
        .await
        .expect_err("DROP FUNCTION must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");

    let err = sqlx::query("UPDATE audit.event SET action = 'rewritten-by-migrate'")
        .execute(migrate)
        .await
        .expect_err("historical row must stay insert-only");
    assert_eq!(pg_code(&err), "42501", "err={err}");

    let enabled: String = sqlx::query_scalar(
        r#"
        SELECT t.tgenabled::text
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'app' AND c.relname = 'protect_me'
           AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
        "#,
    )
    .fetch_one(migrate)
    .await
    .expect("trigger still present");
    assert_eq!(enabled, "O", "zz_audit_row must remain enabled");

    let err = sqlx::query("INSERT INTO app.protect_me (id, n) VALUES (gen_random_uuid(), 1)")
        .execute(db.app_pool())
        .await
        .expect_err("unattributed insert must still be refused");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn module_trigger_can_be_disabled_by_migrate_login() {
    let db = db_case!("iq_module_trig");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.module_trig (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    sqlx::query(
        r#"
        CREATE FUNCTION app.module_noop() RETURNS trigger
        LANGUAGE plpgsql AS $fn$ BEGIN RETURN NEW; END $fn$
        "#,
    )
    .execute(db.migrate_pool())
    .await
    .expect("module function");
    sqlx::query(
        r#"
        CREATE TRIGGER module_own
          AFTER INSERT ON app.module_trig
          FOR EACH ROW EXECUTE FUNCTION app.module_noop()
        "#,
    )
    .execute(db.migrate_pool())
    .await
    .expect("module trigger");

    let migrate = db.migrate_pool();
    let who: String = sqlx::query_scalar("SELECT session_user")
        .fetch_one(migrate)
        .await
        .expect("session_user");
    assert_eq!(who, "wicket_migrate");
    let is_super: bool =
        sqlx::query_scalar("SELECT rolsuper FROM pg_roles WHERE rolname = session_user")
            .fetch_one(migrate)
            .await
            .expect("rolsuper");
    assert!(!is_super, "wicket_migrate must not be a superuser");

    sqlx::query("ALTER TABLE app.module_trig DISABLE TRIGGER module_own")
        .execute(migrate)
        .await
        .expect("migrate must be able to disable a non-audit trigger");
    let module_state: String = sqlx::query_scalar(
        r#"
        SELECT t.tgenabled::text
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'app' AND c.relname = 'module_trig'
           AND t.tgname = 'module_own' AND NOT t.tgisinternal
        "#,
    )
    .fetch_one(migrate)
    .await
    .expect("module trigger");
    assert_eq!(module_state, "D", "module_own must be disabled");

    sqlx::query("ALTER TABLE app.module_trig ENABLE TRIGGER module_own")
        .execute(migrate)
        .await
        .expect("migrate must be able to enable a non-audit trigger");
    let module_state: String = sqlx::query_scalar(
        r#"
        SELECT t.tgenabled::text
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'app' AND c.relname = 'module_trig'
           AND t.tgname = 'module_own' AND NOT t.tgisinternal
        "#,
    )
    .fetch_one(migrate)
    .await
    .expect("module trigger");
    assert_eq!(module_state, "O", "module_own must be enabled again");

    let err = sqlx::query("ALTER TABLE app.module_trig DISABLE TRIGGER zz_audit_row")
        .execute(migrate)
        .await
        .expect_err("DISABLE zz_audit_row must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");

    let err = sqlx::query("ALTER TABLE app.module_trig DISABLE TRIGGER ALL")
        .execute(migrate)
        .await
        .expect_err("DISABLE TRIGGER ALL on an audited table must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");

    let audit_state: String = sqlx::query_scalar(
        r#"
        SELECT t.tgenabled::text
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'app' AND c.relname = 'module_trig'
           AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
        "#,
    )
    .fetch_one(migrate)
    .await
    .expect("audit trigger");
    assert_eq!(audit_state, "O", "zz_audit_row must remain enabled");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn superuser_can_drop_audit_trigger_for_migrate_down() {
    let db = db_case!("iq_protect_su");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.protect_me (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");

    let boot = bootstrap_pool(db.database()).await;
    let is_super: bool =
        sqlx::query_scalar("SELECT rolsuper FROM pg_roles WHERE rolname = session_user")
            .fetch_one(&boot)
            .await
            .expect("rolsuper");
    assert!(is_super, "bootstrap login must be a superuser");

    sqlx::query("DROP TRIGGER zz_audit_row ON app.protect_me")
        .execute(&boot)
        .await
        .expect("superuser drop for migrate down");

    let gone: bool = sqlx::query_scalar(
        r#"
        SELECT NOT EXISTS (
          SELECT 1
            FROM pg_trigger t
            JOIN pg_class c ON c.oid = t.tgrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
           WHERE n.nspname = 'app' AND c.relname = 'protect_me'
             AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
        )
        "#,
    )
    .fetch_one(&boot)
    .await
    .expect("dropped");
    assert!(
        gone,
        "superuser must be able to drop zz_audit_row for migrate down"
    );
    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn verify_is_independent_of_session_timezone() {
    let db = db_case!("iq_tz");
    migrate_and_install(&db).await;
    sqlx::query("CREATE TABLE app.probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("create");
    let write = write_pool(&db).await;
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(sqlx::query("INSERT INTO app.probe (id, n) VALUES ($1, 1)").bind(Uuid::now_v7()))
        .await
        .expect("insert");
    tx.commit().await.expect("commit");

    let head = wicket_audit::head(db.app_pool())
        .await
        .expect("head")
        .expect("sealed");

    let mut conn = db.app_pool().acquire().await.expect("other connection");
    sqlx::query("SET TimeZone = 'America/New_York'")
        .execute(&mut *conn)
        .await
        .expect("session tz");
    let tz: String = sqlx::query_scalar("SHOW TimeZone")
        .fetch_one(&mut *conn)
        .await
        .expect("show tz");
    assert_eq!(tz, "America/New_York");
    let bad: Option<i64> = sqlx::query_scalar(
        "SELECT seq FROM audit.verify($1, $2) WHERE NOT ok ORDER BY seq LIMIT 1",
    )
    .bind(1_i64)
    .bind(head.seq)
    .fetch_optional(&mut *conn)
    .await
    .expect("verify");
    assert_eq!(
        bad, None,
        "verify must succeed after SET TimeZone = America/New_York"
    );
    drop(conn);
    db.finish().await.expect("finish");
}
