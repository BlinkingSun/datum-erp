//! Named commit-mode tests (SPEC).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_docs
)]

mod common;

use rust_decimal::Decimal;
use wicket_core::{
    AnyQuantity, Boundary, DimensionKind, GroupKind, Identifier, ItemId, PostingGroupHeader,
    PostingIntent, PostingSink, QuantityPosting, UnitId,
};
use wicket_db::Tx;
use wicket_ledger::{CostMethod, GroupBuilder, boundary_sql, upsert_location, upsert_stock_item};
use wicket_mod_locations::{
    CreateLocation, Error, ListFilter, Location, LocationKind, LocationStatus, LocationTreeNode,
    UpdateLocation, boundary_code, ensure_wip, install, list, list_locations, migrate,
    seed_install, store,
};
use wicket_test::db_case;

use common::{boot_kernel, has_audit, migrate_kernel, pg_code, write_ctx, write_pool};

/// Canonical example set (`PLAN.md` §3, `docs/10` §9).
const WC_LATHE_03: &str = "WC-LATHE-03";
const QUARANTINE: &str = "QUARANTINE";

fn canonical_item_mds_450() -> ItemId {
    ItemId::from_uuid(uuid::Uuid::parse_str("01932c5a-8b10-7001-8000-000000000001").unwrap())
}

fn canonical_wo_2026_1847() -> Identifier {
    Identifier::from_uuid(uuid::Uuid::parse_str("01932c5a-8b10-7001-8000-000000000006").unwrap())
}

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

    for b in wicket_mod_locations::BOUNDARY_VARIANTS {
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
    let site = store::default_site_id(&mut tx).await.unwrap();
    let bin = store::create(
        &mut tx,
        CreateLocation {
            code: WC_LATHE_03.into(),
            name: "Bin A".into(),
            site_id: site,
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
    let supplier_id = store::boundary_location_id(&mut tx, Boundary::Supplier)
        .await
        .unwrap();
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
    let mut tx = Tx::begin(&pool, &write_ctx("locations.tree"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    let site = store::default_site_id(&mut tx).await.unwrap();
    let a = store::create(
        &mut tx,
        CreateLocation {
            code: WC_LATHE_03.into(),
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
            code: QUARANTINE.into(),
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
    wicket_db::migrate::run(db.migrate_pool(), &[("wicket-uom", &wicket_uom::MIGRATOR)])
        .await
        .unwrap();
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[("wicket-ledger", &wicket_ledger::MIGRATOR)],
    )
    .await
    .unwrap();
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.onhand"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    let site = store::default_site_id(&mut tx).await.unwrap();
    let loc = store::create(
        &mut tx,
        CreateLocation {
            code: WC_LATHE_03.into(),
            name: "Stock".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Bin,
        },
    )
    .await
    .unwrap();
    let item = canonical_item_mds_450();
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
    let supplier = store::boundary_location_id(&mut tx, Boundary::Supplier)
        .await
        .unwrap();
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
    wicket_ledger::post(&mut tx, builder).await.unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&pool, &write_ctx("locations.deact"))
        .await
        .unwrap();
    let loc2 = store::get(&mut tx, loc.id).await.unwrap();
    let err = store::deactivate(&mut tx, loc.id, loc2.version)
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
    let wo = canonical_wo_2026_1847();
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

type RegistryJoinRow = (
    String,
    String,
    Option<uuid::Uuid>,
    Option<String>,
    Option<String>,
);

/// INNER JOIN `locations.location` to `ledger.location` and assert kind/status/parent
/// plus matching `boundary_class`. Missing registry row fails the JOIN.
async fn assert_registry_row_matches(tx: &mut Tx<'_>, loc: &Location) {
    let row: Option<RegistryJoinRow> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT l.kind, l.status, l.parent_id, l.boundary_class::text,
                        r.boundary_class::text
                   FROM locations.location l
                   JOIN ledger.location r ON r.location_id = l.id
                  WHERE l.id = $1",
            )
            .bind(loc.id.as_uuid()),
        )
        .await
        .unwrap();
    let (kind, status, parent_id, loc_boundary, reg_boundary) =
        row.unwrap_or_else(|| panic!("ledger.location JOIN missed {}", loc.id));
    assert_eq!(kind, loc.kind.as_sql(), "kind {}", loc.code);
    assert_eq!(status, loc.status.as_sql(), "status {}", loc.code);
    assert_eq!(
        parent_id,
        loc.parent_id.map(|p| p.as_uuid()),
        "parent {}",
        loc.code
    );
    assert_eq!(loc_boundary, reg_boundary, "registry boundary {}", loc.code);
    let expected_boundary = loc
        .boundary_class
        .map(|b| boundary_sql(b).unwrap().to_string());
    assert_eq!(
        loc_boundary, expected_boundary,
        "location boundary {}",
        loc.code
    );
}

#[tokio::test]
async fn registry_row_matches_location() {
    let db = db_case!("loc_registry");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.reg")).await.unwrap();
    seed_install(&mut tx).await.unwrap();

    for b in wicket_mod_locations::BOUNDARY_VARIANTS {
        let id = store::boundary_location_id(&mut tx, b).await.unwrap();
        let seeded = store::get(&mut tx, id).await.unwrap();
        assert_eq!(seeded.kind, LocationKind::Virtual);
        assert_eq!(seeded.status, LocationStatus::Active);
        assert!(seeded.parent_id.is_none());
        assert_eq!(seeded.boundary_class, Some(b));
        assert_registry_row_matches(&mut tx, &seeded).await;
    }

    let site = store::default_site_id(&mut tx).await.unwrap();
    let warehouse = store::create(
        &mut tx,
        CreateLocation {
            code: WC_LATHE_03.into(),
            name: "Reg".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .unwrap();
    assert_eq!(warehouse.kind, LocationKind::Warehouse);
    assert_eq!(warehouse.status, LocationStatus::Active);
    assert!(warehouse.parent_id.is_none());
    assert!(warehouse.boundary_class.is_none());
    assert_registry_row_matches(&mut tx, &warehouse).await;

    let area = store::create(
        &mut tx,
        CreateLocation {
            code: QUARANTINE.into(),
            name: "Reg area".into(),
            site_id: site,
            parent_id: Some(warehouse.id),
            kind: LocationKind::Area,
        },
    )
    .await
    .unwrap();
    assert_eq!(area.parent_id, Some(warehouse.id));
    assert_registry_row_matches(&mut tx, &area).await;

    let wo = canonical_wo_2026_1847();
    let wip_id = ensure_wip(&mut tx, wo).await.unwrap();
    let wip = store::get(&mut tx, wip_id).await.unwrap();
    assert_eq!(wip.kind, LocationKind::Wip);
    assert_eq!(wip.status, LocationStatus::Active);
    assert!(wip.parent_id.is_none());
    assert!(wip.boundary_class.is_none());
    assert_registry_row_matches(&mut tx, &wip).await;

    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn every_locations_table_is_audited_and_owned_by_wicket_owner() {
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
        assert_eq!(owner.0.as_deref(), Some("wicket_owner"));
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

#[tokio::test]
async fn list_paginates_by_cursor() {
    let db = db_case!("loc_cursor");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.list"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&pool, &write_ctx("locations.list_page"))
        .await
        .unwrap();
    let page1 = list_locations(&mut tx, Some(3), None).await.unwrap();
    assert_eq!(page1.data.len(), 3);
    assert!(page1.has_more);
    assert!(page1.next_cursor.is_some());
    let page2 = list_locations(&mut tx, Some(3), page1.next_cursor.as_deref())
        .await
        .unwrap();
    assert!(!page2.data.is_empty());
    let ids1: Vec<_> = page1.data.iter().map(|l| l.id).collect();
    let ids2: Vec<_> = page2.data.iter().map(|l| l.id).collect();
    assert!(ids1.iter().all(|id| !ids2.contains(id)));
    let err = list(
        &mut tx,
        ListFilter {
            limit: Some(0),
            cursor: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::Validation(ref m) if m == "limit"));
    db.finish().await.unwrap();
}

fn tree_codes(nodes: &[LocationTreeNode]) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(nodes: &[LocationTreeNode], out: &mut Vec<String>) {
        for node in nodes {
            out.push(node.location.code.clone());
            walk(&node.children, out);
        }
    }
    walk(nodes, &mut out);
    out
}

#[tokio::test]
async fn list_tree_filters_inactive_by_default() {
    let db = db_case!("loc_tree_status");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.tree_status"))
        .await
        .unwrap();
    install(&mut tx, true).await.unwrap();
    let site = store::default_site_id(&mut tx).await.unwrap();
    let bin = store::create(
        &mut tx,
        CreateLocation {
            code: WC_LATHE_03.into(),
            name: "Bin A".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Bin,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&pool, &write_ctx("locations.tree_deact"))
        .await
        .unwrap();
    store::deactivate(&mut tx, bin.id, bin.version)
        .await
        .unwrap();

    let active_only = store::list_tree(&mut tx, false).await.unwrap();
    let active_codes = tree_codes(&active_only);
    assert!(
        !active_codes.iter().any(|c| c == WC_LATHE_03),
        "inactive {WC_LATHE_03} must be omitted by default: {active_codes:?}"
    );
    assert!(
        active_only
            .iter()
            .all(|n| n.location.status == LocationStatus::Active)
    );

    let with_inactive = store::list_tree(&mut tx, true).await.unwrap();
    let all_codes = tree_codes(&with_inactive);
    assert!(
        all_codes.iter().any(|c| c == WC_LATHE_03),
        "include_inactive must keep {WC_LATHE_03}: {all_codes:?}"
    );
    fn has_inactive_bin(nodes: &[LocationTreeNode]) -> bool {
        nodes.iter().any(|n| {
            (n.location.code == WC_LATHE_03 && n.location.status == LocationStatus::Inactive)
                || has_inactive_bin(&n.children)
        })
    }
    assert!(has_inactive_bin(&with_inactive));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn deactivate_after_plain_install() {
    let db = db_case!("loc_deact_install");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.install"))
        .await
        .unwrap();
    install(&mut tx, true).await.unwrap();
    let site = store::default_site_id(&mut tx).await.unwrap();
    let bin = store::create(
        &mut tx,
        CreateLocation {
            code: WC_LATHE_03.into(),
            name: "Plain bin".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Bin,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&pool, &write_ctx("locations.deact_plain"))
        .await
        .unwrap();
    let done = store::deactivate(&mut tx, bin.id, bin.version)
        .await
        .unwrap();
    assert_eq!(done.status, LocationStatus::Inactive);
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

fn assert_uuid_v7(id: uuid::Uuid, label: &str) {
    assert_eq!(
        id.get_version_num(),
        7,
        "{label} expected UUID v7 version nibble, got {id}"
    );
}

#[tokio::test]
async fn generated_and_seeded_ids_are_uuid_v7() {
    let db = db_case!("loc_uuidv7");
    migrate_kernel(&db).await;
    let pool = write_pool(&db);
    let mut tx = Tx::begin(&pool, &write_ctx("locations.uuidv7"))
        .await
        .unwrap();
    seed_install(&mut tx).await.unwrap();
    let site = store::default_site_id(&mut tx).await.unwrap();
    assert_uuid_v7(site.as_uuid(), "site MAIN");
    for b in wicket_mod_locations::BOUNDARY_VARIANTS {
        let id = store::boundary_location_id(&mut tx, b).await.unwrap();
        assert_uuid_v7(id.as_uuid(), boundary_code(b));
    }
    let created = store::create(
        &mut tx,
        CreateLocation {
            code: WC_LATHE_03.into(),
            name: "v7 warehouse".into(),
            site_id: site,
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .unwrap();
    assert_uuid_v7(created.id.as_uuid(), "created warehouse");
    let wip = ensure_wip(&mut tx, canonical_wo_2026_1847()).await.unwrap();
    assert_uuid_v7(wip.as_uuid(), "ensure_wip");
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}
