//! Shared helpers for commit-mode items tests.

#![allow(dead_code, unused_imports)]

use datum_core::{
    Actor, ActorKind, AnyQuantity, Boundary, CostElement, DimensionKind, Identifier, ItemId,
    LocationId, Money, PostingIntent, PostingSink, QuantityPosting, UnitId, ValueAccount,
    ValuePosting,
};
use datum_db::{Tx, WriteContext, WritePool};
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{PrincipalKind, create_principal};
use datum_ledger::{CostMethod, GroupBuilder, upsert_location};
use datum_module::{
    Kernel, KernelBuilder, Profile, attach_kernel_audit, migrate_prefix, migrate_suffix,
};
use datum_statemachine::DocRef;
use items::{DOC_TYPE, Kind, NewItem};
use rust_decimal::Decimal;
use sqlx::{PgPool, query_scalar as sql_query_scalar};

pub const EA: UnitId = UnitId(1);
pub const MM: UnitId = UnitId(2);

pub async fn migrate_all(db: &datum_test::TestDb) {
    migrate_prefix(db.migrate_pool())
        .await
        .expect("migrate prefix");
    migrate_suffix(db.migrate_pool())
        .await
        .unwrap_or_else(|e| panic!("migrate suffix: {e:#}"));
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    // Items SQL must run before attach_kernel_audit so datum.schema_history
    // (not yet triggered) can record the crate without an actor GUC.
    datum_db::migrate::run(db.migrate_pool(), &[("items", &items::MIGRATOR)])
        .await
        .unwrap_or_else(|e| panic!("migrate items: {e:#}"));
    attach_kernel_audit(db.migrate_pool())
        .await
        .expect("attach_kernel_audit");
}

pub async fn boot_kernel(db: &datum_test::TestDb) -> Kernel {
    migrate_all(db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    items::register(&mut builder).expect("register items");
    builder.build().await.expect("kernel build")
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn create_ctx(actor: Actor) -> WriteContext {
    let mut ctx = WriteContext::new(actor, "items.create", "ui");
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("items-test".into());
    ctx
}

pub fn update_ctx(actor: Actor) -> WriteContext {
    let mut ctx = WriteContext::new(actor, "items.update", "ui");
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("items-test".into());
    ctx
}

pub fn edge_ctx(actor: Actor, id: ItemId, edge: &str) -> WriteContext {
    let doc = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::from_uuid(id.as_uuid()),
    };
    let mut ctx = Kernel::transition_context(actor, &doc, edge);
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("items-test".into());
    ctx
}

pub fn screw() -> NewItem {
    NewItem {
        number: "MDS-450-M4x12".into(),
        revision: "C".into(),
        description: "Cortical bone screw, Ti-6Al-4V ELI, M4 x 12".into(),
        kind: Kind::Make,
        stock_uom: EA,
        stock_scale: 0,
        residual_tolerance: Decimal::ZERO,
        cost_method: CostMethod::Fifo,
        standard: None,
    }
}

pub fn bar() -> NewItem {
    NewItem {
        number: "RM-TI-BAR-12".into(),
        revision: "A".into(),
        description: "Titanium bar, stocked in mm of length".into(),
        kind: Kind::Buy,
        stock_uom: MM,
        stock_scale: 2,
        residual_tolerance: Decimal::new(1, 2),
        cost_method: CostMethod::Fifo,
        standard: None,
    }
}

pub async fn actor_with_item_perms(write: &WritePool) -> Actor {
    let slug = Identifier::generate().to_string();
    let short: String = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let mut tx = Tx::begin(write, &boot_ctx()).await.expect("begin actor");
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
                "items.view".into(),
                "items.edit".into(),
                "items.release".into(),
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

pub fn boot_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "items.boot",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("items-test".into());
    ctx
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
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

pub async fn count_audit_action(pool: &PgPool, action: &str, table: &str) -> i64 {
    sql_query_scalar("SELECT count(*) FROM audit.event WHERE action = $1 AND table_name = $2")
        .bind(action)
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("audit count")
}

fn usd(n: i64) -> Money {
    Money::new(Decimal::from(n), datum_core::CurrencyId(840)).expect("usd")
}

fn qty_ea(n: i64) -> AnyQuantity {
    AnyQuantity {
        amount: Decimal::from(n),
        unit: EA,
        dimension: DimensionKind::Count,
    }
}

/// One receipt through the ledger (kernel-e2e / D2 case-a shape).
pub async fn post_one_receipt(tx: &mut Tx<'_>, item: ItemId) {
    let stock = LocationId::generate();
    let supplier = LocationId::generate();
    upsert_location(tx, stock, None).await.expect("stock loc");
    upsert_location(tx, supplier, Some(Boundary::Supplier))
        .await
        .expect("supplier loc");
    let mut b = GroupBuilder::new(
        datum_core::GroupKind::Movement,
        datum_core::PostingGroupHeader {
            source_kind: "purchase_order".into(),
            source_id: None,
            work_order_id: None,
            reason_code: None,
            reverses_group_id: None,
        },
    );
    let recv = b
        .contribute(PostingIntent::Quantity(QuantityPosting {
            item,
            quantity: qty_ea(1),
            location: stock,
            boundary: None,
            lot: None,
            serial: None,
            entered: None,
        }))
        .expect("recv");
    b.contribute(PostingIntent::Quantity(QuantityPosting {
        item,
        quantity: qty_ea(-1),
        location: supplier,
        boundary: Some(Boundary::Supplier),
        lot: None,
        serial: None,
        entered: None,
    }))
    .expect("supplier");
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Inventory,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(10),
        values: Some(recv),
    }))
    .expect("inv");
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::ApAccrual,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(-10),
        values: None,
    }))
    .expect("ap");
    datum_ledger::post(tx, b).await.expect("post receipt");
}
