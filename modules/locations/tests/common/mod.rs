#![allow(dead_code)]

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::WriteContext;
use datum_identity::SYSTEM_ID;
use datum_mod_locations::migrate as migrate_locations;
use datum_module::{attach_kernel_audit, migrate_prefix, migrate_suffix};
use sqlx::PgPool;

pub async fn migrate_kernel(db: &datum_test::TestDb) {
    migrate_prefix(db.migrate_pool()).await.expect("prefix");
    migrate_suffix(db.migrate_pool()).await.expect("suffix");
    migrate_locations(db.migrate_pool())
        .await
        .expect("locations migrate");
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");
    boot.close().await;
    attach_kernel_audit(db.migrate_pool())
        .await
        .expect("attach");
}

pub fn write_pool(db: &datum_test::TestDb) -> datum_db::WritePool {
    datum_db::WritePool::new(db.app_pool().clone())
}

pub fn write_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        action,
        "ui",
    );
    ctx.actor_display = Some("test".into());
    ctx.reason = Some("locations test".into());
    ctx
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub async fn has_audit(pool: &PgPool, schema: &str, table: &str) -> bool {
    sqlx::query_scalar(
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
    .expect("audit probe")
}
