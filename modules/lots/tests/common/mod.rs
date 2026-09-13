#![allow(dead_code)]

use sqlx::{PgPool, query_scalar as sql_query_scalar};
use wicket_core::{Actor, ActorKind, Identifier, ItemId, LotId};
use wicket_db::{Tx, WriteContext, WritePool};
use wicket_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use wicket_identity::{PrincipalKind, SYSTEM_ID, create_principal};
use wicket_mod_lots::DOC_TYPE;
use wicket_module::{Kernel, Profile};
use wicket_statemachine::DocRef;

pub async fn actor_with_lots_perms(write: &WritePool) -> Actor {
    let slug = Identifier::generate().to_string();
    let short: String = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let mut tx = Tx::begin(write, &write_ctx("lots.boot"))
        .await
        .expect("begin actor");
    let p = create_principal(
        &mut tx,
        PrincipalKind::User,
        &format!("u{short}"),
        "M. Reyes",
    )
    .await
    .expect("principal");
    let roles = seed_bundles(
        &mut tx,
        &[RoleBundle {
            name: format!("r{short}"),
            permissions: vec![
                "lots.view".into(),
                "lots.edit".into(),
                "lots.release".into(),
            ],
        }],
    )
    .await
    .expect("role");
    assign_role(&mut tx, p.id, roles[0]).await.expect("assign");
    tx.commit().await.expect("commit actor");
    Actor {
        id: p.id.0,
        kind: ActorKind::User,
    }
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
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("lots-test".into());
    ctx
}

pub fn write_pool(db: &wicket_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn item_rm_ti_bar() -> ItemId {
    ItemId::generate()
}

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
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
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

pub async fn migrate(db: &wicket_test::TestDb) {
    install_through(db, "wicket-mod-lots").await;
    wicket_mod_lots::register_schemas().expect("event schemas");
}

pub async fn migrate_kernel(db: &wicket_test::TestDb) {
    migrate(db).await;
}

pub async fn boot_kernel(db: &wicket_test::TestDb) -> Kernel {
    migrate_kernel(db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    wicket_mod_lots::register(&mut builder, &Profile::plain_shop().unwrap())
        .expect("register lots");
    builder.build().await.expect("kernel build")
}

pub fn edge_ctx(kernel: &Kernel, actor: Actor, lot: LotId, edge: &str) -> WriteContext {
    let doc = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::from_uuid(lot.as_uuid()),
    };
    let mut ctx = kernel.transition_context(actor, &doc, edge);
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("lots-test".into());
    ctx
}

pub fn pg_code_db(err: &wicket_db::Error) -> String {
    match err {
        wicket_db::Error::Sqlx(e) => e
            .as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("{e}")),
        other => other.to_string(),
    }
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub fn lots_pg_code(err: &wicket_mod_lots::Error) -> String {
    match err {
        wicket_mod_lots::Error::Db(e) => pg_code_db(e),
        other => other.to_string(),
    }
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

pub async fn count_audit(pool: &PgPool, table: &str) -> i64 {
    sql_query_scalar("SELECT count(*) FROM audit.event WHERE table_name = $1")
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("audit count")
}

pub async fn count_audit_action(pool: &PgPool, action: &str, table: &str) -> i64 {
    sql_query_scalar("SELECT count(*) FROM audit.event WHERE action = $1 AND table_name = $2")
        .bind(action)
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("audit action count")
}

pub async fn sm_instance_count(pool: &PgPool, lot: LotId) -> i64 {
    sql_query_scalar(
        r#"SELECT count(*) FROM sm.instance i
           JOIN sm.machine m ON m.id = i.machine_id
          WHERE m.doc_type = 'lot' AND i.doc_id = $1"#,
    )
    .bind(lot.as_uuid())
    .fetch_one(pool)
    .await
    .expect("sm instance")
}

pub async fn history_stamps(pool: &PgPool, lot: LotId) -> (String, String) {
    sqlx::query_as(
        r#"SELECT application_version, configuration_version
             FROM lots.status_history
            WHERE lot_id = $1
            ORDER BY recorded_at DESC
            LIMIT 1"#,
    )
    .bind(lot.as_uuid())
    .fetch_one(pool)
    .await
    .expect("history stamps")
}

pub fn qty_ea(n: i64) -> wicket_core::AnyQuantity {
    wicket_core::AnyQuantity {
        amount: rust_decimal::Decimal::from(n),
        unit: wicket_core::UnitId(1),
        dimension: wicket_core::DimensionKind::Count,
    }
}
