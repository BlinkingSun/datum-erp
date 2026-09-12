//! Shared helpers for commit-mode numbering tests.

#![allow(dead_code)]

use std::time::Duration;

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{WriteContext, WritePool};
use sqlx::postgres::PgPoolOptions;

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

pub async fn migrate_and_install(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
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
