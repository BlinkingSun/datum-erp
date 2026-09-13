//! Named commit-mode tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::collections::BTreeSet;

use datum_core::{
    Actor, ActorKind, GroupKind, Identifier, NoSignatures, PermissionKey, PostingGroupHeader,
    RecordRef, SignatureError, SignatureGate, SignatureId, SignatureMeaning, SignatureRequirement,
    SignatureToken,
};
use datum_db::{Tx, WriteContext};
use datum_events::EventHandler;
use datum_statemachine::{DocRef, EdgeBuilder, Engine, HookPhase, Machine, ModuleNode};
use datum_test::db_case;

use datum_module::{
    CANONICAL_ORDER, CONTRACT_KERNEL_EDGES, CONTRACT_SLICE_EDGES, ConfigurationManifest,
    DELTA_ALLOWED, GateBinding, KERNEL_AUDIT_RELS, KERNEL_ORDER, Kernel, Profile, ProfileId,
    SLICE_AUDIT_RELS, SignatureEdge, bind_signature_gate, compiled_in, delta_keys, disable,
    edges_from_registry, enable, export_manifest, install, is_topological_sort, list_installed,
    load_kernel_defaults, module_nodes, posting_sink, profile_does_not_rewrite_edges,
    startup_fails_if_required_meets_no_signatures, topological_order, verify,
};

use common::{
    actor_with_perms, has_zz_audit, migrate_and_install, migrate_and_install_slice, module_enabled,
    module_hash, pg_code, table_count, toy_manifest, toy_manifest_regulated, write_pool,
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
    assert!(KERNEL_ORDER.contains(&"datum-esign"));
    assert!(KERNEL_ORDER.contains(&"datum-customfields"));
    assert!(KERNEL_ORDER.contains(&"datum-documents"));
    assert!(KERNEL_ORDER.contains(&"datum-print"));
    let pos = |name: &str| KERNEL_ORDER.iter().position(|n| *n == name).unwrap();
    assert!(pos("datum-esign") < pos("datum-documents"));
    assert!(pos("datum-customfields") < pos("datum-documents"));
    assert!(pos("datum-numbering") < pos("datum-documents"));
    assert!(pos("datum-statemachine") < pos("datum-documents"));
    assert!(pos("datum-documents") < pos("datum-print"));
    assert_eq!(*KERNEL_ORDER.last().unwrap(), "datum-module");
}

#[test]
fn canonical_order_is_a_topological_sort_of_contract_graph() {
    assert_eq!(&CANONICAL_ORDER[..KERNEL_ORDER.len()], KERNEL_ORDER);
    assert!(is_topological_sort(CANONICAL_ORDER, CONTRACT_KERNEL_EDGES));
    assert!(is_topological_sort(CANONICAL_ORDER, CONTRACT_SLICE_EDGES));
    let pos = |name: &str| CANONICAL_ORDER.iter().position(|n| *n == name).unwrap();
    assert_eq!(CANONICAL_ORDER[0], "datum-db");
    assert_eq!(*CANONICAL_ORDER.last().unwrap(), "datum-server");
    assert!(pos("datum-module") < pos("datum-mod-items"));
    assert!(pos("datum-mod-items") < pos("datum-mod-lots"));
    assert!(pos("datum-mod-locations") < pos("datum-mod-lots"));
    assert!(pos("datum-mod-lots") < pos("datum-mod-inventory"));
    assert!(pos("datum-mod-inventory") < pos("datum-mod-production-min"));
    assert!(pos("datum-mod-genealogy") < pos("datum-server"));
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
    let calibration = compiled_in()
        .unwrap()
        .into_iter()
        .find(|m| m.id == "mod-calibration")
        .unwrap();
    assert!(calibration.regulated);
    assert!(
        a.modules
            .iter()
            .any(|m| m.id == "mod-calibration" && m.enabled)
    );
    assert!(
        b.modules
            .iter()
            .any(|m| m.id == "mod-calibration" && !m.enabled)
    );
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
fn required_edge_permission_is_a_set_for_shared_meaning() {
    let profile = Profile::plain_shop().unwrap().with_registry_edges(vec![
        SignatureEdge::Required {
            module: "document".into(),
            edge: "approve".into(),
            meaning: "Approved".into(),
            permission: "documents.approve".into(),
        },
        SignatureEdge::Required {
            module: "document".into(),
            edge: "other".into(),
            meaning: "Approved".into(),
            permission: "documents.other".into(),
        },
        SignatureEdge::Required {
            module: "calibration.certificate".into(),
            edge: "approve".into(),
            meaning: "Approved".into(),
            permission: "calibration.approve".into(),
        },
    ]);
    assert_eq!(
        profile.required_edge_permission("document", "Approved"),
        vec![
            "documents.approve".to_string(),
            "documents.other".to_string()
        ]
    );
    assert_eq!(
        profile.required_edge_permission("calibration.certificate", "Approved"),
        vec!["calibration.approve".to_string()]
    );
    assert!(
        profile
            .required_edge_permission("document", "Released")
            .is_empty()
    );
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
fn regulated_device_startup_fails_if_required_meets_no_signatures() {
    let req = SignatureRequirement {
        meaning: SignatureMeaning("Approved".into()),
        permission: PermissionKey("calibration.approve".into()),
    };
    let m = Machine::builder("calibration.certificate")
        .regulated(true)
        .edge(EdgeBuilder::new("Open", "Approved", "approve", "calibration.approve").required(req))
        .build()
        .unwrap();
    let mut eng = Engine::new();
    eng.register_machine(m).unwrap();
    let err = startup_fails_if_required_meets_no_signatures(&eng, true, true)
        .expect_err("release + NoSignatures + Required");
    assert!(err.to_string().contains("startup"), "got {err}");
    startup_fails_if_required_meets_no_signatures(&eng, true, false).unwrap();
    startup_fails_if_required_meets_no_signatures(&eng, false, true).unwrap();
    let profile = Profile::regulated_device().unwrap();
    assert_eq!(
        profile.signature_gate_binding,
        GateBinding::DatumEsign,
        "regulated-device binds datum-esign so a live release boot is not this failure"
    );
    assert_eq!(profile.session_policy.continuous_session, "off");
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
async fn seed_rows_are_audited() {
    for (label, profile) in [
        ("plain-shop", Profile::plain_shop().unwrap()),
        ("regulated-device", Profile::regulated_device().unwrap()),
    ] {
        let db = db_case!(&format!("seed_{}", &label[..5]));
        migrate_and_install(&db).await;
        let kernel = Kernel::build(db.app_pool(), profile).await.expect(label);
        let _ = kernel;

        let missing_principal: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM identity.principal p
             WHERE NOT EXISTS (
               SELECT 1 FROM audit.event e
                WHERE e.schema_name = 'identity'
                  AND e.table_name = 'principal'
                  AND e.op = 'INSERT'
                  AND e.row_key ->> 'id' = p.id::text
             )
            "#,
        )
        .fetch_one(db.app_pool())
        .await
        .expect("principal audit");
        assert_eq!(
            missing_principal, 0,
            "{label}: every identity.principal seed row has an INSERT audit row"
        );

        let missing_history: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM (
              SELECT h.id FROM identity.username_history h
               WHERE NOT EXISTS (
                 SELECT 1 FROM audit.event e
                  WHERE e.schema_name = 'identity'
                    AND e.table_name = 'username_history'
                    AND e.op = 'INSERT'
                    AND e.row_key ->> 'id' = h.id::text
               )
              UNION ALL
              SELECT h.id FROM identity.display_name_history h
               WHERE NOT EXISTS (
                 SELECT 1 FROM audit.event e
                  WHERE e.schema_name = 'identity'
                    AND e.table_name = 'display_name_history'
                    AND e.op = 'INSERT'
                    AND e.row_key ->> 'id' = h.id::text
               )
            ) t
            "#,
        )
        .fetch_one(db.app_pool())
        .await
        .expect("history audit");
        assert_eq!(
            missing_history, 0,
            "{label}: principal history seed rows have INSERT audit rows"
        );
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn audit_trigger_matrix() {
    for (label, profile) in [
        ("plain-shop", Profile::plain_shop().unwrap()),
        ("regulated-device", Profile::regulated_device().unwrap()),
    ] {
        let db = db_case!(&format!("atm_{}", &label[..5]));
        migrate_and_install_slice(&db).await;
        let kernel = Kernel::build(db.app_pool(), profile).await.expect(label);
        let _ = kernel;

        let discovered: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT n.nspname::text || '.' || c.relname::text
              FROM pg_class c
              JOIN pg_namespace n ON n.oid = c.relnamespace
              JOIN datum.schema_class sc ON sc.nspname = n.nspname
             WHERE sc.class = 'app'
               AND c.relkind IN ('r', 'p')
               AND NOT c.relispartition
               AND c.relname <> 'schema_class'
               AND NOT EXISTS (
                 SELECT 1 FROM audit.exempt e
                  WHERE e.relid = c.oid
                     OR (e.nspname = n.nspname AND e.relname = c.relname)
               )
             ORDER BY 1
            "#,
        )
        .fetch_all(db.migrate_pool())
        .await
        .expect("catalog app-class tables");

        let listed: BTreeSet<&str> = KERNEL_AUDIT_RELS
            .iter()
            .chain(SLICE_AUDIT_RELS.iter())
            .copied()
            .collect();
        let found: BTreeSet<&str> = discovered.iter().map(String::as_str).collect();
        assert_eq!(
            found, listed,
            "{label}: catalog app-class tables must equal KERNEL_AUDIT_RELS ∪ SLICE_AUDIT_RELS"
        );

        for rel in listed {
            let (schema, table) = rel.split_once('.').expect("schema.table");
            assert!(
                has_zz_audit(db.migrate_pool(), schema, table).await,
                "{label}: listed {rel} missing zz_audit_row"
            );
        }

        let slice_rels: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT n.nspname::text || '.' || c.relname::text
              FROM pg_class c
              JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname IN (
                     'inventory', 'production_min', 'server',
                     'items', 'locations', 'lots'
                   )
               AND c.relkind IN ('r', 'p')
               AND NOT c.relispartition
             ORDER BY 1
            "#,
        )
        .fetch_all(db.migrate_pool())
        .await
        .expect("slice-schema tables");
        assert!(
            !slice_rels.is_empty(),
            "{label}: slice schemas must exist after install_slice"
        );
        for rel in &slice_rels {
            let (schema, table) = rel.split_once('.').expect("schema.table");
            assert!(
                has_zz_audit(db.migrate_pool(), schema, table).await,
                "{label}: {rel} missing zz_audit_row"
            );
        }

        for rel in &discovered {
            let (schema, table) = rel.split_once('.').expect("schema.table");
            assert!(
                has_zz_audit(db.migrate_pool(), schema, table).await,
                "{label}: app table {rel} missing zz_audit_row"
            );
        }
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn documents_schema_installed_with_kernel() {
    let db = db_case!("doc_kern");
    migrate_and_install(&db).await;
    let n: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'documents'
              AND c.relkind = 'r'
              AND c.relname IN ('document','revision','blob','attachment','link')"#,
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("documents tables");
    assert_eq!(n, 5, "install_kernel must apply datum-documents");
    let class: String =
        sqlx::query_scalar("SELECT class FROM datum.schema_class WHERE nspname = 'documents'")
            .fetch_one(db.migrate_pool())
            .await
            .expect("schema_class");
    assert_eq!(class, "app");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn documents_events_emitted_from_composition_root() {
    let db = db_case!("doc_evt");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("plain-shop kernel");
    assert!(
        kernel
            .event_schemas
            .get(datum_documents::EVENT_REVISION_CREATED, 1)
            .is_some(),
        "revision_created schema registered"
    );
    assert!(
        kernel
            .event_schemas
            .get(datum_documents::EVENT_EFFECTIVE, 1)
            .is_some(),
        "effective schema registered"
    );

    let write = kernel.write_pool();
    let actor = actor_with_perms(
        &write,
        &[
            "documents.view",
            "documents.edit",
            "documents.approve",
            "documents.release",
        ],
    )
    .await;
    let mut create_ctx = boot_ctx();
    create_ctx.actor = actor;
    create_ctx.actor_display = Some("Operator".into());
    let mut tx = Tx::begin(&write, &create_ctx).await.expect("create begin");
    let id = kernel
        .create_document(&mut tx, "SOP", "Glue event", "quality")
        .await
        .expect("create");
    let rev = kernel
        .new_document_revision(
            &mut tx,
            id,
            "A",
            datum_documents::Manifest::content(serde_json::json!({})),
        )
        .await
        .expect("revision");
    tx.commit().await.expect("create commit");

    let created: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.event WHERE name = $1 AND doc_id = $2")
            .bind(datum_documents::EVENT_REVISION_CREATED)
            .bind(id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("revision_created count");
    assert_eq!(created, 1, "composition root publishes revision_created");
    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM app.event WHERE name = $1 AND doc_id = $2")
            .bind(datum_documents::EVENT_REVISION_CREATED)
            .bind(id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("revision_created payload");
    assert_eq!(payload["revision_id"], rev.as_uuid().to_string());
    assert_eq!(payload["label"], "A");

    let doc = DocRef {
        doc_type: datum_documents::DOC_TYPE.into(),
        doc_id: id.0,
    };
    for edge in ["submit", "approve", "make_effective"] {
        let mut ctx = kernel.transition_context(actor, &doc, edge);
        ctx.actor_display = Some("Operator".into());
        ctx.reason = Some("documents-glue".into());
        let mut tx = Tx::begin(&write, &ctx).await.expect("edge begin");
        kernel
            .transition(&mut tx, &doc, edge, None, &ctx)
            .await
            .unwrap_or_else(|e| panic!("{edge}: {e:#}"));
        tx.commit().await.expect("edge commit");
    }

    let effective: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.event WHERE name = $1 AND doc_id = $2")
            .bind(datum_documents::EVENT_EFFECTIVE)
            .bind(id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("effective count");
    assert_eq!(
        effective, 1,
        "composition root publishes documents.effective"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn install_runs_migrations_in_one_transaction_and_records() {
    let db = db_case!("mod_inst");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let a = toy_manifest("toy-a", &[]).with_migrations(vec![
        "INSERT INTO module.configuration (id, profile_id, spec_version, body, content_hash) VALUES ('mig:toy-a', 'test', '0', '{}'::jsonb, '00')",
    ]);
    let b = toy_manifest("toy-b", &[("toy-a", "^0.1")]).with_migrations(vec!["SELECT 1 / 0"]);
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    install(&mut tx, &a, true).await.expect("install a");
    tx.commit().await.expect("commit a");
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin b");
    let fail = install(&mut tx, &b, true)
        .await
        .expect_err("failing migration");
    assert!(
        fail.to_string().contains("division") || fail.to_string().contains("db"),
        "got {fail}"
    );
    tx.rollback().await.expect("rollback b");
    assert_eq!(module_enabled(db.app_pool(), "toy-a").await, Some(true));
    assert_eq!(module_enabled(db.app_pool(), "toy-b").await, None);
    let marker: i64 = table_count(
        db.app_pool(),
        "SELECT count(*) FROM module.configuration WHERE id = 'mig:toy-a'",
    )
    .await;
    assert_eq!(marker, 1, "successful module migration must persist");
    let fail_marker: i64 = table_count(
        db.app_pool(),
        "SELECT count(*) FROM module.installed WHERE id = 'toy-b'",
    )
    .await;
    assert_eq!(fail_marker, 0, "failing migration must persist nothing");
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
    enable(
        &mut tx,
        "mod-genealogy",
        &catalog,
        &Profile::plain_shop().unwrap(),
    )
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

fn sample_token() -> (SignatureToken, SignatureRequirement, RecordRef) {
    let record = RecordRef {
        table: "calibration.certificate".into(),
        id: Identifier::generate(),
        version: 1,
    };
    let token = SignatureToken {
        signature: SignatureId::generate(),
        signer: Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        meaning: SignatureMeaning("Approved".into()),
        record: record.clone(),
        record_content_hash: [0; 32],
    };
    let required = SignatureRequirement {
        meaning: SignatureMeaning("Approved".into()),
        permission: PermissionKey("calibration.approve".into()),
    };
    (token, required, record)
}

struct DummySubscriber;

impl EventHandler for DummySubscriber {
    fn handle<'a, 'p: 'a>(
        &'a self,
        _tx: &'a mut Tx<'p>,
        _event: &'a datum_events::Event,
    ) -> datum_events::HandlerFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn signature_gate_comes_from_profile_toml_gate_field() {
    let regulated = Profile::regulated_device().unwrap();
    let plain = Profile::plain_shop().unwrap();
    assert_eq!(regulated.signature_gate_binding, GateBinding::DatumEsign);
    assert_eq!(plain.signature_gate_binding, GateBinding::NoSignatures);
    assert_eq!(regulated.session_policy.continuous_session, "off");
    assert_eq!(plain.session_policy.continuous_session, "off");
    let _esign = bind_signature_gate(regulated.signature_gate_binding);
    let _noop = bind_signature_gate(plain.signature_gate_binding);
    let (token, required, record) = sample_token();
    assert!(
        matches!(
            NoSignatures.verify(&token, &required, &record),
            Err(SignatureError::NoProvider)
        ),
        "NoSignatures factory named by SPEC-profiles key 4 / CONTRACT §6.3"
    );
}

#[tokio::test]
async fn signature_gate_returns_the_bound_gate() {
    let db_plain = db_case!("mod_sgp");
    migrate_and_install(&db_plain).await;
    let (token, required, record) = sample_token();

    let plain = Kernel::build(db_plain.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("plain");
    assert!(
        matches!(
            plain.signature_gate().verify(&token, &required, &record),
            Err(SignatureError::NoProvider)
        ),
        "plain-shop signature_gate is NoSignatures"
    );
    db_plain.finish().await.expect("plain finish");

    let db = db_case!("mod_sgr");
    migrate_and_install(&db).await;
    let regulated = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("regulated");
    let err = regulated
        .signature_gate()
        .verify(&token, &required, &record)
        .expect_err("dummy token on bound esign");
    assert!(
        matches!(err, SignatureError::Invalid(_)),
        "regulated signature_gate is the esign binding, got {err:?}"
    );
    assert_ne!(
        err,
        SignatureError::NoProvider,
        "bound esign must not look like NoSignatures"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn regulated_required_set_from_registered_machine() {
    let db = db_case!("mod_reqd");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("build regulated");
    assert_eq!(
        kernel.profile.signature_gate_binding,
        GateBinding::DatumEsign,
        "gate field comes from the profile TOML"
    );
    assert!(
        !kernel.profile.required_edges().is_empty(),
        "regulated-device must resolve a non-empty Required set"
    );
    assert!(
        kernel.profile.signature_edges.iter().any(
            |e| matches!(e, datum_module::SignatureEdge::Required { edge, .. } if edge == "approve")
        ),
        "calibration.certificate.approve must be listed"
    );
    assert!(
        !kernel.gate_is_noop(),
        "regulated-device binds the datum-esign factory"
    );
    startup_fails_if_required_meets_no_signatures(&kernel.engine, true, true)
        .expect_err("§6.3 startup guard must trip on the live Required set");
    startup_fails_if_required_meets_no_signatures(&kernel.engine, false, true)
        .expect("esign-bound release boot is allowed");
    let stored = export_manifest(db.app_pool()).await.expect("export");
    stored.verify_self().expect("hashed");
    assert!(
        stored.signature_edges.iter().any(|e| e.is_required()),
        "manifest must list Required edges"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn module_manifest_custom_fields_registered() {
    let catalog = compiled_in().unwrap();
    let cal = catalog
        .iter()
        .find(|m| m.id == "mod-calibration")
        .expect("calibration");
    assert!(
        cal.custom_fields
            .fields
            .iter()
            .any(|f| f.key == "udi_device_identifier"),
        "compiled-in calibration manifest declares [[custom-fields]]"
    );

    let db = db_case!("mod_cf");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("build regulated");
    let write = kernel.write_pool();
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    let defs = datum_customfields::definitions_for(&mut tx, "items.item")
        .await
        .expect("definitions_for");
    assert!(
        defs.iter().any(|d| d.key == "udi_device_identifier"
            && d.entity == "items.item"
            && d.owner_module == "mod-calibration"),
        "install graph registered manifest custom fields: {defs:?}"
    );
    tx.commit().await.expect("commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn enable_module_the_profile_disallows_is_refused() {
    let catalog = compiled_in().unwrap();

    let db = db_case!("mod_enpl");
    migrate_and_install(&db).await;
    Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("plain boot");
    let write = write_pool(&db);
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    let err = enable(
        &mut tx,
        "mod-calibration",
        &catalog,
        &Profile::plain_shop().unwrap(),
    )
    .await
    .expect_err("plain-shop runtime enable");
    assert!(
        matches!(err, datum_module::Error::EnableRefused { .. }),
        "typed error, got {err}"
    );
    tx.rollback().await.ok();
    assert_eq!(
        module_enabled(db.app_pool(), "mod-calibration").await,
        Some(false)
    );
    db.finish().await.expect("finish");

    let db = db_case!("mod_enrg");
    migrate_and_install(&db).await;
    Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("regulated boot");
    let extra = toy_manifest_regulated("mod-not-listed");
    let mut catalog = compiled_in().unwrap();
    catalog.push(extra);
    let write = write_pool(&db);
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    let err = enable(
        &mut tx,
        "mod-not-listed",
        &catalog,
        &Profile::regulated_device().unwrap(),
    )
    .await
    .expect_err("regulated refuses a module the profile does not list");
    assert!(
        matches!(err, datum_module::Error::EnableRefused { .. }),
        "typed error, got {err}"
    );
    tx.rollback().await.ok();
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn profile_key10_defaults_persist_and_read_back() {
    let db = db_case!("mod_k10");
    migrate_and_install(&db).await;
    let profile = Profile::regulated_device().unwrap();
    Kernel::build(db.app_pool(), profile.clone())
        .await
        .expect("build");
    let loaded = load_kernel_defaults(db.app_pool()).await.expect("load");
    assert_eq!(
        loaded.base_currency,
        profile.seeded_permissions.base_currency
    );
    assert_eq!(
        loaded.stock_uom_system,
        profile.seeded_permissions.stock_uom_system
    );
    assert_eq!(
        loaded.display_timezone,
        profile.seeded_permissions.display_timezone
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn configuration_manifest_refuses_before_edges_or_completes_hashed() {
    let db = db_case!("mod_exp");
    migrate_and_install(&db).await;
    let err = export_manifest(db.app_pool())
        .await
        .expect_err("edges not generated");
    assert!(
        err.to_string().contains("no effective-profile"),
        "got {err}"
    );
    let kernel = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("build");
    let stored = export_manifest(db.app_pool()).await.expect("export");
    stored.verify_self().expect("hash");
    assert_eq!(stored.content_hash.len(), 64);
    assert!(stored.modules.len() >= 7);
    assert!(
        stored.signature_edges.iter().any(|e| e.is_required()),
        "complete: Required edges present after generate"
    );
    let live = ConfigurationManifest::assemble(
        kernel.profile.id.as_str(),
        &kernel.profile.spec_version,
        stored.modules.clone(),
        kernel.profile.signature_edges.clone(),
    )
    .expect("assemble");
    verify(&stored, &live).expect("verify");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn register_events_subscription_hook_accepts_dummy_subscriber() {
    let db = db_case!("mod_evts");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("build");
    let write = write_pool(&db);
    let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("begin");
    kernel
        .register_events_subscription(
            &mut tx,
            "inventory.lot_received",
            "dummy-subscriber",
            DummySubscriber,
        )
        .await
        .expect("register");
    tx.commit().await.expect("commit");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.subscription WHERE subscriber = 'dummy-subscriber'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("count");
    assert_eq!(n, 1);
    db.finish().await.expect("finish");
}

const SEEDED_PRINT_TEMPLATES: [&str; 3] = [
    datum_print::TemplateId::DOCUMENT_REVISION,
    datum_print::TemplateId::GENERIC_RECORD,
    datum_print::TemplateId::WORK_ORDER_TRAVELER,
];

async fn print_install_profile(db: &datum_test::TestDb) -> String {
    sqlx::query_scalar("SELECT profile_id FROM print.install WHERE singleton = 'x'")
        .fetch_one(db.app_pool())
        .await
        .expect("print.install")
}

async fn print_template_rows(db: &datum_test::TestDb) -> Vec<(String, i32, Vec<u8>)> {
    sqlx::query_as(
        "SELECT template_id, version, body_hash FROM print.template ORDER BY template_id, version",
    )
    .fetch_all(db.app_pool())
    .await
    .expect("print.template")
}

fn assert_seeded_print_templates(label: &str, rows: &[(String, i32, Vec<u8>)]) {
    assert_eq!(
        rows.len(),
        3,
        "{label}: three built-in templates, got {rows:?}"
    );
    let ids: Vec<&str> = rows.iter().map(|(id, _, _)| id.as_str()).collect();
    for expected in SEEDED_PRINT_TEMPLATES {
        assert!(
            ids.contains(&expected),
            "{label}: missing template {expected} in {ids:?}"
        );
    }
    assert!(
        !ids.contains(&"item_label"),
        "{label}: item_label is ADR 0009, not a Wave 3b template"
    );
    for (id, version, _) in rows {
        assert_eq!(*version, 1, "{label}: {id} must stay at seeded version 1");
    }
}

#[tokio::test]
async fn kernel_build_seeds_print_templates() {
    for (label, profile) in [
        ("plain-shop", Profile::plain_shop().unwrap()),
        ("regulated-device", Profile::regulated_device().unwrap()),
    ] {
        let db = db_case!(&format!("kprt_{}", &label[..5]));
        migrate_and_install(&db).await;
        let kernel = Kernel::build(db.app_pool(), profile.clone())
            .await
            .expect(label);
        assert_eq!(kernel.profile_id(), profile.id);

        let stamped = print_install_profile(&db).await;
        assert_eq!(
            stamped,
            profile.id.as_str(),
            "{label}: 11.50(b) stamps profile.id, not spec_version"
        );
        assert_ne!(
            stamped, profile.spec_version,
            "{label}: spec_version {} must not be the install stamp",
            profile.spec_version
        );

        let rows = print_template_rows(&db).await;
        assert_seeded_print_templates(label, &rows);

        let retire = kernel
            .engine
            .edges_for_manifest()
            .into_iter()
            .find(|e| e.doc_type == datum_customfields::DOC_TYPE && e.edge == "retire");
        assert!(
            retire.is_some(),
            "{label}: definition_machine retire edge is on the frozen engine"
        );
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn kernel_build_seeds_print_templates_is_idempotent_on_restart() {
    for (label, profile) in [
        ("plain-shop", Profile::plain_shop().unwrap()),
        ("regulated-device", Profile::regulated_device().unwrap()),
    ] {
        let db = db_case!(&format!("kprtr_{}", &label[..5]));
        migrate_and_install(&db).await;
        Kernel::build(db.app_pool(), profile.clone())
            .await
            .expect(label);
        let first_stamp = print_install_profile(&db).await;
        let first_rows = print_template_rows(&db).await;
        assert_eq!(first_stamp, profile.id.as_str(), "{label}");
        assert_seeded_print_templates(label, &first_rows);

        Kernel::build(db.app_pool(), profile.clone())
            .await
            .unwrap_or_else(|e| panic!("{label} restart Kernel::build: {e:#}"));
        let second_stamp = print_install_profile(&db).await;
        let second_rows = print_template_rows(&db).await;
        assert_eq!(
            second_stamp, first_stamp,
            "{label}: restart restamps the same profile.id"
        );
        assert_eq!(
            second_rows, first_rows,
            "{label}: restart must not insert a second template version"
        );
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn kernel_assemble_registers_definition_machine() {
    for (label, profile) in [
        ("plain-shop", Profile::plain_shop().unwrap()),
        ("regulated-device", Profile::regulated_device().unwrap()),
    ] {
        let db = db_case!(&format!("kdef_{}", &label[..5]));
        migrate_and_install(&db).await;
        let kernel = Kernel::build(db.app_pool(), profile.clone())
            .await
            .expect(label);

        let edge = kernel
            .engine
            .edges_for_manifest()
            .into_iter()
            .find(|e| {
                e.doc_type == datum_customfields::DOC_TYPE
                    && e.edge == datum_customfields::RETIRE_EDGE
            })
            .unwrap_or_else(|| panic!("{label}: retire edge missing"));
        assert_eq!(
            edge.kind, "not_required",
            "{label}: retire is configuration"
        );
        assert!(
            kernel.profile.signature_edges.iter().any(|e| matches!(
                e,
                datum_module::SignatureEdge::NotRequired { module, edge, .. }
                    if module == datum_customfields::DOC_TYPE
                        && edge == datum_customfields::RETIRE_EDGE
            )),
            "{label}: generated key 3 lists definition retire as NotRequired"
        );

        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM sm.machine WHERE doc_type = $1")
                .bind(datum_customfields::DOC_TYPE)
                .fetch_one(db.app_pool())
                .await
                .expect("sm.machine");
        assert_eq!(persisted, 1, "{label}: definition_machine persisted");

        let write = kernel.write_pool();
        let actor = actor_with_perms(&write, &[datum_customfields::RETIRE_PERMISSION]).await;
        let mut define_ctx = boot_ctx();
        define_ctx.actor = actor;
        define_ctx.actor_display = Some("Operator".into());
        let mut tx = Tx::begin(&write, &define_ctx).await.expect("define begin");
        let id = datum_customfields::define(
            &mut tx,
            datum_customfields::DefinitionSpec {
                entity: "items.item".into(),
                key: format!("w3b_{}", &label[..5]),
                field_type: datum_customfields::FieldType::String,
                label: "Wave 3b".into(),
                validation_rule: String::new(),
                required: false,
                indexed: false,
                owner_module: "mod-items".into(),
            },
        )
        .await
        .expect("define");
        tx.commit().await.expect("define commit");

        let mut retire_ctx = kernel.transition_context(
            actor,
            &datum_customfields::doc_ref(id),
            datum_customfields::RETIRE_EDGE,
        );
        retire_ctx.actor_display = Some("Operator".into());
        retire_ctx.reason = Some("w3b-compose".into());
        let mut tx = Tx::begin(&write, &retire_ctx).await.expect("retire begin");
        datum_customfields::retire(&mut tx, &kernel.engine, id, &retire_ctx)
            .await
            .unwrap_or_else(|e| panic!("{label} retire through kernel engine: {e:#}"));
        tx.commit().await.expect("retire commit");

        let state: String =
            sqlx::query_scalar("SELECT state FROM sm.instance WHERE doc_type = $1 AND doc_id = $2")
                .bind(datum_customfields::DOC_TYPE)
                .bind(id.as_uuid())
                .fetch_one(db.app_pool())
                .await
                .expect("instance");
        assert_eq!(
            state, "retired",
            "{label}: retire transitioned on the kernel"
        );
        db.finish().await.expect("finish");
    }
}
