//! Reversible module-registry migration (PLAN §6 invariant 8).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use datum_module::MIGRATOR;
use datum_test::db_case;
use sqlx::migrate::Migrator;

fn reversible_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    // `module` is app-class; a version table there cannot be INSERTed without
    // Tx::begin. `transient` is skipped by audit_attach.
    migrator.dangerous_set_table_name("transient._sqlx_migrations_module");
    migrator
}

async fn catalog_module(pool: &sqlx::PgPool) -> String {
    let cols: Vec<(String, String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, a.attname::text, t.typname::text, a.attnotnull
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'module'
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
        WHERE n.nspname = 'module'
        ORDER BY 1, 2, 3
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cons");
    let owners: Vec<(String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, r.rolname::text, r.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles r ON r.oid = c.relowner
        WHERE n.nspname = 'module' AND c.relkind IN ('r', 'p')
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("owners");
    let class: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT nspname::text, class::text
        FROM datum.schema_class
        WHERE nspname = 'module'
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("schema_class");
    format!("{cols:?}\n{cons:?}\n{owners:?}\n{class:?}")
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("mod_down_up");
    datum_module::migrate_prefix(db.migrate_pool())
        .await
        .expect("db+audit");

    let migrator = reversible_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let before = catalog_module(db.migrate_pool()).await;

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sqlx::query_scalar("SELECT to_regclass('module.installed') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop module.installed");
    let class_gone: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM datum.schema_class WHERE nspname = 'module')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("class gone");
    assert!(class_gone, "0001 down must drop module from schema_class");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let after = catalog_module(db.migrate_pool()).await;
    assert_eq!(
        before, after,
        "catalogue must be identical after down-then-up"
    );
    db.finish().await.expect("finish");
}
