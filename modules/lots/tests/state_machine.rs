//! Lot state machine: every manifest edge, illegal jumps, and signature profiles.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::{PermissionKey, SignatureMeaning, SignatureRequirement};
use datum_db::Tx;
use datum_mod_lots::{
    CreateLot, DOC_TYPE, Error, Lot, LotStatus, StatusTarget, create_lot, load_lot, manifest,
    register_schemas, set_status,
};
use datum_module::{Kernel, Profile};
use datum_statemachine::{EdgeBuilder, Engine, Machine};

use crate::common::{
    actor_with_lots_perms, boot_kernel, edge_ctx, sm_instance_count, write_ctx, write_pool,
};

async fn lot_in_quarantine(db: &datum_test::TestDb, kernel: &Kernel) -> Lot {
    let write = write_pool(db);
    let ctx = write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        kernel,
        &ctx,
        CreateLot {
            item: datum_core::ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    lot
}

async fn transition(
    db: &datum_test::TestDb,
    kernel: &Kernel,
    lot: Lot,
    edge: &str,
    to: LotStatus,
) -> Lot {
    let write = write_pool(db);
    let actor = actor_with_lots_perms(&write).await;
    let ctx = edge_ctx(kernel, actor, lot.id, edge);
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    set_status(
        &mut tx,
        kernel,
        actor,
        StatusTarget::Lot(lot.id),
        to,
        "test transition",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    load_lot(
        &mut Tx::begin(&write, &write_ctx("lots.view")).await.unwrap(),
        lot.id,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn lot_spawn_on_create_registers_sm_instance() {
    let db = datum_test::db_case!("sm_spawn");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    assert_eq!(
        sm_instance_count(db.app_pool(), lot.id).await,
        1,
        "spawn must persist sm.instance"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_edge_release_quarantine_to_available() {
    let db = datum_test::db_case!("sm_release");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    let out = transition(&db, &kernel, lot, "release", LotStatus::Available).await;
    assert_eq!(out.status, LotStatus::Available);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_edge_hold_available_to_hold() {
    let db = datum_test::db_case!("sm_hold");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    let lot = transition(&db, &kernel, lot, "release", LotStatus::Available).await;
    let out = transition(&db, &kernel, lot, "hold", LotStatus::Hold).await;
    assert_eq!(out.status, LotStatus::Hold);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_edge_unhold_hold_to_available() {
    let db = datum_test::db_case!("sm_unhold");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    let lot = transition(&db, &kernel, lot, "release", LotStatus::Available).await;
    let lot = transition(&db, &kernel, lot, "hold", LotStatus::Hold).await;
    let out = transition(&db, &kernel, lot, "unhold", LotStatus::Available).await;
    assert_eq!(out.status, LotStatus::Available);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_edge_reject_from_quarantine() {
    let db = datum_test::db_case!("sm_rej_q");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    let out = transition(
        &db,
        &kernel,
        lot,
        "reject_from_quarantine",
        LotStatus::Rejected,
    )
    .await;
    assert_eq!(out.status, LotStatus::Rejected);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_edge_reject_from_available() {
    let db = datum_test::db_case!("sm_rej_a");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    let lot = transition(&db, &kernel, lot, "release", LotStatus::Available).await;
    let out = transition(&db, &kernel, lot, "reject", LotStatus::Rejected).await;
    assert_eq!(out.status, LotStatus::Rejected);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_edge_reject_from_hold() {
    let db = datum_test::db_case!("sm_rej_h");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    let lot = transition(&db, &kernel, lot, "release", LotStatus::Available).await;
    let lot = transition(&db, &kernel, lot, "hold", LotStatus::Hold).await;
    let out = transition(&db, &kernel, lot, "reject_from_hold", LotStatus::Rejected).await;
    assert_eq!(out.status, LotStatus::Rejected);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_illegal_jump_quarantine_to_hold_is_refused() {
    let db = datum_test::db_case!("sm_bad_jump");
    let kernel = boot_kernel(&db).await;
    let lot = lot_in_quarantine(&db, &kernel).await;
    let write = write_pool(&db);
    let actor = actor_with_lots_perms(&write).await;
    let ctx = edge_ctx(&kernel, actor, lot.id, "hold");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let err = set_status(
        &mut tx,
        &kernel,
        actor,
        StatusTarget::Lot(lot.id),
        LotStatus::Hold,
        "illegal",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, Error::InvalidTransition { .. })
            || matches!(err, Error::Module(datum_module::Error::Statemachine(_))),
        "got {err:?}"
    );
    tx.rollback().await.unwrap();
    let still = load_lot(
        &mut Tx::begin(&write, &write_ctx("lots.view")).await.unwrap(),
        lot.id,
    )
    .await
    .unwrap();
    assert_eq!(still.status, LotStatus::Quarantine);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_release_plain_profile_not_required_under_no_signatures() {
    let db = datum_test::db_case!("sm_plain_rel");
    let kernel = boot_kernel(&db).await;
    assert!(
        kernel.profile.required_edges().is_empty(),
        "plain profile: release must be NotRequired"
    );
    let lot = lot_in_quarantine(&db, &kernel).await;
    let _ = transition(&db, &kernel, lot, "release", LotStatus::Available).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lot_release_regulated_profile_refuses_under_no_signatures() {
    let db = datum_test::db_case!("sm_reg_rel");
    common::migrate_kernel(&db).await;
    let req = SignatureRequirement {
        meaning: SignatureMeaning("Released".into()),
        permission: PermissionKey("lots.release".into()),
    };
    let machine = Machine::builder(DOC_TYPE)
        .regulated(true)
        .state("quarantine")
        .state("available")
        .state("hold")
        .state("rejected")
        .edge(EdgeBuilder::new("quarantine", "available", "release", "lots.release").required(req))
        .edge(EdgeBuilder::new("available", "hold", "hold", "lots.edit").not_required("lots test"))
        .edge(
            EdgeBuilder::new("hold", "available", "unhold", "lots.edit").not_required("lots test"),
        )
        .edge(
            EdgeBuilder::new(
                "quarantine",
                "rejected",
                "reject_from_quarantine",
                "lots.edit",
            )
            .not_required("lots test"),
        )
        .edge(
            EdgeBuilder::new("available", "rejected", "reject", "lots.edit")
                .not_required("lots test"),
        )
        .edge(
            EdgeBuilder::new("hold", "rejected", "reject_from_hold", "lots.edit")
                .not_required("lots test"),
        )
        .build()
        .unwrap();
    let mut eng = Engine::new();
    eng.register_machine(machine).unwrap();
    eng.freeze().unwrap();
    let profile = Profile::regulated_device()
        .unwrap()
        .with_registry_edges(datum_module::edges_from_registry(&eng));
    assert!(
        profile.required_edges().iter().any(|e| matches!(
            e,
            datum_module::SignatureEdge::Required { edge, .. } if edge == "release"
        )),
        "regulated profile lists Required release"
    );
    register_schemas().expect("schemas");
    let mut builder = Kernel::builder(db.app_pool().clone(), profile);
    builder
        .register_machine(
            Machine::builder(DOC_TYPE)
                .regulated(true)
                .state("quarantine")
                .state("available")
                .state("hold")
                .state("rejected")
                .edge(
                    EdgeBuilder::new("quarantine", "available", "release", "lots.release")
                        .required(SignatureRequirement {
                            meaning: SignatureMeaning("Released".into()),
                            permission: PermissionKey("lots.release".into()),
                        }),
                )
                .edge(
                    EdgeBuilder::new("available", "hold", "hold", "lots.edit")
                        .not_required("lots test"),
                )
                .edge(
                    EdgeBuilder::new("hold", "available", "unhold", "lots.edit")
                        .not_required("lots test"),
                )
                .edge(
                    EdgeBuilder::new(
                        "quarantine",
                        "rejected",
                        "reject_from_quarantine",
                        "lots.edit",
                    )
                    .not_required("lots test"),
                )
                .edge(
                    EdgeBuilder::new("available", "rejected", "reject", "lots.edit")
                        .not_required("lots test"),
                )
                .edge(
                    EdgeBuilder::new("hold", "rejected", "reject_from_hold", "lots.edit")
                        .not_required("lots test"),
                )
                .build()
                .unwrap(),
        )
        .expect("register machine");
    let kernel = builder.build().await.expect("build");
    let write = write_pool(&db);
    let ctx = write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: datum_core::ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let write = write_pool(&db);
    let actor = actor_with_lots_perms(&write).await;
    let ctx = edge_ctx(&kernel, actor, lot.id, "release");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let before_hist: (i64,) =
        sqlx::query_as("SELECT count(*) FROM lots.status_history WHERE lot_id = $1")
            .bind(lot.id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .unwrap();
    let err = set_status(
        &mut tx,
        &kernel,
        actor,
        StatusTarget::Lot(lot.id),
        LotStatus::Available,
        "release",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Module(datum_module::Error::Statemachine(
                datum_statemachine::Error::Signature(datum_core::SignatureError::NoProvider)
            ))
        ) || matches!(
            err,
            Error::Module(datum_module::Error::Statemachine(
                datum_statemachine::Error::Signature(datum_core::SignatureError::Invalid(_))
            ))
        ),
        "typed signature refusal under NoSignatures, got {err:?}"
    );
    tx.rollback().await.unwrap();
    let after_hist: (i64,) =
        sqlx::query_as("SELECT count(*) FROM lots.status_history WHERE lot_id = $1")
            .bind(lot.id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .unwrap();
    assert_eq!(before_hist.0, after_hist.0, "no history row on refusal");
    let still = load_lot(
        &mut Tx::begin(&write, &write_ctx("lots.view")).await.unwrap(),
        lot.id,
    )
    .await
    .unwrap();
    assert_eq!(still.status, LotStatus::Quarantine);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn http_routes_declare_method_level_permissions() {
    let m = manifest().expect("manifest");
    assert!(m.permissions.contains_key("lots.view"));
    assert!(m.permissions.contains_key("lots.edit"));
    assert!(m.permissions.contains_key("lots.release"));
    let toml = std::fs::read_to_string(format!(
        "{}/module.toml",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
    .expect("module.toml");
    assert!(toml.contains("method = \"GET\"") && toml.contains("permission = \"lots.view\""));
    assert!(toml.contains("method = \"POST\"") && toml.contains("permission = \"lots.edit\""));
    let release_edge = m
        .machines
        .iter()
        .flat_map(|mach| &mach.edges)
        .find(|e| e.name == "release")
        .expect("release edge");
    assert_eq!(release_edge.permission, "lots.release");
}
