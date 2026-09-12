//! Reversible migration.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_statemachine::MIGRATOR;
use datum_test::db_case;
use sqlx::migrate::Migrator;
use sqlx::{query_as as sql_query_as, query_scalar as sql_query_scalar};

use common::{bootstrap_pool, migrate_and_install};

fn sm_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_statemachine");
    migrator
}

#[tokio::test]
async fn migration_is_reversible() {
    let db = db_case!("sm_rev");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("kernel");
    let boot = bootstrap_pool(db.database()).await;
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");

    let migrator = sm_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let owner_before: String = sql_query_scalar(
        r#"SELECT r.rolname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_roles r ON r.oid = c.relowner
          WHERE n.nspname = 'sm' AND c.relname = 'instance'"#,
    )
    .fetch_one(db.app_pool())
    .await
    .expect("owner");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sql_query_scalar("SELECT to_regclass('sm.instance') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("gone");
    assert!(gone, "sm.instance must be dropped");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let owner_after: String = sql_query_scalar(
        r#"SELECT r.rolname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_roles r ON r.oid = c.relowner
          WHERE n.nspname = 'sm' AND c.relname = 'instance'"#,
    )
    .fetch_one(db.app_pool())
    .await
    .expect("owner after");
    assert_eq!(owner_before, owner_after);
    assert_eq!(owner_after, "datum_owner");

    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn catalogue_accepts_sm_schema() {
    let db = db_case!("sm_ddl");
    migrate_and_install(&db).await;
    datum_db::ddl::check(db.migrate_pool())
        .await
        .expect("ddl check");
    let class: String =
        sql_query_scalar("SELECT class FROM datum.schema_class WHERE nspname = 'sm'")
            .fetch_one(db.app_pool())
            .await
            .expect("class");
    assert_eq!(class, "app");
    let cols: Vec<(String, String)> = sql_query_as(
        r#"SELECT c.relname::text, a.attname::text
           FROM pg_attribute a
           JOIN pg_class c ON c.oid = a.attrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = 'sm'
            AND a.attnum > 0 AND NOT a.attisdropped
          ORDER BY 1, 2"#,
    )
    .fetch_all(db.app_pool())
    .await
    .expect("cols");
    assert!(cols.iter().any(|(t, c)| t == "instance" && c == "state"));
    db.finish().await.expect("finish");
}
