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
use sqlx::SqlSafeStr;
use sqlx::migrate::{Migration, MigrationType, Migrator};

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

#[allow(dead_code)]
fn _toy(version: i64, description: &'static str, sql: &'static str) -> Migrator {
    Migrator::with_migrations(vec![Migration::new(
        version,
        Cow::Borrowed(description),
        MigrationType::ReversibleUp,
        sql.into_sql_str(),
        false,
    )])
}
