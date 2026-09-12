//! Shared helpers for commit-mode inventory tests.

#![allow(dead_code, unused_imports)]

use datum_core::{
    Actor, ActorKind, AnyQuantity, CurrencyId, DimensionKind, Identifier, ItemId, LocationId,
    LotId, Money, UnitId,
};
use datum_db::{Tx, WriteContext, WritePool};
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{PrincipalKind, SYSTEM_ID, create_principal};
use datum_ledger::CostMethod;
use datum_mod_inventory::{ReceiveRequest, ReleaseRequest, receive, release_from_quarantine};
use datum_mod_items::{Kind, NewItem};
use datum_mod_locations::{CreateLocation, LocationKind, seed_install};
use datum_mod_lots::{CreateLot, LotStatus};
use datum_module::{
    Kernel, KernelBuilder, Profile, attach_kernel_audit, migrate_prefix, migrate_suffix,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, query_scalar as sql_query_scalar};

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

pub fn qty_in(s: &str) -> AnyQuantity {
    AnyQuantity {
        amount: dec(s),
        unit: IN,
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

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn boot_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "inventory.boot",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("inventory-test".into());
    ctx
}

pub fn action_ctx(w: &World, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(w.actor, action, "ui");
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("inventory-test".into());
    ctx.config_version = Some(w.kernel.profile.spec_version.clone());
    ctx
}

pub async fn migrate_all(db: &datum_test::TestDb) {
    migrate_prefix(db.migrate_pool())
        .await
        .expect("migrate prefix");
    migrate_suffix(db.migrate_pool())
        .await
        .unwrap_or_else(|e| panic!("migrate suffix: {e:#}"));
    let crates: &[(&str, &sqlx::migrate::Migrator)] = &[
        ("datum-mod-items", &datum_mod_items::MIGRATOR),
        ("datum-mod-locations", &datum_mod_locations::MIGRATOR),
        ("datum-mod-lots", &datum_mod_lots::MIGRATOR),
        ("datum-mod-inventory", &datum_mod_inventory::MIGRATOR),
    ];
    for (name, migrator) in crates {
        datum_db::migrate::run(db.migrate_pool(), &[(*name, *migrator)])
            .await
            .unwrap_or_else(|e| panic!("migrate {name}: {e:#}"));
    }
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    attach_kernel_audit(db.migrate_pool())
        .await
        .expect("attach_kernel_audit");
}

pub async fn boot_kernel(db: &datum_test::TestDb) -> Kernel {
    boot_kernel_with(db, Profile::plain_shop().unwrap()).await
}

pub async fn boot_kernel_with(db: &datum_test::TestDb, profile: Profile) -> Kernel {
    migrate_all(db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), profile.clone());
    datum_mod_items::register(&mut builder, &profile).expect("register items");
    builder
        .apply_manifest(&datum_mod_locations::manifest().expect("locations manifest"))
        .expect("register locations");
    datum_mod_lots::register(&mut builder, &profile).expect("register lots");
    datum_mod_inventory::register(&mut builder, &profile).expect("register inventory");
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
    pub wo: Identifier,
}

pub async fn seed_world(db: &datum_test::TestDb, kernel: Kernel) -> World {
    let pool = write_pool(db);
    let mut tx = Tx::begin(&pool, &boot_ctx()).await.expect("begin seed");
    seed_install(&mut tx).await.expect("seed locations");
    let site = datum_mod_locations::default_site_id(&mut tx)
        .await
        .expect("default site");
    let quarantine = datum_mod_locations::create(
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
    let available = datum_mod_locations::create(
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
    let fg = datum_mod_locations::create(
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
    let bar = datum_mod_items::create(
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
    let screw = datum_mod_items::create(
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
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.item_stock (item_id, stock_unit_id, stock_scale, residual_tolerance)
             VALUES ($1, $2, $3, $4), ($5, $6, $7, $8)",
        )
        .bind(bar.as_uuid())
        .bind(FT.0)
        .bind(4_i16)
        .bind(dec("0.0100"))
        .bind(screw.as_uuid())
        .bind(EA.0)
        .bind(0_i16)
        .bind(Decimal::ZERO),
    )
    .await
    .expect("uom.item_stock");
    let lot_ctx = boot_ctx();
    let lot_bar = datum_mod_lots::create_lot(
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
    let actor = actor_with_inventory_perms(&pool).await;
    World {
        actor,
        kernel,
        bar,
        screw,
        quarantine,
        available,
        fg,
        lot_bar,
        wo: Identifier::generate(),
    }
}

pub async fn actor_with_inventory_perms(write: &WritePool) -> Actor {
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

pub async fn consumption_count(pool: &PgPool, group: uuid::Uuid) -> i64 {
    sql_query_scalar("SELECT count(*) FROM ledger.consumption WHERE group_id = $1")
        .bind(group)
        .fetch_one(pool)
        .await
        .expect("consumption")
}

pub async fn group_kind(pool: &PgPool, group: uuid::Uuid) -> String {
    sql_query_scalar("SELECT kind::text FROM ledger.posting_group WHERE group_id = $1")
        .bind(group)
        .fetch_one(pool)
        .await
        .expect("kind")
}

pub async fn reason_code(pool: &PgPool, group: uuid::Uuid) -> Option<String> {
    sql_query_scalar("SELECT reason_code FROM ledger.posting_group WHERE group_id = $1")
        .bind(group)
        .fetch_one(pool)
        .await
        .expect("reason")
}

pub fn residual_parent_tag(parent: uuid::Uuid) -> String {
    format!("inventory.residual.parent.{parent}")
}

pub async fn residual_group_for(pool: &PgPool, parent: uuid::Uuid) -> uuid::Uuid {
    sql_query_scalar(
        "SELECT group_id FROM ledger.posting_group WHERE source_kind = $1 AND kind = 'ADJUSTMENT'",
    )
    .bind(residual_parent_tag(parent))
    .fetch_one(pool)
    .await
    .expect("residual group")
}

pub async fn assert_group_conserves(pool: &PgPool, group: uuid::Uuid) {
    let qty_bad: i64 = sql_query_scalar(
        r#"SELECT count(*) FROM (
             SELECT item_id, uom_id
               FROM ledger.posting
              WHERE group_id = $1 AND measure = 'QUANTITY'
              GROUP BY item_id, uom_id
             HAVING SUM(quantity) <> 0
           ) s"#,
    )
    .bind(group)
    .fetch_one(pool)
    .await
    .expect("qty conserve");
    assert_eq!(qty_bad, 0, "SUM(qty) must be 0 per item/uom in {group}");
    let amt_bad: i64 = sql_query_scalar(
        r#"SELECT count(*) FROM (
             SELECT currency_id
               FROM ledger.posting
              WHERE group_id = $1 AND measure = 'VALUE'
              GROUP BY currency_id
             HAVING SUM(amount) <> 0
           ) s"#,
    )
    .bind(group)
    .fetch_one(pool)
    .await
    .expect("amt conserve");
    assert_eq!(amt_bad, 0, "SUM(amount) must be 0 in {group}");
}

pub fn line(
    item: ItemId,
    entered: AnyQuantity,
    lot: Option<LotId>,
    amount: Option<Money>,
) -> datum_mod_inventory::LineInput {
    datum_mod_inventory::LineInput {
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

pub async fn receive_bars(w: &World, pool: &WritePool) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w, "inventory.receive");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin receive");
    let doc = receive(
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
    doc
}

pub async fn release_lot(w: &World, pool: &WritePool) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w, "lot.release");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin release");
    let doc = release_from_quarantine(
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
    .expect("release");
    tx.commit().await.expect("commit release");
    doc
}
