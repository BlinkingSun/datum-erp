#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, query_as, query_scalar};
use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::{WriteContext, WritePool};

pub fn user_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        "events.publish",
        "api",
    );
    ctx.actor_display = Some("Ada".into());
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

/// Migrate db + audit, install the event trigger, then migrate events so `app.*`
/// tables created here are attached by `audit_attach`.
pub async fn migrate_and_install(db: &wicket_test::TestDb) {
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    wicket_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[("wicket-events", &wicket_events::MIGRATOR)],
    )
    .await
    .expect("migrate events");
}

pub fn write_pool(db: &wicket_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub async fn write_pool_wide(db: &wicket_test::TestDb, max_connections: u32) -> WritePool {
    let url = std::env::var("WICKET_DATABASE_URL").expect("WICKET_DATABASE_URL");
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

pub async fn ping_event() -> wicket_events::Event {
    wicket_events::Event::builder()
        .name("test.ping")
        .version(1)
        .payload(serde_json::json!({}))
        .build()
        .expect("test.ping is seeded")
}

pub async fn count_events(pool: &PgPool) -> i64 {
    query_scalar("SELECT count(*) FROM app.event")
        .fetch_one(pool)
        .await
        .expect("count app.event")
}

pub async fn delivery_attempts(
    pool: &PgPool,
    event_id: Identifier,
    subscriber: &str,
) -> Option<(i32, Option<DateTime>)> {
    let row: Option<(i32, Option<chrono::DateTime<chrono::Utc>>)> = query_as(
        "SELECT attempts, delivered_at FROM transient.delivery WHERE event_id = $1 AND subscriber = $2",
    )
    .bind(event_id.as_uuid())
    .bind(subscriber)
    .fetch_optional(pool)
    .await
    .expect("delivery");
    row
}

pub type DateTime = chrono::DateTime<chrono::Utc>;

pub async fn table_schema(pool: &PgPool, table: &str) -> String {
    let nsp: String = query_scalar(
        r#"
        SELECT n.nspname::text
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE c.relname = $1
           AND n.nspname IN ('app', 'transient', 'audit', 'wicket')
         LIMIT 1
        "#,
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| "missing".into());
    nsp
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

pub async fn table_owner(pool: &PgPool, rel: &str) -> String {
    query_scalar(
        "SELECT pg_catalog.pg_get_userbyid(c.relowner) FROM pg_class c WHERE c.oid = $1::regclass",
    )
    .bind(rel)
    .fetch_one(pool)
    .await
    .expect("owner")
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub fn pg_code_db(err: &wicket_db::Error) -> String {
    match err {
        wicket_db::Error::Refused(s) => s.as_str().to_string(),
        wicket_db::Error::Sqlx(e) => pg_code(e),
        other => other.to_string(),
    }
}
