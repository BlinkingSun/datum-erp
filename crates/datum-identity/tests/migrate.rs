//! Reversible identity migration, including the audit FK.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_identity::MIGRATOR;
use datum_test::db_case;
use sqlx::migrate::Migrator;
use sqlx::{query_as as sql_query_as, query_scalar as sql_query_scalar};

use common::{migrate_identity, migrate_identity_sql};

fn reversible_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    // schema `app` is auto-audited; a version table there cannot be INSERTed
    // without Tx::begin. `transient` is skipped by audit_attach.
    migrator.dangerous_set_table_name("transient._sqlx_migrations_identity");
    migrator
}

async fn catalog_identity(pool: &sqlx::PgPool) -> String {
    let cols: Vec<(String, String, String, bool)> = sql_query_as(
        r#"
        SELECT c.relname::text, a.attname::text, t.typname::text, a.attnotnull
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'identity'
          AND a.attnum > 0
          AND NOT a.attisdropped
          AND c.relkind IN ('r', 'p')
        ORDER BY c.relname, a.attnum
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cols");
    let cons: Vec<(String, String)> = sql_query_as(
        r#"
        SELECT c.relname::text, con.conname::text
        FROM pg_constraint con
        JOIN pg_class c ON c.oid = con.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'identity'
        ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cons");
    let fk: Vec<(String, bool)> = sql_query_as(
        r#"
        SELECT conname::text, convalidated
        FROM pg_constraint
        WHERE conname = 'event_actor_fk'
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("fk");
    let owners: Vec<(String, String, bool)> = sql_query_as(
        r#"
        SELECT c.relname::text, r.rolname::text, r.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles r ON r.oid = c.relowner
        WHERE n.nspname = 'identity' AND c.relkind IN ('r', 'p')
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("owners");
    format!("{cols:?}\n{cons:?}\n{fk:?}\n{owners:?}")
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("id_down_up");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("db+audit");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");

    let migrator = reversible_migrator();
    migrator.run(db.migrate_pool()).await.expect("identity up");
    let before = catalog_identity(db.migrate_pool()).await;

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sql_query_scalar("SELECT to_regclass('identity.principal') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop identity.principal");
    let fk_gone: bool = sql_query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'event_actor_fk')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("fk gone");
    assert!(fk_gone, "0001 down must drop event_actor_fk");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let after = catalog_identity(db.migrate_pool()).await;
    assert_eq!(
        before, after,
        "catalogue must be identical after down-then-up, including the audit FK"
    );
    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn identity_tables_owned_by_datum_owner() {
    let db = db_case!("id_owner");
    migrate_identity(&db).await;
    let owners: Vec<(String, String, bool)> = sql_query_as(
        r#"
        SELECT c.relname::text, r.rolname::text, r.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles r ON r.oid = c.relowner
        WHERE n.nspname = 'identity' AND c.relkind IN ('r', 'p')
        "#,
    )
    .fetch_all(db.migrate_pool())
    .await
    .expect("owners");
    assert!(!owners.is_empty());
    for (table, owner, can_login) in owners {
        assert_eq!(owner, "datum_owner", "{table}");
        assert!(!can_login, "{table} owner must be NOLOGIN");
    }
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn builtins_exist_after_migrate() {
    let db = db_case!("id_builtins");
    migrate_identity_sql(&db).await;
    let count: i64 = sql_query_scalar("SELECT count(*) FROM identity.principal")
        .fetch_one(db.migrate_pool())
        .await
        .expect("count");
    assert_eq!(
        count, 2,
        "identity.principal must contain exactly the two built-ins after identity-up with no Rust seed"
    );
    let rows: Vec<(String, String, String)> = sql_query_as(
        r#"SELECT id::text, kind, username
             FROM identity.principal
            ORDER BY username"#,
    )
    .fetch_all(db.migrate_pool())
    .await
    .expect("rows");
    assert_eq!(rows[0].0, datum_identity::MIGRATION_ID.to_string());
    assert_eq!(rows[0].1, "migration");
    assert_eq!(rows[0].2, "migration");
    assert_eq!(rows[1].0, datum_identity::SYSTEM_ID.to_string());
    assert_eq!(rows[1].1, "service");
    assert_eq!(rows[1].2, "system");
    db.finish().await.expect("finish");
}
