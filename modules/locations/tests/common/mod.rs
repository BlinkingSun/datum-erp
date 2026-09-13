#![allow(dead_code)]

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::WriteContext;
use datum_identity::SYSTEM_ID;
use datum_module::{Kernel, Profile};
use datum_statemachine::DocRef;
use sqlx::PgPool;

/// D-2b-13: one published order. `install_upto` is not on this tree yet
/// (glue lane); fall back to kernel prefix/suffix then `wave_2s1_migrators`
/// / `slice_migrators`, with `audit_attach` up before this crate's DDL.
pub async fn install_through(db: &datum_test::TestDb, crate_name: &str) {
    datum_module::migrate_prefix(db.migrate_pool())
        .await
        .expect("migrate prefix");
    datum_module::migrate_suffix(db.migrate_pool())
        .await
        .unwrap_or_else(|e| panic!("migrate suffix: {e:#}"));
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;

    let mut found = false;
    for (name, migrator) in datum_module::wave_2s1_migrators().expect("wave_2s1") {
        datum_db::migrate::run(db.migrate_pool(), &[(name, migrator)])
            .await
            .unwrap_or_else(|e| panic!("migrate {name}: {e:#}"));
        if name == crate_name {
            found = true;
            break;
        }
    }
    if !found {
        for (name, migrator) in datum_module::slice_migrators() {
            datum_db::migrate::run(db.migrate_pool(), &[(name, migrator)])
                .await
                .unwrap_or_else(|e| panic!("migrate {name}: {e:#}"));
            if name == crate_name {
                found = true;
                break;
            }
        }
    }
    assert!(found, "{crate_name} not in published wave_2s1/slice order");
    datum_module::attach_kernel_audit(db.migrate_pool())
        .await
        .expect("attach_kernel_audit");
}

pub async fn migrate_kernel(db: &datum_test::TestDb) {
    install_through(db, "datum-mod-locations").await;
}

pub async fn boot_kernel(db: &datum_test::TestDb) -> Kernel {
    migrate_kernel(db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    builder
        .apply_manifest(&datum_mod_locations::manifest().expect("manifest"))
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
