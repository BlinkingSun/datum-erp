//! Reversible esign migration.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_esign::MIGRATOR;
use datum_test::db_case;
use sqlx::migrate::Migrator;
use sqlx::{query_as as sql_query_as, query_scalar as sql_query_scalar};

use common::migrate_esign;

fn reversible_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_esign");
    migrator
}

async fn catalog_esign(pool: &sqlx::PgPool) -> String {
    let cols: Vec<(String, String, String, bool)> = sql_query_as(
        r#"
        SELECT n.nspname::text || '.' || c.relname::text, a.attname::text, t.typname::text, a.attnotnull
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'esign'
          AND a.attnum > 0
          AND NOT a.attisdropped
          AND c.relkind IN ('r', 'p')
        ORDER BY 1, a.attnum
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cols");
    let cons: Vec<(String, String)> = sql_query_as(
        r#"
        SELECT n.nspname::text || '.' || c.relname::text, con.conname::text
        FROM pg_constraint con
        JOIN pg_class c ON c.oid = con.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'esign'
        ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cons");
    let owners: Vec<(String, String, bool)> = sql_query_as(
        r#"
        SELECT n.nspname::text || '.' || c.relname::text, r.rolname::text, r.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles r ON r.oid = c.relowner
        WHERE n.nspname = 'esign' AND c.relkind IN ('r', 'p')
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("owners");
    format!("{cols:?}\n{cons:?}\n{owners:?}")
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("es_down_up");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
            ("datum-identity", &datum_identity::MIGRATOR),
        ],
    )
    .await
    .expect("db+audit+identity");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");

    let migrator = reversible_migrator();
    migrator.run(db.migrate_pool()).await.expect("esign up");
    let before = catalog_esign(db.migrate_pool()).await;

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sql_query_scalar("SELECT to_regclass('esign.signature') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop esign.signature");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let after = catalog_esign(db.migrate_pool()).await;
    assert_eq!(
        before, after,
        "catalogue must be identical after down-then-up"
    );
    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn esign_tables_owned_by_datum_owner() {
    let db = db_case!("es_owner");
    migrate_esign(&db).await;
    let owners: Vec<(String, String, bool)> = sql_query_as(
        r#"
        SELECT n.nspname::text || '.' || c.relname::text, r.rolname::text, r.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles r ON r.oid = c.relowner
        WHERE (n.nspname = 'esign'
               OR (n.nspname = 'transient' AND c.relname = 'signing_session'))
          AND c.relkind IN ('r', 'p')
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
