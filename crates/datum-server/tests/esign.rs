//! HTTP esign routes (D-2b-8) under both installation profiles.

#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use common::{NOPERM_PASSWORD, NOPERM_USER, PASSWORD, SIGNING_SECRET, USERNAME};
use datum_module::Profile;
use serde_json::json;
use uuid::Uuid;

fn profiles() -> [Profile; 2] {
    [
        Profile::plain_shop().unwrap(),
        Profile::regulated_device().unwrap(),
    ]
}

fn mint_body(record_id: Uuid) -> serde_json::Value {
    json!({
        "meaning": "Approved",
        "record": {
            "table": "sm.instance",
            "id": record_id.to_string(),
            "version": 1
        },
        "identification": {
            "code": USERNAME,
            "secret": SIGNING_SECRET
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn esign_http_under_both_profiles() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let mut w = common::boot(profile).await;
        let (st, ch) = w.post("/api/v1/esign/challenges", json!({})).await;
        assert_eq!(st, StatusCode::OK, "challenge {ch}");
        let comps = ch["components_required"].as_array().expect("components");
        assert!(
            comps.iter().any(|c| c.as_str() == Some("code"))
                && comps.iter().any(|c| c.as_str() == Some("secret")),
            "v1 two components, got {ch}"
        );
        assert_eq!(ch["credential_kind"], "signing_password");

        let rec = Uuid::now_v7();
        let (st, minted) = w.post("/api/v1/esign/signatures", mint_body(rec)).await;
        assert_eq!(st, StatusCode::CREATED, "mint {minted}");
        let id = minted["signature"]["id"].as_str().expect("id");
        assert_eq!(minted["signature"]["meaning"], "Approved");
        assert_eq!(minted["signature"]["printed_name"], "M. Reyes");

        let (st, got) = w.get(&format!("/api/v1/esign/signatures/{id}")).await;
        assert_eq!(st, StatusCode::OK, "get {got}");
        assert_eq!(got["signature"]["id"], id);

        let (st, bundle) = w
            .get(&format!("/api/v1/esign/signatures/{id}/bundle"))
            .await;
        assert_eq!(st, StatusCode::OK, "bundle {bundle}");
        assert_eq!(bundle["manifestation"]["signature"]["id"], id);

        let key = Uuid::now_v7().to_string();
        let rec2 = Uuid::now_v7();
        let (st, a) = w
            .post_key("/api/v1/esign/signatures", mint_body(rec2), &key, vec![])
            .await;
        assert_eq!(st, StatusCode::CREATED, "idempotent mint {a}");
        let (st, b) = w
            .post_key("/api/v1/esign/signatures", mint_body(rec2), &key, vec![])
            .await;
        assert_eq!(st, StatusCode::CREATED, "idempotent replay {b}");
        assert_eq!(a["signature"]["id"], b["signature"]["id"]);

        let (st, login_as_signing) = w
            .post(
                "/api/v1/esign/signatures",
                json!({
                    "meaning": "Approved",
                    "record": { "table": "sm.instance", "id": Uuid::now_v7().to_string(), "version": 1 },
                    "identification": { "code": USERNAME, "secret": PASSWORD }
                }),
            )
            .await;
        assert_eq!(
            st,
            StatusCode::BAD_REQUEST,
            "login secret {login_as_signing}"
        );
        assert_eq!(
            login_as_signing["error"]["code"], "VALIDATION",
            "{login_as_signing}"
        );

        w.login_as(NOPERM_USER, NOPERM_PASSWORD).await;
        let (st, no_cred) = w
            .post(
                "/api/v1/esign/signatures",
                json!({
                    "meaning": "Approved",
                    "record": { "table": "sm.instance", "id": Uuid::now_v7().to_string(), "version": 1 },
                    "identification": { "code": NOPERM_USER, "secret": "not-a-signing-secret" }
                }),
            )
            .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED, "no signing cred {no_cred}");
        assert_eq!(no_cred["error"]["code"], "SIGNATURE_REQUIRED", "{no_cred}");
        assert_eq!(
            no_cred["error"]["field"], "identification.secret",
            "{no_cred}"
        );
        w.login_as(USERNAME, PASSWORD).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dummy_token_error_code_under_both_profiles() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile.clone()).await;
        match profile.id {
            datum_module::ProfileId::RegulatedDevice => {
                let cal = w.calibration_doc.as_ref().expect("calibration spawned");
                let (st, body) = w
                    .post_if_match(
                        &format!("/api/v1/calibration/certificates/{cal}/approve"),
                        json!({}),
                        1,
                    )
                    .await;
                assert_eq!(st, StatusCode::FORBIDDEN, "dummy {body}");
                assert_eq!(
                    body["error"]["code"], "SIGNATURE_REQUIRED",
                    "D-2b-5 Invalid while esign is bound, got {body}"
                );
                assert_ne!(
                    body["error"]["code"], "SIGNATURE_NO_PROVIDER",
                    "409 is NoProvider only"
                );
            }
            datum_module::ProfileId::PlainShop => {
                let edges = &w.get("/api/v1/iq/manifest").await.1["signature_edges"];
                let required = edges
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e.get("Required").is_some() || e.get("required").is_some())
                    .count();
                assert_eq!(required, 0, "plain-shop has no Required edge {edges}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn regulated_release_refused_without_signature_succeeds_with_two_component_signature() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::regulated_device().unwrap()).await;
    let cal = w
        .calibration_doc
        .clone()
        .expect("calibration.certificate spawned at boot");
    let rec = Uuid::parse_str(&cal).expect("uuid");
    let (st, refused) = w
        .post_if_match(
            &format!("/api/v1/calibration/certificates/{cal}/approve"),
            json!({}),
            1,
        )
        .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "refused without signature {refused}"
    );
    assert_eq!(refused["error"]["code"], "SIGNATURE_REQUIRED", "{refused}");

    let (st, minted) = w.post("/api/v1/esign/signatures", mint_body(rec)).await;
    assert_eq!(st, StatusCode::CREATED, "mint {minted}");
    let sig_id = minted["signature"]["id"].as_str().expect("id");

    let (st, _, approved) = w
        .call(
            "POST",
            &format!("/api/v1/calibration/certificates/{cal}/approve"),
            Some(vec![
                ("if-match", "\"1\"".into()),
                ("x-datum-signature", sig_id.to_string()),
            ]),
            Some(json!({})),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "signed approve {approved}");
    assert_eq!(approved["status"], "approved", "{approved}");

    let w_plain = common::boot(Profile::plain_shop().unwrap()).await;
    assert!(
        w_plain.calibration_doc.is_none(),
        "plain-shop enables no regulated module"
    );
}

#[test]
fn esign_mint_remembers_in_the_mint_transaction() {
    let src = include_str!("../src/handlers.rs");
    let start = src
        .find("async fn esign_mint_inner")
        .expect("esign_mint_inner");
    let rest = &src[start..];
    let end = rest
        .find("\n/// GET /api/v1/esign/signatures/{id}")
        .unwrap_or(rest.len());
    let body = &rest[..end];
    let begins = body.matches("Tx::begin(").count();
    assert_eq!(begins, 1, "mint + remember share one Tx, found {begins}");
    let mint = body.find("datum_esign::mint").expect("mint call");
    let after_mint = &body[mint..];
    assert!(
        !after_mint.contains("Tx::begin("),
        "no follow-up Tx after mint"
    );
    let remember = after_mint.find("idempotency::remember").expect("remember");
    let commit = after_mint.find("tx.commit()").expect("commit after mint");
    assert!(
        remember < commit,
        "remember must run before commit in the mint Tx"
    );
}
