//! Shared helpers for commit-mode numbering tests.

#![allow(dead_code)]

use std::time::Duration;

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{WriteContext, WritePool};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};

pub fn test_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        "numbering.allocate",
        "ui",
    );
    ctx.actor_display = Some("Ada".into());
    ctx
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

fn url_has_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    rest.split(['/', '?']).next().unwrap_or("").contains('@')
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
    sqlx::Executor::execute(&mut conn, sqlx::AssertSqlSafe(sql))
        .await
        .expect("GRANT CREATE");
}

pub async fn migrate_and_install(db: &datum_test::TestDb) {
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
        &[("datum-numbering", &datum_numbering::MIGRATOR)],
    )
    .await
    .expect("migrate numbering");
}

pub async fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub async fn write_pool_wide(db: &datum_test::TestDb, max_connections: u32) -> WritePool {
    let url = rewrite_database(
        &std::env::var("DATUM_DATABASE_URL").expect("DATUM_DATABASE_URL"),
        db.database(),
    );
    WritePool::connect_with(
        &url,
        PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(10)),
    )
    .await
    .expect("WritePool::connect_with")
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}
