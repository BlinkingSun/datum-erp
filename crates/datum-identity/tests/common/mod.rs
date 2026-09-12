#![allow(dead_code, unused_imports)]

use std::time::Duration;

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{WriteContext, WritePool};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};
use sqlx::{query as sql_query, query_as as sql_query_as, query_scalar as sql_query_scalar};
use uuid::Uuid;

pub fn system_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        action,
        "api",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("identity-test".into());
    ctx
}

pub fn user_ctx(id: Uuid, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(id),
            kind: ActorKind::User,
        },
        action,
        "ui",
    );
    ctx.actor_display = Some("user".into());
    ctx.reason = Some("identity-test".into());
    ctx
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub fn rewrite_database(url: &str, database: &str) -> String {
    let Some((prefix, rest)) = url.rsplit_once('/') else {
        panic!("url has no database path: {url}");
    };
    let qs = rest
        .split_once('?')
        .map(|(_, q)| format!("?{q}"))
        .unwrap_or_default();
    format!("{prefix}/{database}{qs}")
}

/// Apply db + audit + identity migrations. Does **not** call `seed_builtins`.
pub async fn migrate_identity_sql(db: &datum_test::TestDb) {
    grant_create_on_database(db.database()).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    let boot = bootstrap_pool(db.database()).await;
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[("datum-identity", &datum_identity::MIGRATOR)],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate identity: {e:#}"));
}

/// Apply identity SQL then call [`datum_identity::seed_builtins`] (idempotent).
pub async fn migrate_identity(db: &datum_test::TestDb) {
    migrate_identity_sql(db).await;
    let write = WritePool::new(db.app_pool().clone());
    let mut tx = datum_db::Tx::begin(&write, &system_ctx("identity.seed"))
        .await
        .expect("seed begin");
    datum_identity::seed_builtins(&mut tx)
        .await
        .expect("seed_builtins");
    tx.commit().await.expect("seed commit");
}

pub async fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub async fn bootstrap_pool(database: &str) -> PgPool {
    let url = std::env::var("DATUM_BOOTSTRAP_URL").expect("DATUM_BOOTSTRAP_URL");
    let rewritten = rewrite_database(&url, database);
    let mut opts: PgConnectOptions = rewritten.parse().expect("bootstrap url");
    if !url_has_userinfo(&url)
        && let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("LOGNAME"))
    {
        opts = opts.username(&user);
    }
    PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opts)
        .await
        .expect("bootstrap pool")
}

pub async fn grant_create_on_database(database: &str) {
    assert!(
        database
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    );
    let url = std::env::var("DATUM_BOOTSTRAP_URL").expect("DATUM_BOOTSTRAP_URL");
    let mut opts: PgConnectOptions = url.parse().expect("bootstrap url");
    if !url_has_userinfo(&url)
        && let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("LOGNAME"))
    {
        opts = opts.username(&user);
    }
    let mut conn = PgConnection::connect_with(&opts)
        .await
        .expect("bootstrap connect");
    let sql = format!("GRANT CREATE ON DATABASE {database} TO datum_migrate, datum_owner");
    sql_query(AssertSqlSafe(sql))
        .execute(&mut conn)
        .await
        .expect("GRANT CREATE");
}

fn url_has_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    rest.split(['/', '?']).next().unwrap_or("").contains('@')
}

pub async fn count_audit(pool: &PgPool, table: &str) -> i64 {
    sql_query_scalar("SELECT count(*) FROM audit.event WHERE table_name = $1")
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("audit count")
}

pub async fn table_owner(pool: &PgPool, schema: &str, table: &str) -> String {
    sql_query_scalar(
        r#"SELECT r.rolname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_roles r ON r.oid = c.relowner
           WHERE n.nspname = $1 AND c.relname = $2"#,
    )
    .bind(schema)
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("owner")
}

pub async fn has_zz_audit(pool: &PgPool, schema: &str, table: &str) -> bool {
    sql_query_scalar(
        r#"SELECT EXISTS (
             SELECT 1 FROM pg_trigger t
             JOIN pg_class c ON c.oid = t.tgrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relname = $2
               AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
           )"#,
    )
    .bind(schema)
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("trigger")
}
