//! Shared helpers for commit-mode genealogy tests.

#![allow(dead_code, unused_imports)]

use datum_core::{
    Actor, ActorKind, AnyQuantity, Boundary, CostElement, CurrencyId, DimensionKind, GroupKind,
    Identifier, ItemId, LocationId, LotId, Money, PostingGroupHeader, PostingIntent, PostingSink,
    QuantityPosting, SerialId, UnitId, ValueAccount, ValuePosting,
};
use datum_db::{Tx, WriteContext, WritePool};
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{PrincipalKind, SYSTEM_ID, create_principal};
use datum_ledger::{CostMethod, GroupBuilder, load_open_layers};
use datum_mod_items::{Kind, NewItem};
use datum_mod_locations::{CreateLocation, LocationKind, seed_install};
use datum_mod_lots::{CreateLot, LotStatus};
use datum_module::{Kernel, KernelBuilder, Profile};
use rust_decimal::Decimal;
use sqlx::{PgPool, query_scalar as sql_query_scalar};

pub const EA: UnitId = UnitId(1);
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

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn boot_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "genealogy.boot",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("genealogy-test".into());
    ctx
}

pub fn action_ctx(actor: Actor, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(actor, action, "ui");
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("genealogy-test".into());
    ctx
}

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
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
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
    datum_module::attach_slice_audit(db.migrate_pool())
        .await
        .expect("attach_slice_audit");
}

pub async fn migrate_all(db: &datum_test::TestDb) {
    install_through(db, "datum-mod-genealogy").await;
}

pub async fn boot_kernel(db: &datum_test::TestDb) -> Kernel {
    migrate_all(db).await;
    let profile = Profile::plain_shop().unwrap();
    let mut builder = Kernel::builder(db.app_pool().clone(), profile.clone());
    datum_mod_items::register(&mut builder, &profile).expect("register items");
    builder
        .apply_manifest(&datum_mod_locations::manifest().expect("locations manifest"))
        .expect("register locations");
    datum_mod_lots::register(&mut builder, &profile).expect("register lots");
    datum_mod_inventory::register(&mut builder, &profile).expect("register inventory");
    datum_mod_genealogy::register(&mut builder, &profile).expect("register genealogy");
    let kernel = builder.build().await.expect("kernel build");
    let pool = write_pool(db);
    let mut tx = Tx::begin(&pool, &boot_ctx()).await.expect("wire tx");
    datum_mod_genealogy::wire(&kernel, &mut tx)
        .await
        .expect("wire genealogy");
    tx.commit().await.expect("commit wire");
    kernel
}

pub struct World {
    pub actor: Actor,
    pub kernel: Kernel,
    pub bar: ItemId,
    pub screw: ItemId,
    pub quarantine: LocationId,
    pub available: LocationId,
    pub fg: LocationId,
    pub lot_heat: LotId,
    pub lot_bar: LotId,
    pub lot_fg: LotId,
    pub serial: SerialId,
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
    let lot_ctx = boot_ctx();
    let lot_heat = datum_mod_lots::create_lot(
        &mut tx,
        &kernel,
        &lot_ctx,
        CreateLot {
            item: bar,
            number: Some("HT-ATI-24-8831".into()),
            template: None,
            supplier_lot: Some("ATI-HEAT".into()),
            heat_or_source_ref: None,
            expiry: None,
            cert_ref: None,
            status: LotStatus::Quarantine,
        },
    )
    .await
    .expect("heat lot")
    .id;
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
    .expect("bar lot")
    .id;
    let lot_fg = datum_mod_lots::create_lot(
        &mut tx,
        &kernel,
        &lot_ctx,
        CreateLot {
            item: screw,
            number: Some("LOT-WO-1847".into()),
            template: None,
            supplier_lot: None,
            heat_or_source_ref: Some("HT-ATI-24-8831".into()),
            expiry: None,
            cert_ref: None,
            status: LotStatus::Quarantine,
        },
    )
    .await
    .expect("fg lot")
    .id;
    let serials = datum_mod_lots::create_serials(&mut tx, &kernel, lot_fg, 1, None)
        .await
        .expect("serial");
    tx.commit().await.expect("commit seed");
    let actor = actor_with_perms(&pool).await;
    World {
        actor,
        kernel,
        bar,
        screw,
        quarantine,
        available,
        fg,
        lot_heat,
        lot_bar,
        lot_fg,
        serial: serials[0].id,
        wo: Identifier::generate(),
    }
}

pub async fn actor_with_perms(write: &WritePool) -> Actor {
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
                "genealogy.view".into(),
                "genealogy.export".into(),
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

pub fn line_serial(
    item: ItemId,
    entered: AnyQuantity,
    lot: Option<LotId>,
    serial: Option<SerialId>,
    amount: Option<Money>,
) -> datum_mod_inventory::LineInput {
    let mut l = line(item, entered, lot, amount);
    l.serial = serial;
    l
}

pub async fn receive_heat(w: &World, pool: &WritePool) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w.actor, "inventory.receive");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin receive");
    let doc = datum_mod_inventory::receive(
        &mut tx,
        &w.kernel,
        &ctx,
        datum_mod_inventory::ReceiveRequest {
            to_location: w.quarantine,
            reference: Some("PO-2024-0841".into()),
            lines: vec![line(
                w.bar,
                qty_ft("2000.0000"),
                Some(w.lot_heat),
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

pub async fn release_heat(w: &World, pool: &WritePool) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w.actor, "lot.release");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin release");
    let doc = datum_mod_inventory::release_from_quarantine(
        &mut tx,
        &w.kernel,
        &ctx,
        datum_mod_inventory::ReleaseRequest {
            lot: w.lot_heat,
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

pub async fn issue_heat(w: &World, pool: &WritePool) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w.actor, "inventory.issue");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin issue");
    let doc = datum_mod_inventory::issue_to_wip(
        &mut tx,
        &w.kernel,
        &ctx,
        datum_mod_inventory::IssueRequest {
            work_order: w.wo,
            from_location: w.available,
            reference: Some("WO-2026-1847".into()),
            lines: vec![line(
                w.bar,
                qty_ft("20.0000"),
                Some(w.lot_heat),
                Some(usd("47.20")),
            )],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("issue");
    tx.commit().await.expect("commit issue");
    doc
}

fn q_post(
    item: ItemId,
    qty: AnyQuantity,
    location: LocationId,
    boundary: Option<Boundary>,
    lot: Option<LotId>,
    serial: Option<SerialId>,
) -> QuantityPosting {
    QuantityPosting {
        item,
        quantity: qty,
        location,
        boundary,
        lot,
        serial,
        entered: None,
    }
}

fn v_post(
    account: ValueAccount,
    amount: Money,
    values: Option<datum_core::PostingHandle>,
    cost_object: Option<Identifier>,
) -> ValuePosting {
    ValuePosting {
        account,
        cost_element: CostElement::Material,
        cost_object,
        amount,
        values,
    }
}

/// TRANSFORMATION: consume released heat at available, produce finished lot.
pub async fn complete_wo(w: &World, pool: &WritePool) {
    let ctx = action_ctx(w.actor, "production.complete");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin xform");
    let consumed = datum_mod_locations::boundary_location_id(&mut tx, Boundary::Consumed)
        .await
        .expect("consumed");
    let produced = datum_mod_locations::boundary_location_id(&mut tx, Boundary::Produced)
        .await
        .expect("produced");
    let layers = load_open_layers(&mut tx, w.bar, w.available)
        .await
        .expect("layers");
    let layer = layers
        .iter()
        .find(|l| l.lot == Some(w.lot_heat))
        .expect("available heat layer")
        .posting_id;
    let header = PostingGroupHeader {
        source_kind: "production.complete".into(),
        source_id: Some(w.wo),
        work_order_id: Some(w.wo),
        reason_code: None,
        reverses_group_id: None,
    };
    let mut b = GroupBuilder::new(GroupKind::Transformation, header);
    let bar_out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-20.0000"),
            w.available,
            None,
            Some(w.lot_heat),
            None,
        )))
        .unwrap();
    let consumed_h = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("20.0000"),
            consumed,
            Some(Boundary::Consumed),
            Some(w.lot_heat),
            None,
        )))
        .unwrap();
    let screw_in = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("50"),
            w.fg,
            None,
            Some(w.lot_fg),
            Some(w.serial),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("-50"),
        produced,
        Some(Boundary::Produced),
        Some(w.lot_fg),
        Some(w.serial),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("-47.20"),
        Some(bar_out),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Wip,
        cost_element: CostElement::Labor,
        cost_object: Some(w.wo),
        amount: usd("-15.00"),
        values: Some(consumed_h),
    }))
    .unwrap();
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Wip,
        cost_element: CostElement::Burden,
        cost_object: Some(w.wo),
        amount: usd("-7.50"),
        values: Some(consumed_h),
    }))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("12.50"),
        Some(screw_in),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("57.20"),
        Some(consumed_h),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
        consuming: screw_in,
        consumed_posting_id: layer,
        quantity: qty_ft("20.0000"),
        amount: usd("47.20"),
    }))
    .unwrap();
    datum_ledger::post(&mut tx, b).await.expect("xform");
    tx.commit().await.expect("commit xform");
}

pub async fn ship_fg(
    w: &World,
    pool: &WritePool,
    order: Identifier,
) -> datum_mod_inventory::Document {
    let ctx = action_ctx(w.actor, "inventory.issue");
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin ship");
    let doc = datum_mod_inventory::ship_to_customer(
        &mut tx,
        &w.kernel,
        &ctx,
        datum_mod_inventory::ShipRequest {
            order,
            from_location: w.fg,
            reference: Some("SO-2026-0101".into()),
            lines: vec![line_serial(
                w.screw,
                qty_ea("10"),
                Some(w.lot_fg),
                Some(w.serial),
                Some(usd("2.50")),
            )],
            idempotency_key: Some(uuid::Uuid::now_v7()),
        },
    )
    .await
    .expect("ship");
    tx.commit().await.expect("commit ship");
    doc
}
