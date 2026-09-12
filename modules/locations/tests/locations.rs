//! Named commit-mode tests (SPEC).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_docs
)]

mod common;

use datum_core::{
    AnyQuantity, Boundary, DimensionKind, GroupKind, Identifier, ItemId, PostingGroupHeader,
    PostingIntent, PostingSink, QuantityPosting, UnitId,
};
use datum_db::Tx;
use datum_events::SchemaRegistry;
use datum_ledger::{CostMethod, GroupBuilder, upsert_location, upsert_stock_item};
use datum_mod_locations::{
    CreateLocation, Error, LocationKind, UpdateLocation, boundary_code, ensure_wip, install,
    migrate, register_schemas, seed_install, store,
};
use datum_test::db_case;
use rust_decimal::Decimal;

use common::{boot_kernel, has_audit, migrate_kernel, pg_code, write_ctx, write_pool};

#[tokio::test]
async fn install_seeds_seven_boundary_locations_once() {
    let db = db_case!("loc_seed");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.install"))
        .await
        .unwrap();
    install(&mut tx, true).await.unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&pool, &write_ctx("locations.install2"))
        .await
        .unwrap();
    install(&mut tx, true).await.unwrap();
    tx.commit().await.unwrap();

    let count: (i64,) =
        sqlx::query_as("SELECT count(*) FROM locations.location WHERE boundary_class IS NOT NULL")
            .fetch_one(db.migrate_pool())
            .await
            .unwrap();
    assert_eq!(count.0, 7);

    for b in datum_mod_locations::BOUNDARY_VARIANTS {
        let code = boundary_code(b);
        let row: (String,) = sqlx::query_as("SELECT code FROM locations.location WHERE code = $1")
            .bind(code)
            .fetch_one(db.migrate_pool())
            .await
            .unwrap();
        assert_eq!(row.0, code);
    }
    db.finish().await.unwrap();
}

#[tokio::test]
async fn boundary_class_is_immutable() {
    let db = db_case!("loc_immutable");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.seed"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    let bin = store::create(
        &mut tx,
        CreateLocation {
            code: "BIN-A".into(),
            name: "Bin A".into(),
            site_id: store::default_site_id(),
            parent_id: None,
            kind: LocationKind::Bin,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&pool, &write_ctx("locations.update"))
        .await
        .unwrap();
    let updated = store::update(
        &mut tx,
        bin.id,
        UpdateLocation {
            name: Some("Renamed".into()),
            version: bin.version,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(updated.boundary_class.is_none());
    let supplier_id = store::boundary_location_id(Boundary::Supplier);
    let supplier = store::get(&mut tx, supplier_id).await.unwrap();
    assert_eq!(supplier.boundary_class, Some(Boundary::Supplier));
    let err = store::update(
        &mut tx,
        supplier_id,
        UpdateLocation {
            name: Some("Hack".into()),
            version: supplier.version,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(err.boundary_class, Some(Boundary::Supplier));
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn tree_has_no_cycles() {
    let db = db_case!("loc_cycle");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let site = datum_mod_locations::store::default_site_id();
    let mut tx = Tx::begin(&pool, &write_ctx("locations.tree"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    let a = store::create(
        &mut tx,
        CreateLocation {
            code: "WH-A".into(),
            name: "A".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .unwrap();
    let b = store::create(
        &mut tx,
        CreateLocation {
            code: "WH-B".into(),
            name: "B".into(),
            site_id: site,
            parent_id: Some(a.id),
            kind: LocationKind::Area,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&pool, &write_ctx("locations.cycle"))
        .await
        .unwrap();
    let err = store::update(
        &mut tx,
        a.id,
        UpdateLocation {
            parent_id: Some(Some(b.id)),
            version: a.version,
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::Cycle));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn deactivate_refused_while_on_hand() {
    let db = db_case!("loc_onhand");
    migrate_kernel(&db).await;
    migrate(db.migrate_pool()).await.unwrap();
    datum_db::migrate::run(db.migrate_pool(), &[("datum-uom", &datum_uom::MIGRATOR)])
        .await
        .unwrap();
    datum_db::migrate::run(
        db.migrate_pool(),
        &[("datum-ledger", &datum_ledger::MIGRATOR)],
    )
    .await
    .unwrap();
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.onhand"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    let loc = store::create(
        &mut tx,
        CreateLocation {
            code: "STOCK-1".into(),
            name: "Stock".into(),
            site_id: datum_mod_locations::store::default_site_id(),
            parent_id: None,
            kind: LocationKind::Bin,
        },
    )
    .await
    .unwrap();
    let item = ItemId::generate();
    upsert_stock_item(
        &mut tx,
        item,
        UnitId(1),
        0,
        Decimal::ZERO,
        CostMethod::Fifo,
        None,
    )
    .await
    .unwrap();
    upsert_location(&mut tx, loc.id, None).await.unwrap();
    let supplier = store::boundary_location_id(Boundary::Supplier);
    let mut header = PostingGroupHeader {
        source_kind: "test".into(),
        source_id: None,
        work_order_id: None,
        reason_code: None,
        reverses_group_id: None,
    };
    header.source_id = Some(Identifier::generate());
    let mut builder = GroupBuilder::new(GroupKind::Movement, header);
    builder
        .contribute(PostingIntent::Quantity(QuantityPosting {
            item,
            quantity: AnyQuantity {
                amount: Decimal::from(5),
                unit: UnitId(1),
                dimension: DimensionKind::Count,
            },
            location: loc.id,
            boundary: None,
            lot: None,
            serial: None,
            entered: None,
        }))
        .unwrap();
    builder
        .contribute(PostingIntent::Quantity(QuantityPosting {
            item,
            quantity: AnyQuantity {
                amount: Decimal::from(-5),
                unit: UnitId(1),
                dimension: DimensionKind::Count,
            },
            location: supplier,
            boundary: Some(Boundary::Supplier),
            lot: None,
            serial: None,
            entered: None,
        }))
        .unwrap();
    datum_ledger::post(&mut tx, builder).await.unwrap();
    tx.commit().await.unwrap();

    let mut registry = SchemaRegistry::new();
    register_schemas(&mut registry).unwrap();
    let mut tx = Tx::begin(&pool, &write_ctx("locations.deact"))
        .await
        .unwrap();
    let loc2 = store::get(&mut tx, loc.id).await.unwrap();
    let err = store::deactivate(&mut tx, loc.id, loc2.version, &registry)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::OnHand));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn ensure_wip_is_idempotent_per_work_order() {
    let db = db_case!("loc_wip");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let wo = Identifier::generate();
    let mut tx = Tx::begin(&pool, &write_ctx("locations.wip1"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    let a = ensure_wip(&mut tx, wo).await.unwrap();
    let b = ensure_wip(&mut tx, wo).await.unwrap();
    assert_eq!(a, b);
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn registry_row_matches_location() {
    let db = db_case!("loc_registry");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.reg")).await.unwrap();
    seed_install(&mut tx).await.unwrap();
    let loc = store::create(
        &mut tx,
        CreateLocation {
            code: "REG-1".into(),
            name: "Reg".into(),
            site_id: store::default_site_id(),
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .unwrap();
    let supplier_id = store::boundary_location_id(Boundary::Supplier);
    let supplier = store::get(&mut tx, supplier_id).await.unwrap();
    assert_eq!(supplier.boundary_class, Some(Boundary::Supplier));
    assert!(loc.boundary_class.is_none());
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn every_locations_table_is_audited_and_owned_by_datum_owner() {
    let db = db_case!("loc_audit");
    let kernel = boot_kernel(&db).await;
    migrate(db.migrate_pool()).await.unwrap();
    let ctx = common::edge_ctx(&kernel, write_ctx("locations.create").actor, "create");
    let cfg = ctx.config_version.as_deref().unwrap_or("");
    assert!(!cfg.is_empty(), "config_version must be non-empty");
    assert_eq!(
        cfg, kernel.profile.spec_version,
        "config_version equals the profile spec"
    );
    for table in ["site", "location"] {
        assert!(has_audit(db.migrate_pool(), "locations", table).await);
        let owner: (Option<String>,) = sqlx::query_as(
            "SELECT pg_catalog.pg_get_userbyid(c.relowner)
               FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = 'locations' AND c.relname = $1",
        )
        .bind(table)
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
        assert_eq!(owner.0.as_deref(), Some("datum_owner"));
    }
    db.finish().await.unwrap();
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("loc_writes_tx");
    migrate_kernel(&db).await;
    migrate(db.migrate_pool()).await.unwrap();
    let err = sqlx::query(
        "INSERT INTO locations.site (id, code, name, version)
         VALUES (gen_random_uuid(), 'RAW', 'Raw', 1)",
    )
    .execute(db.app_pool())
    .await
    .unwrap_err();
    assert_eq!(pg_code(&err), "42501");
    db.finish().await.unwrap();
}
