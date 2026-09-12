//! Migration runner and reversible 0001.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies
)]

mod common;

use std::borrow::Cow;
use std::time::Duration;

use datum_db::migrate::ADVISORY_LOCK_KEY;
use datum_db::{MIGRATOR, ddl, migrate};
use datum_test::db_case;
use sqlx::migrate::{Migration, MigrationType, Migrator};
use sqlx::{AssertSqlSafe, SqlSafeStr};

fn toy(version: i64, description: &'static str, sql: &'static str) -> Migrator {
    Migrator::with_migrations(vec![Migration::new(
        version,
        Cow::Borrowed(description),
        MigrationType::ReversibleUp,
        sql.into_sql_str(),
        false,
    )])
}

/// sqlx's default `_sqlx_migrations` lives in `public`, which `datum_migrate`
/// cannot CREATE INTO (PG 15+). Track 0001 in `app` for undo/redo.
fn reversible_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("app._sqlx_migrations");
    migrator
}

async fn catalog_datum(pool: &sqlx::PgPool) -> String {
    let cols: Vec<(String, String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, a.attname::text, t.typname::text, a.attnotnull
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'datum'
          AND a.attnum > 0
          AND NOT a.attisdropped
          AND c.relkind IN ('r', 'p')
        ORDER BY c.relname, a.attnum
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("catalog cols");
    let cons: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, con.contype::text, con.conname::text
        FROM pg_constraint con
        JOIN pg_class c ON c.oid = con.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'datum'
        ORDER BY 1, 2, 3
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("catalog cons");
    let owners: Vec<(String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, o.rolname::text, o.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles o ON o.oid = c.relowner
        WHERE n.nspname = 'datum'
          AND c.relkind IN ('r', 'p')
        ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("catalog owners");
    format!("{cols:?}\n{cons:?}\n{owners:?}")
}

/// No `app` / `transient` / `datum` table is owned by a login role (addendum 2).
async fn assert_no_login_owned_tables(pool: &sqlx::PgPool) {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT n.nspname::text, c.relname::text, o.rolname::text
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles o ON o.oid = c.relowner
        WHERE n.nspname IN ('app', 'transient', 'datum')
          AND c.relkind IN ('r', 'p')
          AND o.rolcanlogin
          AND c.relname IS DISTINCT FROM '_sqlx_migrations'
        ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("table owners");
    assert!(
        rows.is_empty(),
        "tables owned by a login role (want datum_owner): {rows:?}"
    );
}

#[tokio::test]
async fn migrate_runs_in_order_under_lock() {
    let db = db_case!("migrate_order");
    common::grant_create_on_database(db.database()).await;
    sqlx::query("CREATE TABLE transient.migrate_log (id bigserial PRIMARY KEY, crate text NOT NULL, at timestamptz NOT NULL DEFAULT clock_timestamp())")
        .execute(db.migrate_pool())
        .await
        .expect("log table");

    let toy_a = toy(
        1,
        "a",
        "INSERT INTO transient.migrate_log (crate) VALUES ('toy_a'); SELECT pg_sleep(0.4);",
    );
    let toy_b = toy(
        1,
        "b",
        "INSERT INTO transient.migrate_log (crate) VALUES ('toy_b');",
    );

    let pool = db.migrate_pool().clone();
    let first = tokio::spawn(async move {
        migrate::run(&pool, &[("datum-db", &MIGRATOR), ("toy_a", &toy_a)]).await
    });

    let mut held = false;
    let mut probe = db.app_pool().acquire().await.expect("probe conn");
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let acquired: bool = sqlx::query_scalar("SELECT pg_catalog.pg_try_advisory_lock($1)")
            .bind(ADVISORY_LOCK_KEY)
            .fetch_one(&mut *probe)
            .await
            .expect("try lock");
        if acquired {
            sqlx::query("SELECT pg_catalog.pg_advisory_unlock($1)")
                .bind(ADVISORY_LOCK_KEY)
                .execute(&mut *probe)
                .await
                .expect("unlock probe");
        } else {
            held = true;
            break;
        }
    }
    drop(probe);
    assert!(held, "advisory lock must be held during run");

    first.await.expect("join").expect("first run");

    migrate::run(db.migrate_pool(), &[("toy_b", &toy_b)])
        .await
        .expect("second crate");

    let order: Vec<String> = sqlx::query_scalar(
        "SELECT crate FROM datum.schema_history ORDER BY applied_at, crate, version",
    )
    .fetch_all(db.migrate_pool())
    .await
    .expect("history");
    assert!(order.iter().any(|c| c == "datum-db"), "history={order:?}");
    let toys: Vec<String> =
        sqlx::query_scalar("SELECT crate FROM transient.migrate_log ORDER BY id")
            .fetch_all(db.migrate_pool())
            .await
            .expect("log");
    assert_eq!(toys, vec!["toy_a".to_string(), "toy_b".to_string()]);

    let history_toys: Vec<(String, i64)> = sqlx::query_as(
        "SELECT crate, version FROM datum.schema_history WHERE crate LIKE 'toy%' ORDER BY applied_at, crate",
    )
    .fetch_all(db.migrate_pool())
    .await
    .expect("toy history");
    assert_eq!(
        history_toys,
        vec![("toy_a".into(), 1i64), ("toy_b".into(), 1)]
    );

    ddl::check(db.migrate_pool()).await.expect("clean ddl");
    assert_no_login_owned_tables(db.migrate_pool()).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let db = db_case!("migrate_idemp");
    common::grant_create_on_database(db.database()).await;
    migrate::run(db.migrate_pool(), &[("datum-db", &MIGRATOR)])
        .await
        .expect("first");
    migrate::run(db.migrate_pool(), &[("datum-db", &MIGRATOR)])
        .await
        .expect("second");
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM datum.schema_history WHERE crate = 'datum-db'")
            .fetch_one(db.migrate_pool())
            .await
            .expect("count");
    let versions: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT version) FROM datum.schema_history WHERE crate = 'datum-db'",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("distinct");
    assert_eq!(n, versions);
    assert!(n >= 1);
    assert_no_login_owned_tables(db.migrate_pool()).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("migrate_down_up");
    common::grant_create_on_database(db.database()).await;
    let migrator = reversible_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let before = catalog_datum(db.migrate_pool()).await;
    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sqlx::query_scalar("SELECT to_regclass('datum.schema_history') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop schema_history");
    migrator.run(db.migrate_pool()).await.expect("up again");
    let after = catalog_datum(db.migrate_pool()).await;
    assert_eq!(
        before, after,
        "catalogue must be identical after down-then-up"
    );
    assert_no_login_owned_tables(db.migrate_pool()).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn tables_not_owned_by_login_role() {
    let db = db_case!("migrate_owner");
    common::grant_create_on_database(db.database()).await;
    migrate::run(db.migrate_pool(), &[("datum-db", &MIGRATOR)])
        .await
        .expect("run");
    let owners: Vec<(String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, o.rolname::text, o.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles o ON o.oid = c.relowner
        WHERE n.nspname = 'datum'
          AND c.relkind IN ('r', 'p')
        ORDER BY 1
        "#,
    )
    .fetch_all(db.migrate_pool())
    .await
    .expect("owners");
    assert!(
        !owners.is_empty(),
        "0001 must create tables in schema datum"
    );
    for (table, role, can_login) in &owners {
        assert_eq!(role, "datum_owner", "{table} owner={role}");
        assert!(!*can_login, "{table} owner {role} must be NOLOGIN");
    }
    assert_no_login_owned_tables(db.migrate_pool()).await;
    db.finish().await.expect("finish");
}

#[allow(dead_code)]
fn _assert_sql_safe(sql: String) -> sqlx::SqlStr {
    AssertSqlSafe(sql).into_sql_str()
}
