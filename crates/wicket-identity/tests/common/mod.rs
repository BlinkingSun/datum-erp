#![allow(dead_code, unused_imports)]

use sqlx::PgPool;
use sqlx::{query as sql_query, query_as as sql_query_as, query_scalar as sql_query_scalar};
use uuid::Uuid;
use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::{WriteContext, WritePool};

pub fn system_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(wicket_identity::SYSTEM_ID),
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
pub async fn migrate_identity_sql(db: &wicket_test::TestDb) {
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
        &[("wicket-identity", &wicket_identity::MIGRATOR)],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate identity: {e:#}"));
}

/// Apply identity SQL then call [`wicket_identity::seed_builtins`] (idempotent).
pub async fn migrate_identity(db: &wicket_test::TestDb) {
    migrate_identity_sql(db).await;
    let write = WritePool::new(db.app_pool().clone());
    let mut tx = wicket_db::Tx::begin(&write, &system_ctx("identity.seed"))
        .await
        .expect("seed begin");
    wicket_identity::seed_builtins(&mut tx)
        .await
        .expect("seed_builtins");
    tx.commit().await.expect("seed commit");
}

pub async fn write_pool(db: &wicket_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
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
