//! HTTP documents routes (Wave 3b). Both installation profiles.

#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use common::{NOPERM_PASSWORD, NOPERM_USER, SIGNING_SECRET, USERNAME};
use datum_core::{Actor, ActorKind, Identifier, PermissionKey, RecordRef, SignatureMeaning};
use datum_db::{Tx, WriteContext};
use datum_documents::{DOC_TYPE, EVENT_REVISION_CREATED};
use datum_esign::{InstanceTriple, MintRequest, mint};
use datum_identity::UserId;
use datum_module::Profile;
use serde_json::json;
use sqlx::query_scalar;
use uuid::Uuid;

fn profiles() -> [Profile; 2] {
    [
        Profile::plain_shop().unwrap(),
        Profile::regulated_device().unwrap(),
    ]
}

async fn operator_actor(w: &common::World) -> Actor {
    let id: Uuid = query_scalar("SELECT id FROM identity.principal WHERE username = $1")
        .bind(USERNAME)
        .fetch_one(&w.pool)
        .await
        .expect("mreyes");
    Actor {
        id: Identifier::from_uuid(id),
        kind: ActorKind::User,
    }
}

/// Submit is not in this wave's HTTP surface. Drive Draft → InReview through
/// the kernel so approve can be exercised as the Required documents edge.
async fn submit_document(w: &common::World, id: &str) {
    let kernel = w.state.kernel();
    let doc_id = Identifier::from_uuid(Uuid::parse_str(id).expect("uuid"));
    let doc = datum_statemachine::DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id,
    };
    let mut ctx = kernel.transition_context(operator_actor(w).await, &doc, "submit");
    ctx.reason = Some("test".into());
    ctx.actor_display = Some("M. Reyes".into());
    let write = kernel.write_pool();
    let mut tx = Tx::begin(&write, &ctx).await.expect("submit begin");
    kernel
        .transition(&mut tx, &doc, "submit", None, &ctx)
        .await
        .expect("submit");
    tx.commit().await.expect("submit commit");
}

/// HTTP `POST /esign/signatures` maps meaning `Approved` to
/// `calibration.approve` (`permission_for_meaning`). Documents mint through
/// the published esign API with `documents.approve` so the permission
/// snapshot matches the Required edge.
async fn mint_approve_signature(w: &common::World, id: &str, version: i64) -> String {
    let kernel = w.state.kernel();
    let actor = operator_actor(w).await;
    let principal = datum_identity::load_principal(kernel.pool(), UserId(actor.id))
        .await
        .expect("principal");
    let doc_id = Identifier::from_uuid(Uuid::parse_str(id).expect("uuid"));
    let mut ctx = WriteContext::new(actor, "esign.mint", "api");
    ctx.config_version = Some(kernel.profile.spec_version.clone());
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("test".into());
    let write = kernel.write_pool();
    let mut tx = Tx::begin(&write, &ctx).await.expect("mint begin");
    let (doc_type, state, live_version) = kernel
        .load_sm_instance(&mut tx, doc_id)
        .await
        .expect("instance")
        .expect("spawned");
    assert_eq!(live_version, version, "mint version matches GET");
    let projection = kernel
        .live_record(&mut tx, &doc_type, doc_id)
        .await
        .expect("live record");
    let sig = mint(
        &mut tx,
        &MintRequest {
            components: vec!["code".into(), "secret".into()],
            code: Some(USERNAME.into()),
            secret: SIGNING_SECRET.into(),
            meaning: SignatureMeaning("Approved".into()),
            reason: None,
            record: RecordRef {
                table: "sm.instance".into(),
                id: doc_id,
                version: live_version,
            },
            doc_type: doc_type.clone(),
            projection,
            instance: InstanceTriple {
                doc_type,
                doc_id,
                state,
                version: live_version,
            },
            permission: PermissionKey("documents.approve".into()),
            signed_at_zone: kernel.profile.seeded_permissions.display_timezone.clone(),
            policy: kernel.profile.session_policy.clone(),
            principal,
            login_session_id: None,
            device_fingerprint: Some("test".into()),
            source_ip: Some("127.0.0.1".into()),
            boot_epoch: "1".into(),
            credential_kind: "signing_password".into(),
        },
    )
    .await
    .expect("mint documents.approve");
    tx.commit().await.expect("mint commit");
    sig.id.as_uuid().to_string()
}

async fn create_sop(w: &common::World, title: &str) -> (String, i64) {
    let (st, v) = w
        .post(
            "/api/v1/documents",
            json!({
                "kind": "SOP",
                "title": title,
                "retention_class": "quality",
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "create {v}");
    let id = v["id"].as_str().expect("id").to_string();
    let version = v["version"].as_i64().unwrap_or(1);
    (id, version)
}

#[test]
fn handlers_use_kernel_seams() {
    let src = include_str!("../src/handlers/documents.rs");
    assert!(src.contains("create_document"));
    assert!(src.contains("new_document_revision"));
    assert!(src.contains("bind_esign_header"));
    assert!(src.contains("required_edge_token"));
    assert!(src.contains(".transition("));
    assert!(
        !src.contains("datum_documents::transition("),
        "never documents::transition + kernel.signature_gate"
    );
    assert!(!src.contains("signature_gate()"));
    assert!(!src.contains("datum_documents::new_revision("));
}

#[tokio::test(flavor = "multi_thread")]
async fn create_and_get_document_under_both_profiles() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let (id, ver) = create_sop(&w, "Work instruction").await;
        assert_eq!(ver, 1);
        let (st, got) = w.get(&format!("/api/v1/documents/{id}")).await;
        assert_eq!(st, StatusCode::OK, "get {got}");
        assert_eq!(got["id"], id);
        assert_eq!(got["kind"], "SOP");
        assert_eq!(got["status"], "Draft");
        assert_eq!(got["title"], "Work instruction");
        assert_eq!(got["retention_class"], "quality");
        assert_eq!(got["legal_hold"], false);
        assert_eq!(got["version"], 1);
        assert!(
            got["number"].as_str().unwrap().starts_with("SOP-"),
            "gap-free number, got {got}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn create_document_rejects_client_id_and_invalid_kind() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::plain_shop().unwrap()).await;
    let (st, body) = w
        .post(
            "/api/v1/documents",
            json!({
                "kind": "SOP",
                "title": "minted",
                "retention_class": "quality",
                "id": Uuid::now_v7().to_string(),
            }),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION", "{body}");
    assert_eq!(body["error"]["field"], "id", "{body}");

    let (st, body) = w
        .post(
            "/api/v1/documents",
            json!({
                "kind": "bad kind!",
                "title": "nope",
                "retention_class": "quality",
            }),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION", "{body}");
    assert_eq!(body["error"]["field"], "kind", "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn get_document_not_found_is_404() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::plain_shop().unwrap()).await;
    let missing = Uuid::now_v7();
    let (st, body) = w.get(&format!("/api/v1/documents/{missing}")).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "NOT_FOUND", "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn new_document_revision_under_both_profiles() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let (id, _) = create_sop(&w, "Revision parent").await;
        let (st, rev) = w
            .post(
                &format!("/api/v1/documents/{id}/revisions"),
                json!({"label": "A", "content": {"file": "sop.pdf"}}),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "revision {rev}");
        assert_eq!(rev["document_id"], id);
        assert_eq!(rev["label"], "A");
        let rev_id = rev["id"].as_str().expect("revision id");

        let created: i64 =
            query_scalar("SELECT count(*) FROM app.event WHERE name = $1 AND doc_id = $2")
                .bind(EVENT_REVISION_CREATED)
                .bind(Uuid::parse_str(&id).unwrap())
                .fetch_one(&w.pool)
                .await
                .expect("revision_created count");
        assert_eq!(
            created, 1,
            "Kernel::new_document_revision publishes the event"
        );
        let payload: serde_json::Value =
            query_scalar("SELECT payload FROM app.event WHERE name = $1 AND doc_id = $2")
                .bind(EVENT_REVISION_CREATED)
                .bind(Uuid::parse_str(&id).unwrap())
                .fetch_one(&w.pool)
                .await
                .expect("payload");
        assert_eq!(payload["revision_id"], rev_id);
        assert_eq!(payload["label"], "A");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_document_signature_gate_under_both_profiles() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile.clone()).await;
        let (id, _) = create_sop(&w, "Approve me").await;
        submit_document(&w, &id).await;
        let (st, got) = w.get(&format!("/api/v1/documents/{id}")).await;
        assert_eq!(st, StatusCode::OK, "{got}");
        assert_eq!(got["status"], "InReview", "{got}");
        let ver = got["version"].as_i64().expect("version after submit");

        let (st, unsigned) = w
            .post_if_match(&format!("/api/v1/documents/{id}/approve"), json!({}), ver)
            .await;
        match profile.id {
            datum_module::ProfileId::RegulatedDevice => {
                assert_eq!(st, StatusCode::UNAUTHORIZED, "unsigned {unsigned}");
                assert_eq!(
                    unsigned["error"]["code"], "SIGNATURE_REQUIRED",
                    "{unsigned}"
                );
                assert!(
                    unsigned["error"]["message"]
                        .as_str()
                        .unwrap_or("")
                        .contains("missing token"),
                    "D-2b-5 missing token, got {unsigned}"
                );
                let after = w.get(&format!("/api/v1/documents/{id}")).await.1;
                assert_eq!(after["status"], "InReview", "unsigned writes nothing");

                let sig_id = mint_approve_signature(&w, &id, ver).await;

                let (st, _, approved) = w
                    .call(
                        "POST",
                        &format!("/api/v1/documents/{id}/approve"),
                        Some(vec![
                            ("if-match", format!("\"{ver}\"")),
                            ("x-datum-signature", sig_id.to_string()),
                        ]),
                        Some(json!({})),
                    )
                    .await;
                assert_eq!(st, StatusCode::OK, "signed approve {approved}");
                assert_eq!(approved["status"], "Approved", "{approved}");

                let new_ver = approved["version"].as_i64().unwrap_or(ver + 1);
                let (st, _, replay) = w
                    .call(
                        "POST",
                        &format!("/api/v1/documents/{id}/approve"),
                        Some(vec![
                            ("if-match", format!("\"{new_ver}\"")),
                            ("x-datum-signature", sig_id.to_string()),
                        ]),
                        Some(json!({})),
                    )
                    .await;
                assert_eq!(st, StatusCode::CONFLICT, "replay {replay}");
                assert_eq!(replay["error"]["code"], "CONFLICT", "{replay}");
            }
            datum_module::ProfileId::PlainShop => {
                assert_eq!(st, StatusCode::OK, "plain-shop unsigned {unsigned}");
                assert_eq!(unsigned["status"], "Approved", "{unsigned}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn documents_routes_require_permission() {
    if common::skip_if_no_pg() {
        return;
    }
    let mut w = common::boot(Profile::plain_shop().unwrap()).await;
    w.login_as(NOPERM_USER, NOPERM_PASSWORD).await;
    let missing = Uuid::now_v7();
    let (st, body) = w
        .post(
            "/api/v1/documents",
            json!({"kind": "SOP", "title": "x", "retention_class": "quality"}),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "create {body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN", "{body}");
    let (st, body) = w.get(&format!("/api/v1/documents/{missing}")).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "get {body}");
    let (st, body) = w
        .post(
            &format!("/api/v1/documents/{missing}/revisions"),
            json!({"label": "A"}),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "revision {body}");
    let (st, body) = w
        .post_if_match(
            &format!("/api/v1/documents/{missing}/approve"),
            json!({}),
            1,
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "approve {body}");
}
