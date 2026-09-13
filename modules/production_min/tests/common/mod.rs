//! Shared helpers for commit-mode production-min tests.

#![allow(dead_code, unused_imports)]

use rust_decimal::Decimal;
use sqlx::{PgPool, query_scalar as sql_query_scalar};
use wicket_core::{
    Actor, ActorKind, AnyQuantity, CurrencyId, DimensionKind, Identifier, ItemId, LocationId,
    LotId, Money, UnitId,
};
use wicket_db::{Tx, WriteContext, WritePool};
use wicket_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use wicket_identity::{PrincipalKind, SYSTEM_ID, create_principal};
use wicket_ledger::CostMethod;
use wicket_mod_inventory::{
    IssueRequest, LineInput, ReceiveRequest, ReleaseRequest, issue_to_wip, receive,
    release_from_quarantine,
};
use wicket_mod_items::{Kind, NewItem};
use wicket_mod_locations::{CreateLocation, LocationKind, seed_install};
use wicket_mod_lots::{CreateLot, LotStatus};
use wicket_mod_production_min::{
    CompleteRequest, CreateWorkOrder, DOC_TYPE, FinishedLotTemplate, IssueMaterialRequest,
    StartRequest, Status, WorkOrder, complete, create, load, release, start,
};
use wicket_module::{Kernel, KernelBuilder, Profile};
use wicket_statemachine::DocRef;

pub const EA: UnitId = UnitId(1);
pub const IN: UnitId = UnitId(3);
pub const FT: UnitId = UnitId(4);
pub const USD: CurrencyId = CurrencyId(840);

pub fn dec(s: &str) -> Decimal {
    s.parse().expect("decimal")
}

pub fn usd(s: &str) -> Money {
    Money::new(dec(s), USD).expect("money")
}

pub fn qty_ft(s: &str) -> AnyQuantity {
    AnyQuantity {
        amount: dec(s),
        unit: FT,
        dimension: DimensionKind::Length,
    }
}

pub fn qty_ea(s: &str) -> AnyQuantity {
    AnyQuantity {
        amount: dec(s),
        unit: EA,
        dimension: DimensionKind::Count,
    }
}

pub fn write_pool(db: &wicket_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn boot_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "production.boot",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("production-test".into());
    ctx
}

pub fn action_ctx(actor: Actor, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(actor, action, "ui");
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("production-test".into());
    ctx.config_version = Some("1.0.0".into());
    ctx
}

pub fn edge_ctx(kernel: &Kernel, actor: Actor, id: Identifier, edge: &str) -> WriteContext {
    let doc = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: id,
    };
    let mut ctx = kernel.transition_context(actor, &doc, edge);
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("production-test".into());
    ctx
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
    wicket_module::attach_slice_audit(db.migrate_pool())
        .await
        .expect("attach_slice_audit");
}

pub async fn migrate_all(db: &wicket_test::TestDb) {
    install_through(db, "wicket-mod-production-min").await;
}

pub async fn boot_kernel(db: &wicket_test::TestDb) -> Kernel {
    migrate_all(db).await;
    let profile = Profile::plain_shop().unwrap();
    let mut builder = Kernel::builder(db.app_pool().clone(), profile.clone());
    wicket_mod_items::register(&mut builder, &profile).expect("register items");
    builder
        .apply_manifest(&wicket_mod_locations::manifest().expect("locations manifest"))
        .expect("register locations");
    wicket_mod_lots::register(&mut builder, &profile).expect("register lots");
    wicket_mod_inventory::register(&mut builder, &profile).expect("register inventory");
    wicket_mod_production_min::register(&mut builder, &profile).expect("register production");
    builder.build().await.expect("kernel build")
}

pub struct World {
    pub actor: Actor,
    pub kernel: Kernel,
    pub bar: ItemId,
    pub screw: ItemId,
    pub quarantine: LocationId,
    pub available: LocationId,
    pub fg: LocationId,
    pub lot_bar: LotId,
}

pub async fn seed_world(db: &wicket_test::TestDb, kernel: Kernel) -> World {
    let pool = write_pool(db);
    let mut tx = Tx::begin(&pool, &boot_ctx()).await.expect("begin seed");
    seed_install(&mut tx).await.expect("seed locations");
    let site = wicket_mod_locations::default_site_id(&mut tx)
        .await
        .expect("default site");
    let quarantine = wicket_mod_locations::create(
        &mut tx,
        CreateLocation {
            code: "WH-Q".into(),
            name: "Quarantine".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .expect("quarantine")
    .id;
    let available = wicket_mod_locations::create(
        &mut tx,
        CreateLocation {
            code: "WH-A".into(),
            name: "Available".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .expect("available")
    .id;
    let fg = wicket_mod_locations::create(
        &mut tx,
        CreateLocation {
            code: "WH-FG".into(),
            name: "Finished goods".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .expect("fg")
    .id;
    let bar = wicket_mod_items::create(
        &mut tx,
        &kernel,
        NewItem {
            number: "RM-TI-BAR-12".into(),
            revision: "A".into(),
            description: "Titanium bar, stocked in feet".into(),
            kind: Kind::Buy,
            stock_uom: FT,
            stock_scale: 4,
            residual_tolerance: dec("0.0100"),
            cost_method: CostMethod::Fifo,
            standard: None,
        },
    )
    .await
    .expect("bar")
    .id;
    let screw = wicket_mod_items::create(
        &mut tx,
        &kernel,
        NewItem {
            number: "MDS-450-M4x12".into(),
            revision: "C".into(),
            description: "Cortical bone screw, Ti-6Al-4V ELI, M4 x 12".into(),
            kind: Kind::Make,
            stock_uom: EA,
            stock_scale: 0,
            residual_tolerance: Decimal::ZERO,
            cost_method: CostMethod::Standard,
            standard: Some(usd("0.25")),
        },
    )
    .await
    .expect("screw")
    .id;
    let lot_ctx = boot_ctx();
    let lot_bar = wicket_mod_lots::create_lot(
        &mut tx,
        &kernel,
        &lot_ctx,
        CreateLot {
            item: bar,
            number: Some("LOT-BAR-24-4412".into()),
            template: None,
            supplier_lot: Some("ATI-HEAT".into()),
            heat_or_source_ref: Some("HT-ATI-24-8831".into()),
            expiry: None,
            cert_ref: None,
            status: LotStatus::Quarantine,
        },
    )
    .await
    .expect("lot")
    .id;
    tx.commit().await.expect("commit seed");
    let actor = actor_with_production_perms(&pool).await;
    World {
        actor,
        kernel,
        bar,
        screw,
        quarantine,
        available,
        fg,
        lot_bar,
    }
}

pub async fn actor_with_production_perms(write: &WritePool) -> Actor {
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
                "production.view".into(),
                "production.create".into(),
                "production.release".into(),
                "production.issue".into(),
                "production.complete".into(),
                "inventory.view".into(),
                "inventory.receive".into(),
                "inventory.issue".into(),
                "inventory.move".into(),
                "inventory.adjust".into(),
                "inventory.count".into(),
                "items.view".into(),
                "items.edit".into(),
                "lots.view".into(),
                "lots.edit".into(),
                "lots.release".into(),
                "locations.view".into(),
                "locations.edit".into(),
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

pub fn line(
    item: ItemId,
    entered: AnyQuantity,
    lot: Option<LotId>,
    amount: Option<Money>,
) -> LineInput {
    LineInput {
        item,
        entered,
        lot,
        serial: None,
        from_location: None,
        to_location: None,
        package: None,
        amount,
        reason_code: None,
    }
}

pub async fn receive_bars(w: &World, pool: &WritePool) {
    let ctx = action_ctx(w.actor, "inventory.receive");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin receive");
    receive(
        &mut tx,
        &w.kernel,
        &ctx,
        ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-2024-0841".into()),
            lines: vec![line(
                w.bar,
                qty_ft("2000.0000"),
                Some(w.lot_bar),
                Some(usd("4720.00")),
            )],
            expected: Some(qty_ft("2000.0000")),
            tolerance: Some(dec("0.0000")),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("receive");
    tx.commit().await.expect("commit receive");
}

pub async fn release_lot(w: &World, pool: &WritePool) {
    let ctx = action_ctx(w.actor, "lot.release");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin release");
    release_from_quarantine(
        &mut tx,
        &w.kernel,
        &ctx,
        ReleaseRequest {
            lot: w.lot_bar,
            from_location: w.quarantine,
            to_location: w.available,
            entered: qty_ft("2000.0000"),
            amount: Some(usd("4720.00")),
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("release lot");
    tx.commit().await.expect("commit release lot");
}

pub async fn create_wo(w: &World, pool: &WritePool) -> WorkOrder {
    let ctx = action_ctx(w.actor, "production.create");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin create");
    let wo = create(
        &mut tx,
        &w.kernel,
        CreateWorkOrder {
            item: w.screw,
            quantity_ordered: qty_ea("500"),
            revision: "C".into(),
        },
    )
    .await
    .expect("create wo");
    tx.commit().await.expect("commit create");
    wo
}

pub async fn release_wo(w: &World, pool: &WritePool, id: Identifier) -> WorkOrder {
    let ctx = edge_ctx(&w.kernel, w.actor, id, "release");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin release");
    let wo = release(&mut tx, &w.kernel, &ctx, id)
        .await
        .expect("release wo");
    tx.commit().await.expect("commit release");
    wo
}

/// Issue bar stock and start the work order in one `production.issue` transaction.
pub async fn issue_and_start(w: &World, pool: &WritePool, wo_id: Identifier) -> WorkOrder {
    let ctx = edge_ctx(&w.kernel, w.actor, wo_id, "issue");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin issue+start");
    let wo = start(
        &mut tx,
        &w.kernel,
        &ctx,
        StartRequest {
            work_order: wo_id,
            issue: Some(IssueMaterialRequest {
                work_order: wo_id,
                from_location: w.available,
                lines: vec![line(
                    w.bar,
                    qty_ft("20.0000"),
                    Some(w.lot_bar),
                    Some(usd("47.20")),
                )],
                idempotency_key: Some(uuid::Uuid::now_v7()),
            }),
        },
    )
    .await
    .expect("issue+start");
    tx.commit().await.expect("commit issue+start");
    wo
}

pub async fn start_wo(w: &World, pool: &WritePool, id: Identifier) -> WorkOrder {
    let ctx = edge_ctx(&w.kernel, w.actor, id, "issue");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin start");
    let wo = start(
        &mut tx,
        &w.kernel,
        &ctx,
        StartRequest {
            work_order: id,
            issue: None,
        },
    )
    .await
    .expect("start");
    tx.commit().await.expect("commit start");
    wo
}

pub async fn complete_wo(
    w: &World,
    pool: &WritePool,
    id: Identifier,
) -> wicket_mod_production_min::Completion {
    let ctx = edge_ctx(&w.kernel, w.actor, id, "complete");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin complete");
    let c = complete(
        &mut tx,
        &w.kernel,
        &ctx,
        CompleteRequest {
            work_order: id,
            to_location: w.fg,
            good: qty_ea("500"),
            scrap: qty_ea("0"),
            finished_lot: FinishedLotTemplate {
                number: Some("LOT-WO-1847".into()),
                template: None,
                serial_template: None,
            },
        },
    )
    .await
    .expect("complete");
    tx.commit().await.expect("commit complete");
    c
}
