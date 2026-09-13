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
