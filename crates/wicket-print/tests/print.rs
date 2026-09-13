#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_docs
)]

mod common;

use serde_json::json;
use sqlx::query;
use wicket_core::{Identifier, RecordRef};
use wicket_db::Tx;
use wicket_documents::{BlobStore, Manifest, attach, create, new_revision};
use wicket_esign::{MintRequest, SessionPolicy, mint};
use wicket_identity::{
    PrincipalKind, create_principal,
    rbac::{assign_role, seed_bundles},
    set_login_credential, set_signing_credential,
};
use wicket_print::{
    Format, TemplateId, archive, bump_template, list_templates, list_templates_on, log, render,
};

use common::{
    PROFILES, SPEC_VERSION, blob_store, frozen_engine, migrate, open_db, persist_engine, read_pool,
    write_ctx, write_pool,
};

async fn for_each_profile<F, Fut>(f: F)
where
    F: Fn(&'static str) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for profile in PROFILES {
        f(profile).await;
    }
}

fn record(table: &str, id: Identifier, version: i64) -> RecordRef {
    RecordRef {
        table: table.into(),
        id,
        version,
    }
}

async fn mint_for_record(write: &wicket_db::WritePool, rec: RecordRef, body: serde_json::Value) {
    let mut tx = Tx::begin(write, &write_ctx("esign.mint")).await.unwrap();
    let p = create_principal(&mut tx, PrincipalKind::User, "signer1", "M. Reyes")
        .await
        .unwrap();
    set_login_credential(&mut tx, p.id, "login-secret-ok")
        .await
        .unwrap();
    set_signing_credential(&mut tx, p.id, "signing-secret-ok")
        .await
        .unwrap();
    let roles = seed_bundles(
        &mut tx,
        &[wicket_identity::rbac::RoleBundle {
            name: "print-signer".into(),
            permissions: vec!["documents.approve".into()],
        }],
    )
    .await
    .unwrap();
    assign_role(&mut tx, p.id, roles[0]).await.unwrap();
    tx.commit().await.unwrap();
    let mut tx = Tx::begin(write, &write_ctx("esign.mint")).await.unwrap();
    let req = MintRequest {
        components: vec!["code".into(), "secret".into()],
        code: Some("signer1".into()),
        secret: "signing-secret-ok".into(),
        meaning: wicket_core::SignatureMeaning("Approved".into()),
        reason: None,
        record: rec.clone(),
        doc_type: "test.doc".into(),
        projection: body,
        instance: wicket_esign::InstanceTriple {
            doc_type: "test.doc".into(),
            doc_id: rec.id,
            state: "Effective".into(),
            version: rec.version,
        },
        permission: wicket_core::PermissionKey("documents.approve".into()),
        signed_at_zone: "America/New_York".into(),
        policy: SessionPolicy::default(),
        principal: p,
        login_session_id: None,
        device_fingerprint: Some("dev1".into()),
        source_ip: Some("127.0.0.1".into()),
        boot_epoch: "1".into(),
        credential_kind: "signing_password".into(),
    };
    mint(&mut tx, &req).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn render_is_deterministic_html() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("det_html", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let rec = record("generic.record", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let a = render(
            &mut tx,
            rec.clone(),
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        let b = render(
            &mut tx,
            rec.clone(),
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(a.output_hash, b.output_hash);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn render_is_deterministic_pdf() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("det_pdf", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let rec = record("generic.record", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let a = render(
            &mut tx,
            rec.clone(),
            Format::Pdf,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        let b = render(
            &mut tx,
            rec.clone(),
            Format::Pdf,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(a.bytes, b.bytes);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn template_change_bumps_version_and_hash() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("tpl_bump", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let rec = record("generic.record", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let first = render(
            &mut tx,
            rec.clone(),
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        bump_template(
            &mut tx,
            TemplateId::GENERIC_RECORD,
            2,
            "1.1.0",
            "<!DOCTYPE html><html><body><p>changed</p>{{MANIFESTATION}}<footer>{{FOOTER}}</footer></body></html>",
        )
        .await
        .unwrap();
        let second = render(
            &mut tx,
            rec.clone(),
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(second.template_version, 2);
        assert_ne!(first.output_hash, second.output_hash);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn manifestation_block_inline_regulated() {
    let profile = "regulated-device";
    let Some(db) = open_db("manif_inl", profile).await else {
        return;
    };
    migrate(&db, profile).await;
    let write = write_pool(&db);
    let rec = record("generic.record", Identifier::generate(), 3);
    mint_for_record(&write, rec.clone(), json!({"k": 1})).await;
    let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
    assert_eq!(
        tx.setting("wicket.config_version").await.unwrap(),
        SPEC_VERSION
    );
    let out = render(
        &mut tx,
        rec.clone(),
        Format::Html,
        TemplateId::new(TemplateId::GENERIC_RECORD),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let html = String::from_utf8(out.bytes).unwrap();
    assert!(html.contains("M. Reyes"));
    assert!(html.contains("Approved"));
    assert!(html.contains("America/New_York"));
    assert!(html.contains("Z "));
    assert!(html.contains("hash "));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn unsigned_regulated_record_marked_unsigned() {
    let profile = "regulated-device";
    let Some(db) = open_db("unsigned", profile).await else {
        return;
    };
    migrate(&db, profile).await;
    let write = write_pool(&db);
    let rec = record("generic.record", Identifier::generate(), 1);
    let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
    assert_eq!(
        tx.setting("wicket.config_version").await.unwrap(),
        SPEC_VERSION
    );
    let out = render(
        &mut tx,
        rec,
        Format::Html,
        TemplateId::new(TemplateId::GENERIC_RECORD),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(String::from_utf8(out.bytes).unwrap().contains("UNSIGNED"));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn plain_shop_no_signature_block() {
    let profile = "plain-shop";
    let Some(db) = open_db("plain_sig", profile).await else {
        return;
    };
    migrate(&db, profile).await;
    let write = write_pool(&db);
    let rec = record("generic.record", Identifier::generate(), 1);
    mint_for_record(&write, rec.clone(), json!({})).await;
    let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
    assert_eq!(
        tx.setting("wicket.config_version").await.unwrap(),
        SPEC_VERSION
    );
    let out = render(
        &mut tx,
        rec,
        Format::Html,
        TemplateId::new(TemplateId::GENERIC_RECORD),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let html = String::from_utf8(out.bytes).unwrap();
    assert!(!html.contains("signatures"));
    assert!(!html.contains("UNSIGNED"));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn renderer_and_config_version_in_footer() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("footer", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let rec = record("generic.record", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let out = render(
            &mut tx,
            rec,
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let html = String::from_utf8(out.bytes).unwrap();
        assert!(html.contains("app_version="));
        assert!(html.contains(&format!("config_version={SPEC_VERSION}")));
        assert!(
            !html.contains(&format!("config_version={profile}")),
            "config_version must be spec_version, not profile id {profile}"
        );
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn archive_creates_immutable_blob_and_audit_row() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("arch1", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let blobs = blob_store("arch");
        let write = write_pool(&db);
        let rec = record("generic.record", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let rendered = render(
            &mut tx,
            rec.clone(),
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        let hash = archive(&mut tx, &rendered, rec.clone(), &blobs)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let n: (i64,) =
            sqlx::query_as("SELECT count(*) FROM print.render_log WHERE blob_hash IS NOT NULL")
                .fetch_one(db.migrate_pool())
                .await
                .unwrap();
        assert_eq!(n.0, 1);
        let stored = blobs.get(hash).expect("blob bytes in FsBlobStore");
        assert_eq!(stored, rendered.bytes);
        let events: (i64,) =
            sqlx::query_as("SELECT count(*) FROM audit.event WHERE table_name = 'render_log'")
                .fetch_one(db.migrate_pool())
                .await
                .unwrap();
        assert!(
            events.0 >= 1,
            "archive must leave an audited render_log row, got {}",
            events.0
        );
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn archive_twice_is_noop() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("arch2", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let blobs = blob_store("arch2");
        let write = write_pool(&db);
        let rec = record("generic.record", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let rendered = render(
            &mut tx,
            rec.clone(),
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        let h1 = archive(&mut tx, &rendered, rec.clone(), &blobs)
            .await
            .unwrap();
        let h2 = archive(&mut tx, &rendered, rec.clone(), &blobs)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(h1, h2);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn render_log_row_per_render() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("rlog", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let rec = record("generic.record", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        render(
            &mut tx,
            rec.clone(),
            Format::Html,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        render(
            &mut tx,
            rec.clone(),
            Format::Pdf,
            TemplateId::new(TemplateId::GENERIC_RECORD),
        )
        .await
        .unwrap();
        let rows = log(&mut tx, rec.clone()).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(rows.len(), 2);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn golden_document_revision() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("gold_doc", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng).await;
        let att_store = blob_store("gold_doc_att");
        let mut tx = Tx::begin(&write, &write_ctx("documents.create"))
            .await
            .unwrap();
        let doc = create(&mut tx, &eng, "SOP", "MDS-450-M4x12", "quality")
            .await
            .unwrap();
        let rev = new_revision(
            &mut tx,
            doc,
            "C",
            Manifest::content(json!({"part": "MDS-450-M4x12"})),
        )
        .await
        .unwrap();
        attach(
            &mut tx,
            &att_store,
            rev,
            b"%PDF-1.4 fixture-bytes",
            "MDS-450-M4x12-drawing.pdf",
            "application/pdf",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let rec = record("documents.revision", rev.0, 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let out = render(
            &mut tx,
            rec,
            Format::Html,
            TemplateId::new(TemplateId::DOCUMENT_REVISION),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let html = String::from_utf8(out.bytes).unwrap();
        assert!(html.contains("MDS-450-M4x12"));
        assert!(html.contains("Rev C"));
        assert!(html.contains("Attachments:"));
        assert!(html.contains("MDS-450-M4x12-drawing.pdf"));
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn golden_work_order_traveler() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("gold_wo", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let rec = record("work_order", Identifier::generate(), 1);
        let mut tx = Tx::begin(&write, &write_ctx("print.render")).await.unwrap();
        let out = render(
            &mut tx,
            rec,
            Format::Html,
            TemplateId::new(TemplateId::WORK_ORDER_TRAVELER),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let html = String::from_utf8(out.bytes).unwrap();
        assert!(html.contains("WO-2026-1847"));
        assert!(html.contains("Turn OD"));
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn list_templates_latest_effective_both_profiles() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("list_tpl", profile).await else {
            return;
        };
        migrate(&db, profile).await;
        let write = write_pool(&db);
        let mut tx = Tx::begin(&write, &write_ctx("print.templates"))
            .await
            .unwrap();
        let via_tx = list_templates(&mut tx, profile).await.unwrap();
        tx.commit().await.unwrap();
        let via_pool = list_templates_on(&read_pool(&db), profile).await.unwrap();
        assert_eq!(via_tx, via_pool);
        let ids: Vec<&str> = via_tx.iter().map(|t| t.template_id.as_str()).collect();
        assert_eq!(
            ids,
            [
                TemplateId::DOCUMENT_REVISION,
                TemplateId::GENERIC_RECORD,
                TemplateId::WORK_ORDER_TRAVELER
            ]
        );
        assert!(
            via_tx.iter().all(|t| t.template_id != "item_label"),
            "labels are ADR 0009; no item_label template"
        );
        assert!(
            via_tx
                .iter()
                .all(|t| t.version == 1 && t.body_hash.len() == 64)
        );
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn list_templates_unknown_profile() {
    let Some(db) = open_db("list_bad", "plain-shop").await else {
        return;
    };
    migrate(&db, "plain-shop").await;
    let write = write_pool(&db);
    let mut tx = Tx::begin(&write, &write_ctx("print.templates"))
        .await
        .unwrap();
    let err = list_templates(&mut tx, "not-a-profile").await.unwrap_err();
    tx.rollback().await.ok();
    assert!(matches!(err, wicket_print::Error::UnknownProfile(_)));
    let err = list_templates_on(&read_pool(&db), "not-a-profile")
        .await
        .unwrap_err();
    assert!(matches!(err, wicket_print::Error::UnknownProfile(_)));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = wicket_test::db_case!("print_writes_tx");
    migrate(&db, "plain-shop").await;
    let err = query(
        "INSERT INTO print.render_log
         (render_id, record_table, record_id, record_version, record_content_hash,
          template_id, template_version, renderer_version, output_format, output_hash)
         VALUES (gen_random_uuid(), 't', gen_random_uuid(), 1,
                 decode(repeat('00', 32), 'hex'), 'generic_record', 1, '0.1.0', 'html',
                 decode(repeat('00', 32), 'hex'))",
    )
    .execute(db.app_pool())
    .await
    .unwrap_err();
    assert_eq!(
        err.as_database_error()
            .and_then(|d| d.code().map(|c| c.to_string()))
            .unwrap_or_default(),
        "42501"
    );
    db.finish().await.unwrap();
}
