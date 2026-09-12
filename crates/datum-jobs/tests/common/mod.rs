#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{WriteContext, WritePool};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, query_scalar};

pub fn user_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        "jobs.test",
        "api",
    );
    ctx.actor_display = Some("Test User".into());
    ctx
}

pub fn service_actor() -> Actor {
    Actor {
        id: Identifier::generate(),
        kind: ActorKind::ServicePrincipal,
    }
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
        &[
            ("datum-events", &datum_events::MIGRATOR),
            ("datum-jobs", &datum_jobs::MIGRATOR),
        ],
    )
    .await
    .expect("migrate events+jobs");
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub async fn write_pool_wide(db: &datum_test::TestDb, max_connections: u32) -> WritePool {
    let url = std::env::var("DATUM_DATABASE_URL").expect("DATUM_DATABASE_URL");
    let rewritten = rewrite_database(&url, db.database());
    WritePool::connect_with(
        &rewritten,
        PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5)),
    )
    .await
    .expect("WritePool::connect_with")
}

pub async fn table_schema(pool: &PgPool, table: &str) -> String {
    let nsp: String = query_scalar(
        r#"
        SELECT n.nspname::text
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE c.relname = $1
           AND n.nspname IN ('app', 'transient', 'audit', 'datum')
         LIMIT 1
        "#,
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| "missing".into());
    nsp
}

pub async fn table_class(pool: &PgPool, table: &str) -> String {
    let class: String = query_scalar(
        r#"
        SELECT sc.class
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN datum.schema_class sc ON sc.nspname = n.nspname
         WHERE c.relname = $1
           AND n.nspname IN ('app', 'transient', 'audit', 'datum')
         LIMIT 1
        "#,
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| "missing".into());
    class
}

pub async fn has_audit_trigger(pool: &PgPool, rel: &str) -> bool {
    query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM pg_trigger t
             WHERE t.tgrelid = $1::regclass
               AND t.tgname LIKE 'zz_audit%'
               AND NOT t.tgisinternal
        )
        "#,
    )
    .bind(rel)
    .fetch_one(pool)
    .await
    .expect("pg_trigger")
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}
