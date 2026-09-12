//! Wave 2s.1 composed install: items, locations, lots on a fresh database.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::{
    AnyQuantity, Boundary, CostElement, CurrencyId, DimensionKind, Identifier, ItemId, LocationId,
    LotId, Money, PostingIntent, QuantityPosting, SignatureError, UnitId, ValueAccount,
    ValuePosting,
};
use datum_db::Tx;
use datum_ledger::{rebuild, upsert_location, verify_projection};
use datum_mod_items::{Kind, NewItem, create};
use datum_mod_locations::{
    CreateLocation, LocationKind, boundary_location_id, create as create_location, default_site_id,
};
use datum_mod_lots::{CreateLot, DOC_TYPE as LOT_DOC, LotStatus, create_lot};
use datum_statemachine::{DocRef, Veto};
use datum_test::db_case;
use rust_decimal::Decimal;
use sqlx::query_scalar;
use std::sync::{Arc, Mutex};

use datum_module::{Kernel, KernelBuilder, Profile, SignatureEdge, lots_release_is_required};

use common::{actor_with_perms, boot_ctx, migrate_and_install, pg_code};

fn register_wave_2s1(builder: &mut KernelBuilder, profile: &Profile) {
    datum_mod_items::register(builder, profile).expect("items");
    builder
        .apply_manifest(&datum_mod_locations::manifest().expect("locations manifest"))
        .expect("locations routes");
    datum_mod_lots::register(builder, profile).expect("lots");
}

const EA: UnitId = UnitId(1);

fn qty_ea(n: i64) -> AnyQuantity {
    AnyQuantity {
        amount: Decimal::from(n),
        unit: EA,
        dimension: DimensionKind::Count,
    }
}

fn usd(n: i64) -> Money {
    Money::new(Decimal::from(n), CurrencyId(840)).expect("usd")
}

fn lot_receipt(
    sink: &mut dyn datum_core::PostingSink,
    item: ItemId,
    lot: LotId,
    stock: LocationId,
    supplier: LocationId,
) -> core::result::Result<(), Veto> {
    let veto = |e: datum_core::PostingError| Veto {
        module: "mod-lots".into(),
        reason: e.to_string(),
    };
    let recv = sink
        .contribute(PostingIntent::Quantity(QuantityPosting {
            item,
            quantity: qty_ea(1),
            location: stock,
            boundary: None,
            lot: Some(lot),
            serial: None,
            entered: None,
        }))
        .map_err(veto)?;
    sink.contribute(PostingIntent::Quantity(QuantityPosting {
        item,
        quantity: qty_ea(-1),
        location: supplier,
        boundary: Some(Boundary::Supplier),
        lot: None,
        serial: None,
        entered: None,
    }))
    .map_err(veto)?;
    sink.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Inventory,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(10),
        values: Some(recv),
    }))
    .map_err(veto)?;
    sink.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::ApAccrual,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(-10),
        values: None,
    }))
    .map_err(veto)?;
    Ok(())
}

async fn compose_once(db: datum_test::TestDb, profile: Profile, label: &str) {
    migrate_and_install(&db).await;

    let supplier = boundary_location_id(Boundary::Supplier);
    let stock = LocationId::generate();
    let item_slot: Arc<Mutex<Option<ItemId>>> = Arc::new(Mutex::new(None));

    let mut builder = Kernel::builder(db.app_pool().clone(), profile.clone());
    register_wave_2s1(&mut builder, &profile);
    builder.register_hook("mod-lots", LOT_DOC, "release", {
        let item_slot = Arc::clone(&item_slot);
        move |view, sink| {
            let item = item_slot
                .lock()
                .expect("item slot")
                .expect("item set before transition");
            let lot = LotId::from_uuid(view.doc_id.as_uuid());
            lot_receipt(sink, item, lot, stock, supplier)
        }
    });
    let kernel = builder.build().await.expect("kernel build");

    let write = kernel.write_pool();
    let actor = actor_with_perms(
        &write,
        &[
            "items.edit",
            "items.release",
            "locations.edit",
            "lots.create",
            "lots.status",
        ],
    )
    .await;

    let mut boot = boot_ctx();
    boot.config_version = Some(profile.spec_version.clone());
    let mut tx = Tx::begin(&write, &boot).await.expect("boot tx");
    upsert_location(&mut tx, stock, None)
        .await
        .expect("stock ledger");
    upsert_location(&mut tx, supplier, Some(Boundary::Supplier))
        .await
        .expect("supplier ledger");
    tx.commit().await.expect("boot commit");

    let mut ctx = datum_db::WriteContext::new(actor, "items.create", "ui");
    ctx.config_version = Some(profile.spec_version.clone());
    let mut tx = Tx::begin(&write, &ctx).await.expect("item tx");
    let item = create(
        &mut tx,
        &kernel,
        NewItem {
            number: "MDS-450-M4x12".into(),
            revision: "C".into(),
            description: "Compose screw".into(),
            kind: Kind::Make,
            stock_uom: EA,
            stock_scale: 0,
            residual_tolerance: Decimal::ZERO,
            cost_method: datum_ledger::CostMethod::Fifo,
            standard: None,
        },
    )
    .await
    .expect("create item");
    *item_slot.lock().expect("item slot") = Some(item.id);
    tx.commit().await.expect("commit item");

    let mut ctx = datum_db::WriteContext::new(actor, "locations.create", "ui");
    ctx.config_version = Some(profile.spec_version.clone());
    let mut tx = Tx::begin(&write, &ctx).await.expect("loc tx");
    let _wh = create_location(
        &mut tx,
        CreateLocation {
            code: "WH-A".into(),
            name: "Warehouse A".into(),
            site_id: default_site_id(),
            parent_id: None,
            kind: LocationKind::Warehouse,
        },
    )
    .await
    .expect("warehouse");
    let _bin = create_location(
        &mut tx,
        CreateLocation {
            code: "BIN-01".into(),
            name: "Bin 01".into(),
            site_id: default_site_id(),
            parent_id: None,
            kind: LocationKind::Bin,
        },
    )
    .await
    .expect("bin");
    tx.commit().await.expect("commit loc");

    let mut ctx = datum_db::WriteContext::new(actor, "lots.create", "ui");
    ctx.config_version = Some(profile.spec_version.clone());
    let mut tx = Tx::begin(&write, &ctx).await.expect("lot tx");
    let lot = create_lot(
        &mut tx,
        CreateLot {
            item: item.id,
            number: Some("LOT-BAR-24-4412".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .expect("create lot");
    tx.commit().await.expect("commit lot");

    let doc = DocRef {
        doc_type: LOT_DOC.into(),
        doc_id: Identifier::from_uuid(lot.id.as_uuid()),
    };
    let mut rel_ctx = kernel.transition_context(actor, &doc, "release");
    rel_ctx.actor_display = Some("Operator".into());
    rel_ctx.reason = Some("2s1-compose".into());

    let mut tx = Tx::begin(&write, &rel_ctx).await.expect("spawn lot");
    kernel
        .spawn(&mut tx, &doc, "quarantine")
        .await
        .expect("spawn lot");
    tx.commit().await.expect("commit spawn");

    if lots_release_is_required(&profile) {
        let mut tx = Tx::begin(&write, &rel_ctx).await.expect("regulated tr");
        let err = kernel
            .transition(&mut tx, &doc, "release", None, &rel_ctx)
            .await
            .expect_err("Required refuses under NoSignatures");
        assert!(
            matches!(
                err,
                datum_module::Error::Statemachine(datum_statemachine::Error::Signature(
                    SignatureError::NoProvider
                ))
            ) || matches!(
                err,
                datum_module::Error::Statemachine(datum_statemachine::Error::Signature(
                    SignatureError::Invalid(_)
                ))
            ),
            "{label}: signature gate must refuse, got {err:?}"
        );
        tx.rollback().await.ok();
        let groups: i64 = query_scalar("SELECT count(*) FROM ledger.posting_group")
            .fetch_one(db.app_pool())
            .await
            .expect("groups");
        assert_eq!(groups, 0, "{label}: Required must post nothing");
        db.finish().await.expect("finish");
        return;
    }

    let mut tx = Tx::begin(&write, &rel_ctx).await.expect("release tr");
    kernel
        .transition(&mut tx, &doc, "release", None, &rel_ctx)
        .await
        .expect("lot release posts receipt");
    tx.commit().await.expect("commit release");

    let qty: Decimal = query_scalar(
        "SELECT COALESCE(SUM(quantity), 0) FROM ledger.posting WHERE measure = 'QUANTITY'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("sum qty");
    assert_eq!(qty, Decimal::ZERO, "{label}: ledger conserved");

    let cfg_ver: String =
        query_scalar("SELECT configuration_version FROM items.item WHERE id = $1")
            .bind(item.id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("cfg ver");
    assert_eq!(cfg_ver, profile.spec_version, "{label}: config_version");

    let app_ver: String = query_scalar(
        "SELECT app_version FROM audit.event
          WHERE table_name = 'posting_group' AND op = 'INSERT'
          ORDER BY at DESC LIMIT 1",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("audit app");
    assert!(!app_ver.is_empty(), "{label}: audit app_version");

    let werr = sqlx::query("UPDATE items.item SET description = 'x' WHERE id = $1")
        .bind(item.id.as_uuid())
        .execute(db.app_pool())
        .await
        .expect_err("raw write without actor must fail");
    assert_eq!(pg_code(&werr), "42501");

    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("proj tx");
    verify_projection(&mut tx)
        .await
        .expect("live projection equals fold");
    rebuild(&mut tx).await.expect("rebuild");
    verify_projection(&mut tx)
        .await
        .expect("rebuilt projection equals fold");
    tx.commit().await.expect("commit proj");

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn modules_2s1_compose_plain_shop() {
    let db = db_case!("2s1_plain");
    compose_once(db, Profile::plain_shop().unwrap(), "plain-shop").await;
}

#[tokio::test]
async fn modules_2s1_compose_regulated_device() {
    let db = db_case!("2s1_reg");
    let profile = Profile::regulated_device().unwrap();
    migrate_and_install(&db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), profile.clone());
    register_wave_2s1(&mut builder, &profile);
    let kernel = builder.build().await.expect("build for edge list");
    assert!(
        kernel.profile.required_edges().iter().any(|e| matches!(
            e,
            SignatureEdge::Required { module, edge, .. }
                if module == LOT_DOC && edge == "release"
        )),
        "regulated profile lists lot.release Required"
    );
    db.finish().await.expect("finish edge probe");

    let db = db_case!("2s1_reg2");
    compose_once(db, profile, "regulated-device").await;
}
