//! Named commit-mode tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::{
    Actor, ActorKind, GroupKind, Identifier, PermissionKey, PostingGroupHeader, SignatureMeaning,
    SignatureRequirement,
};
use datum_db::{Tx, WriteContext};
use datum_statemachine::{EdgeBuilder, Engine, HookPhase, Machine, ModuleNode};
use datum_test::db_case;

use datum_module::{
    CONTRACT_KERNEL_EDGES, ConfigurationManifest, DELTA_ALLOWED, KERNEL_ORDER, Kernel, Profile,
    ProfileId, compiled_in, delta_keys, disable, edges_from_registry, enable, export_manifest,
    install, is_topological_sort, list_installed, module_nodes, posting_sink,
    profile_does_not_rewrite_edges, startup_fails_if_required_meets_no_signatures,
    topological_order, verify,
};

use common::{
    has_zz_audit, migrate_and_install, module_enabled, module_hash, pg_code, table_count,
    toy_manifest, write_pool,
};

fn boot_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "module.boot",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("module-test".into());
    ctx
}

#[test]
fn kernel_order_is_a_topological_sort_of_contract_graph() {
    assert!(is_topological_sort(KERNEL_ORDER, CONTRACT_KERNEL_EDGES));
    assert_eq!(KERNEL_ORDER[0], "datum-db");
    assert_eq!(KERNEL_ORDER[1], "datum-audit");
    assert_eq!(*KERNEL_ORDER.last().unwrap(), "datum-statemachine");
}

#[test]
fn both_profiles_carry_eleven_keys_and_load() {
    let a = Profile::regulated_device().expect("regulated");
    let b = Profile::plain_shop().expect("plain");
    assert_eq!(a.id, ProfileId::RegulatedDevice);
    assert_eq!(b.id, ProfileId::PlainShop);
    assert_eq!(a.spec_version, b.spec_version);
    assert!(a.modules.iter().all(|m| m.installed));
    assert!(b.modules.iter().all(|m| m.installed));
    assert!(!b.modules.iter().any(|m| m.regulated && m.enabled));
    let genealogy = compiled_in()
        .unwrap()
        .into_iter()
        .find(|m| m.id == "mod-genealogy")
        .unwrap();
    assert!(!genealogy.regulated);
}

#[test]
fn profiles_delta_is_subset_of_allowed_keys() {
    let a = Profile::regulated_device().unwrap();
    let b = Profile::plain_shop().unwrap();
    assert!(profile_does_not_rewrite_edges(&a, &b));
    let da = a.effective_dump().unwrap();
    let db = b.effective_dump().unwrap();
    let delta = delta_keys(&da, &db);
    for key in &delta {
        assert!(
            DELTA_ALLOWED.contains(&key.as_str()),
            "unexpected delta key {key}"
        );
    }
    assert!(
        delta.iter().any(|k| DELTA_ALLOWED.contains(&k.as_str())),
        "profiles should differ on an allowed key, got {delta:?}"
    );
}

#[test]
fn plain_shop_required_signature_set_is_empty() {
    let plain = Profile::plain_shop().unwrap();
    assert!(plain.required_edges().is_empty());
    let mut eng = Engine::new();
    eng.freeze().unwrap();
    let live = Profile::plain_shop()
        .unwrap()
        .with_registry_edges(edges_from_registry(&eng));
    assert!(live.required_edges().is_empty());
}

#[test]
fn signature_edges_come_from_registry_not_toml() {
    let decoy = r#"
signature_edges = ["this-must-not-appear"]
kernel_always_on = ["audit_trigger"]
[profile]
id = "plain-shop"
display_name = "Plain shop"
spec_version = "1.0.0"
[[modules]]
id = "mod-items"
version = "0.1.0"
regulated = false
installed = true
enabled = true
[signature_gate_binding]
gate = "NoSignatures"
[validation_manifest]
generated = true
route = "/api/v1/iq/manifest"
cli = "datum iq"
permission = "validation.manifest.read"
audit_export_route = "/api/v1/audit"
audit_export_cli = "datum audit export"
hidden_by_profile = false
[navigation]
visible = ["items"]
hidden = ["validation", "iq"]
[numbering.wo]
format = "WO-{0000}"
prefix = "WO"
scope = "document"
gap_free = true
reset = "never"
[anchor_sink]
per_instance = true
iq_suite = "per-instance"
[seeded_permissions]
base_currency = "USD"
stock_uom_system = "SI"
display_timezone = "UTC"
[acceptance]
wave2s_items_that_differ = [7, 11]
"#;
    let parsed = Profile::parse(decoy).expect("profile with decoy array");
    assert!(
        parsed.signature_edges.is_empty(),
        "TOML values must be ignored"
    );
    let req = SignatureRequirement {
        meaning: SignatureMeaning("Released".into()),
        permission: PermissionKey("wo.release".into()),
    };
    let m = Machine::builder("wo")
        .regulated(true)
        .edge(EdgeBuilder::new("Draft", "Released", "release", "wo.release").required(req))
        .build()
        .unwrap();
    let mut eng = Engine::new();
    eng.register_machine(m).unwrap();
    let live = parsed.with_registry_edges(edges_from_registry(&eng));
    assert_eq!(live.signature_edges.len(), 1);
    assert_eq!(live.required_edges().len(), 1);
    assert!(!format!("{:?}", live.signature_edges).contains("this-must-not-appear"));
}

#[test]
fn startup_fails_release_required_edge_with_no_signatures() {
    let req = SignatureRequirement {
        meaning: SignatureMeaning("Released".into()),
        permission: PermissionKey("wo.release".into()),
    };
    let m = Machine::builder("wo")
        .regulated(true)
        .edge(EdgeBuilder::new("Draft", "Released", "release", "wo.release").required(req))
        .build()
        .unwrap();
    let mut eng = Engine::new();
    eng.register_machine(m).unwrap();
    let err = startup_fails_if_required_meets_no_signatures(&eng, true, true)
        .expect_err("release + NoSignatures + Required");
    assert!(err.to_string().contains("startup"), "got {err}");
    startup_fails_if_required_meets_no_signatures(&eng, true, false).unwrap();
    startup_fails_if_required_meets_no_signatures(&eng, false, true).unwrap();
}

#[test]
fn hook_order_matches_statemachine_hook_order() {
    let nodes = vec![
        ModuleNode {
            id: "mod-a".into(),
            depends_on: vec![],
        },
        ModuleNode {
            id: "mod-c".into(),
            depends_on: vec!["mod-a".into()],
        },
        ModuleNode {
            id: "mod-b".into(),
            depends_on: vec!["mod-a".into()],
        },
        ModuleNode {
            id: "mod-d".into(),
            depends_on: vec!["mod-b".into(), "mod-c".into()],
        },
    ];
    let ours = topological_order(
        &nodes
            .iter()
            .map(|n| datum_module::ModuleNode {
                id: n.id.clone(),
                depends_on: n.depends_on.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut eng = Engine::new();
    eng.set_module_graph(nodes).unwrap();
    let m = Machine::builder("wo")
        .edge(EdgeBuilder::new(
            "Draft",
            "Released",
            "release",
            "wo.release",
        ))
        .build()
        .unwrap();
    eng.register_machine(m).unwrap();
    for id in ["mod-d", "mod-c", "mod-a", "mod-b"] {
        eng.register_hook(id, "wo", "release", HookPhase::Before, 50, |_v, _s| Ok(()))
            .unwrap();
    }
    eng.freeze().unwrap();
    assert_eq!(eng.hook_order("wo", "release"), ours);
    assert_eq!(
        ours,
        vec![
            "mod-a".to_string(),
            "mod-b".to_string(),
            "mod-c".to_string(),
            "mod-d".to_string()
        ]
    );
    let _ = module_nodes().unwrap();
    let _ = posting_sink(
        GroupKind::Adjustment,
        PostingGroupHeader {
            source_kind: "test".into(),
            source_id: None,
            work_order_id: None,
            reason_code: None,
            reverses_group_id: None,
        },
    );
}

#[tokio::test]
async fn schema_history_trigger_is_present() {
    let db = db_case!("mod_hist");
    migrate_and_install(&db).await;
    assert!(
        has_zz_audit(db.migrate_pool(), "datum", "schema_history").await,
        "datum.schema_history must be audited after attach"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn install_runs_migrations_in_one_transaction_and_records() {
    let db = db_case!("mod_inst");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let a = toy_manifest("toy-a", &[]);
    let b = toy_manifest("toy-b", &[("toy-a", "^0.1")]);
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    install(&mut tx, &a, true).await.expect("install a");
    tx.commit().await.expect("commit a");
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin b");
    install(&mut tx, &b, true).await.expect("install b");
    tx.rollback().await.expect("rollback b");
    assert_eq!(module_enabled(db.app_pool(), "toy-a").await, Some(true));
    assert_eq!(module_enabled(db.app_pool(), "toy-b").await, None);
    let log: i64 = table_count(
        db.app_pool(),
        "SELECT count(*) FROM module.install_log WHERE module_id = 'toy-a'",
    )
    .await;
    assert_eq!(log, 1);
    let log_b: i64 = table_count(
        db.app_pool(),
        "SELECT count(*) FROM module.install_log WHERE module_id = 'toy-b'",
    )
    .await;
    assert_eq!(log_b, 0);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn enable_closes_over_dependencies() {
    let db = db_case!("mod_encl");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let catalog = compiled_in().unwrap();
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    for m in &catalog {
        install(&mut tx, m, false).await.expect("install");
    }
    enable(&mut tx, "mod-genealogy", &catalog)
        .await
        .expect("enable genealogy");
    tx.commit().await.expect("commit");
    for id in [
        "mod-genealogy",
        "mod-production-min",
        "mod-inventory",
        "mod-items",
        "mod-locations",
        "mod-lots",
    ] {
        assert_eq!(module_enabled(db.app_pool(), id).await, Some(true), "{id}");
    }
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn disable_depended_on_module_is_refused_naming_dependents() {
    let db = db_case!("mod_disr");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let catalog = compiled_in().unwrap();
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    for m in &catalog {
        install(&mut tx, m, true).await.expect("install");
    }
    let err = disable(&mut tx, "mod-items", &catalog)
        .await
        .expect_err("must refuse");
    tx.rollback().await.ok();
    let msg = err.to_string();
    assert!(msg.contains("mod-items"), "{msg}");
    assert!(
        msg.contains("mod-inventory"),
        "dependents must be named: {msg}"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn disable_never_drops_tables() {
    let db = db_case!("mod_drop");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let catalog = compiled_in().unwrap();
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    for m in &catalog {
        install(&mut tx, m, true).await.expect("install");
    }
    disable(&mut tx, "mod-genealogy", &catalog)
        .await
        .expect("leaf disable");
    tx.commit().await.expect("commit");
    assert_eq!(
        module_enabled(db.app_pool(), "mod-genealogy").await,
        Some(false)
    );
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'module'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("tables");
    assert!(tables >= 3, "disable must not drop module.* tables");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn manifest_hash_changes_when_enabled_set_changes() {
    let db = db_case!("mod_hash");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let catalog = compiled_in().unwrap();
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    for m in &catalog {
        install(&mut tx, m, true).await.expect("install");
    }
    tx.commit().await.expect("commit");
    let before = module_hash(db.app_pool(), "mod-genealogy")
        .await
        .expect("hash");
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin2");
    disable(&mut tx, "mod-genealogy", &catalog)
        .await
        .expect("disable");
    tx.commit().await.expect("commit2");
    let after = module_hash(db.app_pool(), "mod-genealogy")
        .await
        .expect("hash2");
    assert_ne!(before, after);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn configuration_manifest_round_trips_and_verifies() {
    let db = db_case!("mod_cfg");
    migrate_and_install(&db).await;
    let profile = Profile::plain_shop().unwrap();
    let kernel = Kernel::build(db.app_pool(), profile).await.expect("build");
    let stored = export_manifest(db.app_pool()).await.expect("export");
    stored.verify_self().expect("self");
    let live = ConfigurationManifest::assemble(
        kernel.profile.id.as_str(),
        &kernel.profile.spec_version,
        stored.modules.clone(),
        kernel.profile.signature_edges.clone(),
    )
    .expect("assemble");
    verify(&stored, &live).expect("verify");
    assert_eq!(
        stored
            .kernel_order
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        KERNEL_ORDER
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("mod_raw");
    migrate_and_install(&db).await;
    let before: i64 = table_count(db.app_pool(), "SELECT count(*) FROM module.installed").await;
    let err = sqlx::query(
        r#"INSERT INTO module.installed
               (id, version, regulated, enabled, manifest_hash)
           VALUES ('raw-write', '0.0.1', false, true, '00')"#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after: i64 = table_count(db.app_pool(), "SELECT count(*) FROM module.installed").await;
    assert_eq!(before, after, "table must be unchanged");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn list_installed_after_kernel_build() {
    let db = db_case!("mod_list");
    migrate_and_install(&db).await;
    Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("build");
    let write = write_pool(&db);
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    let rows = list_installed(&mut tx).await.expect("list");
    tx.commit().await.expect("commit");
    assert!(rows.len() >= 6);
    db.finish().await.expect("finish");
}
