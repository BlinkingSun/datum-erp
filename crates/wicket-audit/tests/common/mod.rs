#![allow(dead_code)]

use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};
use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::{WriteContext, WritePool};

pub fn test_ctx() -> WriteContext {
    ctx_named("test.insert")
}

pub fn ctx_named(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        action,
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

pub async fn migrate_and_install(db: &wicket_test::TestDb) {
    grant_create_on_database(db.database()).await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate");
    let boot = bootstrap_pool(db.database()).await;
    wicket_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
}

pub async fn write_pool(db: &wicket_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub async fn bootstrap_pool(database: &str) -> PgPool {
    let url = std::env::var("WICKET_BOOTSTRAP_URL").expect("WICKET_BOOTSTRAP_URL");
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
    let url = std::env::var("WICKET_BOOTSTRAP_URL").expect("WICKET_BOOTSTRAP_URL");
    let mut opts: PgConnectOptions = url.parse().expect("bootstrap url");
    if !url_has_userinfo(&url)
        && let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("LOGNAME"))
    {
        opts = opts.username(&user);
    }
    let mut conn = PgConnection::connect_with(&opts)
        .await
        .expect("bootstrap connect");
    let sql = format!("GRANT CREATE ON DATABASE {database} TO wicket_migrate, wicket_owner");
    sqlx::raw_sql(AssertSqlSafe(sql))
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

pub async fn as_owner(pool: &PgPool, sql: &str) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("SET ROLE wicket_owner")
        .execute(&mut *conn)
        .await?;
    let result = sqlx::raw_sql(AssertSqlSafe(sql.to_owned()))
        .execute(&mut *conn)
        .await;
    let _ = sqlx::query("RESET ROLE").execute(&mut *conn).await;
    result.map(|_| ())
}

/// Superuser edit of `audit.event` (replication role skips the insert-only trigger).
pub async fn tamper_action(database: &str, table: &str) {
    let boot = bootstrap_pool(database).await;
    let mut conn = boot.acquire().await.expect("bootstrap conn");
    sqlx::query("SET session_replication_role = replica")
        .execute(&mut *conn)
        .await
        .expect("replica");
    sqlx::query("UPDATE audit.event SET action = 'tampered' WHERE table_name = $1")
        .bind(table)
        .execute(&mut *conn)
        .await
        .expect("update");
    sqlx::query("SET session_replication_role = DEFAULT")
        .execute(&mut *conn)
        .await
        .expect("restore");
    drop(conn);
    boot.close().await;
}
