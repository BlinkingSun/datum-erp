#![allow(dead_code)]

use sqlx::PgPool;
use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::WriteContext;
use wicket_identity::SYSTEM_ID;
use wicket_module::{Kernel, Profile};
use wicket_statemachine::DocRef;

/// D-2b-13: one published order. `install_upto` is not on this tree yet
/// (glue lane); fall back to kernel prefix/suffix then `wave_2s1_migrators`
/// / `slice_migrators`, with `audit_attach` up before this crate's DDL.
pub async fn install_through(db: &wicket_test::TestDb, crate_name: &str) {
    wicket_module::migrate_prefix(db.migrate_pool())
        .await
        .expect("migrate prefix");
    wicket_module::migrate_suffix(db.migrate_pool())
        .await
        .unwrap_or_else(|e| panic!("migrate suffix: {e:#}"));
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    wicket_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;

    let mut found = false;
    for (name, migrator) in wicket_module::wave_2s1_migrators().expect("wave_2s1") {
        wicket_db::migrate::run(db.migrate_pool(), &[(name, migrator)])
            .await
            .unwrap_or_else(|e| panic!("migrate {name}: {e:#}"));
        if name == crate_name {
            found = true;
            break;
        }
    }
    if !found {
        for (name, migrator) in wicket_module::slice_migrators() {
            wicket_db::migrate::run(db.migrate_pool(), &[(name, migrator)])
                .await
                .unwrap_or_else(|e| panic!("migrate {name}: {e:#}"));
            if name == crate_name {
                found = true;
                break;
            }
        }
    }
    assert!(found, "{crate_name} not in published wave_2s1/slice order");
    wicket_module::attach_kernel_audit(db.migrate_pool())
        .await
        .expect("attach_kernel_audit");
}

pub async fn migrate_kernel(db: &wicket_test::TestDb) {
    install_through(db, "wicket-mod-locations").await;
}

pub async fn boot_kernel(db: &wicket_test::TestDb) -> Kernel {
    migrate_kernel(db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    builder
        .apply_manifest(&wicket_mod_locations::manifest().expect("manifest"))
        .expect("register locations");
    builder.build().await.expect("kernel build")
}

pub fn edge_ctx(kernel: &Kernel, actor: Actor, edge: &str) -> WriteContext {
    let doc = DocRef {
        doc_type: "location".into(),
        doc_id: Identifier::generate(),
    };
    let mut ctx = kernel.transition_context(actor, &doc, edge);
    ctx.actor_display = Some("test".into());
    ctx.reason = Some("locations test".into());
    ctx
}

pub fn write_pool(db: &wicket_test::TestDb) -> wicket_db::WritePool {
    wicket_db::WritePool::new(db.app_pool().clone())
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
