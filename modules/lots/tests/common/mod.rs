#![allow(dead_code)]

use datum_core::{Actor, ActorKind, Identifier, ItemId, LotId};
use datum_db::{Tx, WriteContext, WritePool};
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{PrincipalKind, SYSTEM_ID, create_principal};
use datum_module::{Kernel, Profile};
use datum_statemachine::DocRef;
use lots::states::DOC_TYPE;
use sqlx::{PgPool, query_scalar as sql_query_scalar};

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

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn item_rm_ti_bar() -> ItemId {
    ItemId::generate()
}

pub async fn migrate(db: &datum_test::TestDb) {
    datum_module::migrate_prefix(db.migrate_pool())
        .await
        .expect("migrate prefix");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    let crates: &[(&str, &sqlx::migrate::Migrator)] = &[
        ("datum-identity", &datum_identity::MIGRATOR),
        ("datum-numbering", &datum_numbering::MIGRATOR),
        ("datum-events", &datum_events::MIGRATOR),
        ("datum-mod-lots", &datum_mod_lots::MIGRATOR),
    ];
    for (name, migrator) in crates {
        datum_db::migrate::run(db.migrate_pool(), &[(*name, *migrator)])
            .await
            .unwrap_or_else(|e| panic!("migrate {name}: {e:#}"));
    }
    datum_mod_lots::register_schemas().expect("event schemas");
}

pub async fn boot_kernel(db: &datum_test::TestDb) -> Kernel {
    migrate_kernel(db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    lots::apply(&mut builder).expect("apply lots");
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

pub async fn migrate_kernel(db: &datum_test::TestDb) {
    datum_module::migrate_prefix(db.migrate_pool())
        .await
        .expect("migrate prefix");
    datum_module::migrate_suffix(db.migrate_pool())
        .await
        .unwrap_or_else(|e| panic!("migrate suffix: {e:#}"));
    datum_db::migrate::run(
        db.migrate_pool(),
        &[("datum-identity", &datum_identity::MIGRATOR)],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate identity: {e:#}"));
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[("datum-mod-lots", &datum_mod_lots::MIGRATOR)],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate lots: {e:#}"));
    datum_module::attach_kernel_audit(db.migrate_pool())
        .await
        .expect("attach_kernel_audit");
    datum_mod_lots::register_schemas().expect("event schemas");
}

pub fn pg_code_db(err: &datum_db::Error) -> String {
    match err {
        datum_db::Error::Sqlx(e) => e
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

pub fn lots_pg_code(err: &datum_mod_lots::Error) -> String {
    match err {
        datum_mod_lots::Error::Db(e) => pg_code_db(e),
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

pub fn qty_ea(n: i64) -> datum_core::AnyQuantity {
    datum_core::AnyQuantity {
        amount: rust_decimal::Decimal::from(n),
        unit: datum_core::UnitId(1),
        dimension: datum_core::DimensionKind::Count,
    }
}
