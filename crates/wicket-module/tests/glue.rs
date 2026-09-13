//! ADDENDUM 1 named tests (composition glue).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use rust_decimal::Decimal;
use serde_json::json;
use sqlx::query_scalar;
use wicket_core::{
    ActorKind, AnyQuantity, Boundary, ConversionContext, CostElement, CurrencyId, DimensionKind,
    GroupKind, Identifier, ItemId, LengthDim, LocationId, LotId, Money, PostingGroupHeader,
    PostingIntent, PostingSink, Quantity, QuantityPosting, Rounding, SignatureError, SignatureId,
    SignatureMeaning, SignatureToken, UnitId, UnitRef, ValueAccount, ValuePosting,
};
use wicket_db::Tx;
use wicket_esign::identity_projection;
use wicket_ledger::{CostMethod, upsert_location, upsert_stock_item};
use wicket_statemachine::{DocRef, EdgeBuilder, Machine, Veto};
use wicket_test::db_case;

use wicket_module::{Kernel, ModuleManifest, Profile};

use common::{actor_with_perm, migrate_and_install};

fn dummy_token(doc: &DocRef, version: i64, meaning: &str) -> SignatureToken {
    SignatureToken {
        signature: SignatureId::generate(),
        signer: wicket_core::Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        meaning: SignatureMeaning(meaning.into()),
        record: wicket_core::RecordRef {
            table: "sm.instance".into(),
            id: doc.doc_id,
            version,
        },
        record_content_hash: [0; 32],
    }
}

fn qty_ea(n: i64) -> AnyQuantity {
    AnyQuantity {
        amount: Decimal::from(n),
        unit: UnitId(1),
        dimension: DimensionKind::Count,
    }
}

fn usd(n: i64) -> Money {
    Money::new(Decimal::from(n), CurrencyId(840)).expect("usd")
}

fn q_post(
    item: ItemId,
    qty: AnyQuantity,
    location: LocationId,
    boundary: Option<Boundary>,
) -> QuantityPosting {
    QuantityPosting {
        item,
        quantity: qty,
        location,
        boundary,
        lot: None,
        serial: None,
        entered: None,
    }
}

fn contribute_receipt(
    sink: &mut dyn PostingSink,
    item: ItemId,
    stock: LocationId,
    supplier: LocationId,
) -> core::result::Result<(), Veto> {
    let veto = |e: wicket_core::PostingError| Veto {
        module: "mod-production-min".into(),
        reason: e.to_string(),
    };
    let recv = sink
        .contribute(PostingIntent::Quantity(q_post(
            item,
            qty_ea(1),
            stock,
            None,
        )))
        .map_err(veto)?;
    sink.contribute(PostingIntent::Quantity(q_post(
        item,
        qty_ea(-1),
        supplier,
        Some(Boundary::Supplier),
    )))
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

#[tokio::test]
async fn machine_registered_through_kernel_is_transitionable_after_build() {
    let db = db_case!("g_mach");
    migrate_and_install(&db).await;
    let extra = Machine::builder("extra.doc")
        .regulated(false)
        .state("A")
        .state("B")
        .edge(EdgeBuilder::new("A", "B", "go", "wo.release"))
        .build()
        .unwrap();
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    builder.register_machine(extra).unwrap();
    builder.register_projection("extra.doc", identity_projection);
    let mut kernel = builder.build().await.expect("build");
    let write = kernel.write_pool();
    let doc = DocRef {
        doc_type: "extra.doc".into(),
        doc_id: Identifier::generate(),
    };
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "go").await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin spawn");
    kernel.spawn(&mut tx, &doc, "A").await.expect("spawn");
    tx.commit().await.expect("commit spawn");

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin tr");
    let inst = kernel
        .transition(&mut tx, &doc, "go", None, &ctx)
        .await
        .expect("transition after build");
    tx.commit().await.expect("commit tr");
    assert_eq!(inst.state.0, "B");

    let late = Machine::builder("late.doc")
        .edge(EdgeBuilder::new("X", "Y", "n", "wo.release"))
        .build()
        .unwrap();
    let err = kernel.engine.register_machine(late).expect_err("frozen");
    assert!(
        matches!(err, wicket_statemachine::Error::Frozen),
        "got {err:?}"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn gate_wrapped_transitions_under_both_profiles() {
    for (label, profile, doc_type, initial, edge, meaning, perm, expect_required) in [
        (
            "regulated-device",
            Profile::regulated_device().unwrap(),
            "calibration.certificate",
            "Open",
            "approve",
            "Approved",
            "calibration.approve",
            true,
        ),
        (
            "plain-shop",
            Profile::plain_shop().unwrap(),
            "wo",
            "Draft",
            "release",
            "Released",
            "wo.release",
            false,
        ),
    ] {
        let db = db_case!(&format!("g_gt{label:.4}"));
        migrate_and_install(&db).await;
        let kernel = Kernel::build(db.app_pool(), profile).await.expect(label);
        let stored = wicket_module::export_manifest(db.app_pool())
            .await
            .expect("manifest");
        if expect_required {
            assert!(
                kernel.profile.required_edges().iter().any(|e| matches!(
                    e,
                    wicket_module::SignatureEdge::Required { edge: name, .. } if name == edge
                )),
                "{label} must list Required {edge}"
            );
            assert!(
                stored.signature_edges.iter().any(|e| e.is_required()),
                "{label} manifest lists Required"
            );
        } else {
            assert!(
                kernel.profile.required_edges().is_empty(),
                "{label} Required set empty"
            );
        }

        let write = kernel.write_pool();
        let doc = DocRef {
            doc_type: doc_type.into(),
            doc_id: Identifier::generate(),
        };
        let (_, ctx) = actor_with_perm(&write, perm, &doc, edge).await;
        let mut tx = Tx::begin(&write, &ctx).await.expect("spawn");
        kernel.spawn(&mut tx, &doc, initial).await.expect("spawn");
        tx.commit().await.expect("commit spawn");

        let token = dummy_token(&doc, 1, meaning);
        let mut tx = Tx::begin(&write, &ctx).await.expect("tr");
        let result = kernel
            .transition(&mut tx, &doc, edge, Some(&token), &ctx)
            .await;
        if expect_required {
            let err = result.expect_err("Required edge must refuse a dummy token");
            assert!(
                matches!(
                    err,
                    wicket_module::Error::Statemachine(wicket_statemachine::Error::Signature(
                        SignatureError::Invalid(_)
                    ))
                ),
                "{label} typed error, got {err:?}"
            );
            tx.rollback().await.ok();
        } else {
            result.expect("NotRequired runs under NoSignatures");
            tx.commit().await.expect("commit");
        }
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn hook_postings_reach_the_ledger() {
    let db = db_case!("g_post");
    migrate_and_install(&db).await;
    let item = ItemId::generate();
    let stock = LocationId::generate();
    let supplier = LocationId::generate();
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    builder.register_hook("mod-production-min", "wo", "release", move |_v, sink| {
        contribute_receipt(sink, item, stock, supplier)
    });
    let kernel = builder.build().await.expect("build");
    let write = kernel.write_pool();
    let doc = DocRef {
        doc_type: "wo".into(),
        doc_id: Identifier::generate(),
    };
    let (actor, _) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    let mut ctx = kernel.transition_context(actor, &doc, "release");
    ctx.actor_display = Some("Operator".into());
    ctx.reason = Some("module-glue-test".into());

    let mut tx = Tx::begin(&write, &common_boot()).await.expect("reg");
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
    .expect("stock");
    upsert_location(&mut tx, stock, None)
        .await
        .expect("stock loc");
    upsert_location(&mut tx, supplier, Some(Boundary::Supplier))
        .await
        .expect("supplier");
    tx.commit().await.expect("commit reg");

    let mut tx = Tx::begin(&write, &ctx).await.expect("spawn");
    kernel.spawn(&mut tx, &doc, "Draft").await.expect("spawn");
    tx.commit().await.expect("commit spawn");

    let mut tx = Tx::begin(&write, &ctx).await.expect("tr");
    kernel
        .transition(&mut tx, &doc, "release", None, &ctx)
        .await
        .expect("posted transition");
    tx.commit().await.expect("commit tr");

    let qty: Decimal = query_scalar(
        "SELECT COALESCE(SUM(quantity), 0) FROM ledger.posting WHERE measure = 'QUANTITY'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("sum qty");
    assert_eq!(qty, Decimal::ZERO, "P1 conserved per group");
    let groups: i64 = query_scalar("SELECT count(*) FROM ledger.posting_group")
        .fetch_one(db.app_pool())
        .await
        .expect("groups");
    assert_eq!(groups, 1);

    let inst_upd: i64 = query_scalar(
        "SELECT count(*) FROM audit.event
          WHERE actor_id = $1 AND table_name = 'instance' AND op = 'UPDATE'",
    )
    .bind(actor.id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("inst audit");
    assert_eq!(inst_upd, 1, "one instance UPDATE audit row");
    let grp_ins: i64 = query_scalar(
        "SELECT count(*) FROM audit.event
          WHERE actor_id = $1 AND table_name = 'posting_group' AND op = 'INSERT'",
    )
    .bind(actor.id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("grp audit");
    assert_eq!(grp_ins, 1);
    let post_ins: i64 = query_scalar(
        "SELECT count(*) FROM audit.event
          WHERE actor_id = $1 AND table_name = 'posting' AND op = 'INSERT'",
    )
    .bind(actor.id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("post audit");
    assert_eq!(post_ins, 4, "one audit row per posting insert");
    let receipt_cfg: String = query_scalar(
        "SELECT config_version FROM audit.event
          WHERE actor_id = $1 AND table_name = 'posting_group' AND op = 'INSERT'",
    )
    .bind(actor.id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("receipt config_version");
    assert!(
        !receipt_cfg.is_empty(),
        "glue receipt config_version must be non-empty"
    );
    assert_eq!(
        receipt_cfg, kernel.profile.spec_version,
        "glue receipt config_version equals the profile spec"
    );
    let foreign: i64 = query_scalar(
        "SELECT count(*) FROM audit.event
          WHERE table_name IN ('instance', 'posting_group', 'posting')
            AND actor_id <> $1
            AND action LIKE 'wo.%'",
    )
    .bind(actor.id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("foreign");
    assert_eq!(
        foreign, 0,
        "mutating steps attributed to the acting identity"
    );

    db.finish().await.expect("finish");

    let db = db_case!("g_nopost");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("build empty");
    let write = kernel.write_pool();
    let doc = DocRef {
        doc_type: "wo".into(),
        doc_id: Identifier::generate(),
    };
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("spawn");
    kernel.spawn(&mut tx, &doc, "Draft").await.expect("spawn");
    tx.commit().await.expect("commit spawn");
    let mut tx = Tx::begin(&write, &ctx).await.expect("tr");
    kernel
        .transition(&mut tx, &doc, "release", None, &ctx)
        .await
        .expect("empty hook");
    tx.commit().await.expect("commit");
    let groups: i64 = query_scalar("SELECT count(*) FROM ledger.posting_group")
        .fetch_one(db.app_pool())
        .await
        .expect("groups");
    assert_eq!(
        groups, 0,
        "hook that finalizes nothing leaves no ledger rows"
    );
    db.finish().await.expect("finish");

    let db = db_case!("g_poison");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("build poison");
    let write = kernel.write_pool();
    let mut tx = Tx::begin(&write, &common_boot()).await.expect("begin");
    let mut sink = kernel.posting_sink(
        GroupKind::Movement,
        PostingGroupHeader {
            source_kind: "poison".into(),
            source_id: None,
            work_order_id: None,
            reason_code: None,
            reverses_group_id: None,
        },
    );
    kernel.bind_sink(&mut tx, &mut sink).await.expect("bind");
    sink.contribute(PostingIntent::Quantity(q_post(
        ItemId::generate(),
        qty_ea(1),
        LocationId::generate(),
        None,
    )))
    .expect("contribute");
    drop(sink);
    let err = wicket_ledger::commit(tx, &[])
        .await
        .expect_err("unfinalized poisons");
    assert!(
        matches!(err, wicket_ledger::Error::Unfinalized),
        "got {err:?}"
    );
    db.finish().await.expect("finish");
}

fn common_boot() -> wicket_db::WriteContext {
    let mut ctx = wicket_db::WriteContext::new(
        wicket_core::Actor {
            id: Identifier::from_uuid(wicket_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "module.boot",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("module-glue-test".into());
    ctx.config_version = Some(
        Profile::plain_shop()
            .expect("plain-shop")
            .spec_version
            .clone(),
    );
    ctx
}

#[tokio::test]
async fn units_in_the_same_transaction() {
    let db = db_case!("g_uom");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("build");
    let write = kernel.write_pool();
    let doc = DocRef {
        doc_type: "wo".into(),
        doc_id: Identifier::generate(),
    };
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    let item = ItemId::generate();
    let lot = LotId::generate();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.item_stock (item_id, stock_unit_id, stock_scale)
             VALUES ($1, $2, $3)",
        )
        .bind(item.as_uuid())
        .bind(4_i64)
        .bind(4_i16),
    )
    .await
    .expect("item_stock");
    wicket_uom::pin_lot_factor(
        &mut tx,
        item,
        lot,
        UnitId(3),
        UnitId(4),
        Decimal::ONE,
        Decimal::from(8),
    )
    .await
    .expect("pin");
    let catalog = wicket_uom::load_catalog(&mut tx).await.expect("catalog");
    let inch = UnitRef::<LengthDim>::checked(UnitId(3), DimensionKind::Length).unwrap();
    let foot = UnitRef::<LengthDim>::checked(UnitId(4), DimensionKind::Length).unwrap();
    let lot_ctx = ConversionContext {
        item,
        lot: Some(lot),
    };
    let entered = AnyQuantity::from(Quantity::new(Decimal::from(8), inch).unwrap());
    let conv = kernel
        .to_stock::<LengthDim>(&mut tx, &catalog, item, entered, &lot_ctx)
        .await
        .expect("to_stock honours pin in this Tx");
    assert_eq!(conv.canonical.amount(), Decimal::ONE);
    assert_eq!(conv.factor, Decimal::new(125, 3));
    let qty = Quantity::new(Decimal::from(8), inch).unwrap();
    let converted = kernel
        .convert(&catalog, qty, foot, &lot_ctx)
        .expect("convert");
    let (canonical, residual) = converted.split(Rounding::HalfEven);
    assert_eq!(canonical.amount(), Decimal::ONE);
    let _ = residual;
    kernel.spawn(&mut tx, &doc, "Draft").await.expect("spawn");
    kernel
        .transition(&mut tx, &doc, "release", None, &ctx)
        .await
        .expect("same Tx transition");
    tx.commit().await.expect("commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn events_and_jobs_are_live() {
    let db = db_case!("g_evt");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("build");
    let write = kernel.write_pool();
    let doc = DocRef {
        doc_type: "wo".into(),
        doc_id: Identifier::generate(),
    };
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    kernel.spawn(&mut tx, &doc, "Draft").await.expect("spawn");
    kernel
        .transition(&mut tx, &doc, "release", None, &ctx)
        .await
        .expect("hook path");
    let event = wicket_events::Event::builder()
        .name("inventory.lot_received")
        .version(1)
        .payload(json!({
            "item_id": Identifier::generate().as_uuid().to_string(),
            "lot_id": Identifier::generate().as_uuid().to_string(),
        }))
        .build_with(&kernel.event_schemas)
        .expect("event");
    kernel
        .publish_event(&mut tx, event)
        .await
        .expect("emit from the transition Tx (hook path)");
    tx.commit().await.expect("commit");

    let n = kernel
        .dispatch_tick(Kernel::service_actor())
        .await
        .expect("dispatch");
    assert!(n >= 1, "subscribed handler ran, got {n}");
    let jobs: i64 = query_scalar("SELECT count(*) FROM transient.job WHERE kind = $1")
        .bind(wicket_jobs::events::bridge_job_kind())
        .fetch_one(db.app_pool())
        .await
        .expect("jobs");
    assert_eq!(jobs, 1);

    let ran = kernel
        .worker_tick(Kernel::service_actor())
        .await
        .expect("worker");
    assert_eq!(ran, 1);
    let done: i64 =
        query_scalar("SELECT count(*) FROM transient.job WHERE kind = $1 AND state = 'succeeded'")
            .bind(wicket_jobs::events::bridge_job_kind())
            .fetch_one(db.app_pool())
            .await
            .expect("succeeded");
    assert_eq!(done, 1);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn registries_populated_from_manifests() {
    let fixture = r#"
[module]
id = "mod-glue-test"
version = "0.1.0"
name = "Glue test"
description = "Exercises every extension point"

[dependencies]
kernel = "^0.1"

[permissions]
"glue.view" = "View glue"
"glue.advance" = "Advance glue"

[capabilities]
requires-signature = []
regulated = false

[[machines]]
doc_type = "glue.doc"
regulated = false
states = ["Start", "Done"]

[[machines.edges]]
from = "Start"
to = "Done"
name = "advance"
permission = "glue.advance"

[[subscriptions]]
event = "inventory.lot_received"
subscriber = "mod-glue-test"

[[routes]]
path = "/api/v1/glue"
permission = "glue.view"

[[jobs]]
kind = "glue.tick"
"#;
    let manifest = ModuleManifest::parse(fixture).expect("fixture");
    assert!(!manifest.permissions.is_empty());
    assert_eq!(manifest.machines.len(), 1);
    assert_eq!(manifest.subscriptions.len(), 1);
    assert_eq!(manifest.routes.len(), 1);
    assert_eq!(manifest.jobs.len(), 1);

    let db = db_case!("g_reg");
    migrate_and_install(&db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::regulated_device().unwrap());
    builder.apply_manifest(&manifest).unwrap();
    builder.register_projection("glue.doc", identity_projection);
    let kernel = builder.build().await.expect("build");

    assert!(
        kernel
            .engine
            .edges_for_manifest()
            .iter()
            .any(|e| e.doc_type == "glue.doc" && e.edge == "advance"),
        "machine from fixture manifest"
    );
    assert!(
        kernel
            .engine
            .edges_for_manifest()
            .iter()
            .any(|e| e.doc_type == "calibration.certificate" && e.edge == "approve"),
        "machine from compiled-in manifest, not a kernel constant"
    );
    assert!(
        kernel
            .routes
            .iter()
            .any(|r| r.path == "/api/v1/glue" && r.permission == "glue.view")
    );
    assert!(
        kernel
            .routes
            .iter()
            .any(|r| r.module_id == "mod-calibration" && r.path == "/api/v1/calibration")
    );
    assert!(
        kernel
            .subscriptions
            .iter()
            .any(|s| s.event == "inventory.lot_received" && s.subscriber == "wicket-jobs")
    );
    assert!(
        kernel
            .subscriptions
            .iter()
            .any(|s| s.subscriber == "mod-glue-test")
    );
    assert!(
        kernel
            .job_kinds
            .iter()
            .any(|j| j.kind == "genealogy.refresh")
    );
    assert!(kernel.job_kinds.iter().any(|j| j.kind == "glue.tick"));
    assert!(
        kernel
            .catalog()
            .iter()
            .any(|m| m.permissions.contains_key("calibration.approve"))
    );
    db.finish().await.expect("finish");
}
