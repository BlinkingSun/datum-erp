//! Reversible migrations and catalogue grants.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies
)]

mod common;

use std::borrow::Cow;

use datum_audit::MIGRATOR;
use datum_test::db_case;
use sqlx::migrate::{Migration, MigrationType, Migrator};
use sqlx::{AssertSqlSafe, SqlSafeStr};

use common::{bootstrap_pool, grant_create_on_database, migrate_and_install};

fn reversible_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("app._sqlx_migrations_audit");
    migrator
}

async fn catalog_audit(pool: &sqlx::PgPool) -> String {
    let cols: Vec<(String, String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, a.attname::text, t.typname::text, a.attnotnull
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'audit'
          AND a.attnum > 0
          AND NOT a.attisdropped
          AND c.relkind IN ('r', 'p')
        ORDER BY c.relname, a.attnum
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cols");
    let cons: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, con.contype::text, con.conname::text
        FROM pg_constraint con
        JOIN pg_class c ON c.oid = con.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'audit'
        ORDER BY 1, 2, 3
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cons");
    let funs: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT p.proname::text, r.rolname::text
        FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        JOIN pg_roles r ON r.oid = p.proowner
        WHERE n.nspname = 'audit'
        ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("funs");
    let evts: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT evtname::text, evtevent::text
        FROM pg_event_trigger
        WHERE evtname IN ('audit_attach', 'audit_protect', 'audit_protect_drop')
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("event triggers");
    let types: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT t.typname::text
        FROM pg_type t
        JOIN pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = 'audit' AND t.typtype = 'c'
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("types");
    format!("{cols:?}\n{cons:?}\n{funs:?}\n{evts:?}\n{types:?}")
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("audit_down_up");
    grant_create_on_database(db.database()).await;
    let migrator = reversible_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let boot = bootstrap_pool(db.database()).await;
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");
    let before = catalog_audit(db.migrate_pool()).await;
    datum_audit::uninstall_privileged(&boot)
        .await
        .expect("uninstall");
    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sqlx::query_scalar("SELECT to_regclass('audit.event') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop audit.event");
    migrator.run(db.migrate_pool()).await.expect("up again");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged again");
    let after = catalog_audit(db.migrate_pool()).await;
    assert_eq!(
        before, after,
        "catalogue must be identical after down-then-up, including event triggers"
    );
    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn grants_match_d3_section_1_3() {
    let db = db_case!("audit_grants");
    migrate_and_install(&db).await;
    let pool = db.migrate_pool();

    async fn priv_ok(pool: &sqlx::PgPool, role: &str, table: &str, verb: &str) -> bool {
        sqlx::query_scalar("SELECT has_table_privilege($1, $2, $3)")
            .bind(role)
            .bind(table)
            .bind(verb)
            .fetch_one(pool)
            .await
            .expect("has_table_privilege")
    }

    assert!(priv_ok(pool, "datum_app", "audit.event", "SELECT").await);
    assert!(!priv_ok(pool, "datum_app", "audit.event", "INSERT").await);
    assert!(!priv_ok(pool, "datum_app", "audit.event", "UPDATE").await);
    assert!(!priv_ok(pool, "datum_app", "audit.event", "DELETE").await);
    assert!(!priv_ok(pool, "datum_app", "audit.event", "TRUNCATE").await);

    assert!(priv_ok(pool, "datum_audit_row", "audit.event", "INSERT").await);
    assert!(!priv_ok(pool, "datum_audit_row", "audit.event", "UPDATE").await);
    assert!(!priv_ok(pool, "datum_audit_row", "audit.event", "DELETE").await);

    assert!(!priv_ok(pool, "datum_audit_event", "audit.event", "UPDATE").await);
    assert!(!priv_ok(pool, "datum_audit_event", "audit.event", "DELETE").await);

    let event_rels: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT c.oid::regclass::text
          FROM pg_class c
         WHERE c.oid = 'audit.event'::regclass
            OR c.oid IN (
                 SELECT inhrelid FROM pg_inherits
                  WHERE inhparent = 'audit.event'::regclass
               )
         ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("event relations");
    assert!(
        event_rels.iter().any(|r| r == "audit.event"),
        "parent missing: {event_rels:?}"
    );
    assert!(
        event_rels.iter().any(|r| r.contains("event_")),
        "partitions missing: {event_rels:?}"
    );

    const EVENT_WRITER_COLS: &[&str] = &[
        "event_id",
        "at",
        "stmt_at",
        "xid",
        "actor_id",
        "actor_kind",
        "actor_display",
        "acting_for_id",
        "session_id",
        "request_id",
        "source_kind",
        "source_device_id",
        "source_ip",
        "client_app",
        "action",
        "reason",
        "doc_type",
        "doc_id",
        "esign_id",
        "app_version",
        "config_version",
    ];
    const FORBIDDEN_EVENT_COLS: &[&str] = &["op", "old_row", "new_row", "table_name"];

    for rel in &event_rels {
        let table_level_insert: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
              SELECT 1
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl, '{}'::aclitem[])) a
                JOIN pg_roles r ON r.oid = a.grantee
               WHERE c.oid = $1::regclass
                 AND r.rolname = 'datum_audit_event'
                 AND a.privilege_type = 'INSERT'
            )
            "#,
        )
        .bind(rel)
        .fetch_one(pool)
        .await
        .expect("table-level INSERT");
        assert!(
            !table_level_insert,
            "datum_audit_event must not hold table-level INSERT on {rel}"
        );

        for col in EVENT_WRITER_COLS {
            let ok: bool = sqlx::query_scalar(
                "SELECT has_column_privilege('datum_audit_event', $1::regclass, $2, 'INSERT')",
            )
            .bind(rel)
            .bind(*col)
            .fetch_one(pool)
            .await
            .expect("col grant");
            assert!(ok, "datum_audit_event INSERT {col} on {rel}");
        }
        for col in FORBIDDEN_EVENT_COLS {
            let ok: bool = sqlx::query_scalar(
                "SELECT has_column_privilege('datum_audit_event', $1::regclass, $2, 'INSERT')",
            )
            .bind(rel)
            .bind(*col)
            .fetch_one(pool)
            .await
            .expect("forbidden col");
            assert!(!ok, "datum_audit_event must not INSERT {col} on {rel}");
        }
    }

    let row_owner: String = sqlx::query_scalar(
        r#"
        SELECT r.rolname
        FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        JOIN pg_roles r ON r.oid = p.proowner
        WHERE n.nspname = 'audit' AND p.proname = 'row_change'
        "#,
    )
    .fetch_one(pool)
    .await
    .expect("row_change owner");
    assert_eq!(row_owner, "datum_audit_row");
    let event_owner: String = sqlx::query_scalar(
        r#"
        SELECT r.rolname
        FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        JOIN pg_roles r ON r.oid = p.proowner
        WHERE n.nspname = 'audit' AND p.proname = 'log_event'
        "#,
    )
    .fetch_one(pool)
    .await
    .expect("log_event owner");
    assert_eq!(event_owner, "datum_audit_event");
    db.finish().await.expect("finish");
}

fn toy(version: i64, description: &'static str, sql: &'static str) -> Migrator {
    Migrator::with_migrations(vec![Migration::new(
        version,
        Cow::Borrowed(description),
        MigrationType::ReversibleUp,
        sql.into_sql_str(),
        false,
    )])
}

async fn has_zz_audit_row(pool: &sqlx::PgPool, schema: &str, table: &str) -> bool {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
          SELECT 1
            FROM pg_trigger t
            JOIN pg_class c ON c.oid = t.tgrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
           WHERE n.nspname = $1
             AND c.relname = $2
             AND t.tgname = 'zz_audit_row'
             AND NOT t.tgisinternal
        )
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("zz_audit_row")
}

/// D-2b-12(1): skip set is `datum.schema_class`, not the literal name
/// `'transient'`. A `<module>_transient` table must not acquire `zz_audit_row`.
#[tokio::test]
async fn event_trigger_skips_transient_class_by_catalog() {
    let db = db_case!("audit_skip_class");
    migrate_and_install(&db).await;
    let pool = db.migrate_pool();

    // Grant TRIGGER so a skip miss attaches (the inventory probe failure
    // mode) instead of 42501-ing CREATE TRIGGER.
    sqlx::raw_sql(AssertSqlSafe(
        r#"
        CREATE SCHEMA inventory_transient AUTHORIZATION datum_migrate;
        GRANT USAGE, CREATE ON SCHEMA inventory_transient TO datum_migrate, datum_owner;
        INSERT INTO datum.schema_class (nspname, class)
          VALUES ('inventory_transient', 'transient')
          ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;
        ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA inventory_transient
          GRANT TRIGGER ON TABLES TO datum_owner;
        CREATE TABLE inventory_transient.idempotency (
          key text PRIMARY KEY,
          created_at timestamptz NOT NULL DEFAULT now()
        );
        "#
        .to_owned(),
    ))
    .execute(pool)
    .await
    .expect("inventory_transient.idempotency");
    assert!(
        !has_zz_audit_row(pool, "inventory_transient", "idempotency").await,
        "class=transient must skip attach even when nspname is not the literal 'transient'"
    );

    // Catalog class, not the `_transient` suffix: class app still attaches.
    sqlx::raw_sql(AssertSqlSafe(
        r#"
        CREATE SCHEMA weird_transient AUTHORIZATION datum_migrate;
        GRANT USAGE, CREATE ON SCHEMA weird_transient TO datum_migrate, datum_owner;
        INSERT INTO datum.schema_class (nspname, class)
          VALUES ('weird_transient', 'app')
          ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;
        ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA weird_transient
          GRANT TRIGGER ON TABLES TO datum_owner;
        CREATE TABLE weird_transient.probe (id int PRIMARY KEY);
        "#
        .to_owned(),
    ))
    .execute(pool)
    .await
    .expect("weird_transient.probe");
    assert!(
        has_zz_audit_row(pool, "weird_transient", "probe").await,
        "class=app must attach even when nspname ends in _transient"
    );

    sqlx::raw_sql(AssertSqlSafe(
        "CREATE TABLE app.probe_audited (id int PRIMARY KEY)".to_owned(),
    ))
    .execute(pool)
    .await
    .expect("app.probe_audited");
    assert!(
        has_zz_audit_row(pool, "app", "probe_audited").await,
        "schema app (class app) must still attach"
    );

    sqlx::raw_sql(AssertSqlSafe(
        "CREATE TABLE datum.probe_skipped (id int PRIMARY KEY)".to_owned(),
    ))
    .execute(pool)
    .await
    .expect("datum.probe_skipped");
    assert!(
        !has_zz_audit_row(pool, "datum", "probe_skipped").await,
        "nspname datum must skip even though class is app"
    );

    db.finish().await.expect("finish");
}

/// D-2b-12(2): `migrate::run` sets the six GUCs so a later run records
/// history after `datum.schema_history` is attached.
#[tokio::test]
async fn runner_records_history_on_an_attached_schema_history() {
    let db = db_case!("hist_attached");
    migrate_and_install(&db).await;
    datum_audit::attach(db.migrate_pool(), "datum.schema_history")
        .await
        .expect("attach schema_history");
    assert!(
        has_zz_audit_row(db.migrate_pool(), "datum", "schema_history").await,
        "schema_history must carry zz_audit_row"
    );

    let later = toy(1, "later", "SELECT 1;");
    datum_db::migrate::run(db.migrate_pool(), &[("later-crate", &later)])
        .await
        .expect("run after attach");

    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM datum.schema_history WHERE crate = 'later-crate' AND version = 1",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("count");
    assert_eq!(
        n, 1,
        "runner must record history after schema_history is attached"
    );

    let audited: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM audit.event
         WHERE table_name = 'schema_history'
           AND op = 'INSERT'
           AND new_row->>'crate' = 'later-crate'
        "#,
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("audit rows");
    assert_eq!(audited, 1, "history insert must be judged by zz_audit_row");

    db.finish().await.expect("finish");
}
