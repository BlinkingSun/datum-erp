//! HTTP print routes: templates (published read), traveler render, archive.

#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use common::{NOPERM_PASSWORD, NOPERM_USER, PASSWORD, SIGNING_SECRET, USERNAME};
use serde_json::{Value, json};
use std::process::Command;
use uuid::Uuid;
use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::{Tx, WriteContext, WritePool};
use wicket_documents::{BlobHash, BlobStore};
use wicket_module::Profile;
use wicket_print::{TemplateId, seed_templates, set_installation_profile};
use wicket_server::{App, Config, Error};

fn profiles() -> [Profile; 2] {
    [
        Profile::plain_shop().unwrap(),
        Profile::regulated_device().unwrap(),
    ]
}

async fn ready(profile: Profile) -> common::World {
    let w = common::boot(profile).await;
    seed_print(&w).await;
    w
}

async fn seed_print(w: &common::World) {
    let write = WritePool::new(w.pool.clone());
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(wicket_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "print.test.seed",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.config_version = Some("1.0.0".into());
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin print seed");
    seed_templates(&mut tx).await.expect("seed templates");
    set_installation_profile(&mut tx, w.profile.as_str())
        .await
        .expect("stamp install profile");
    tx.commit().await.expect("commit print seed");
}

fn render_body(
    table: &str,
    id: Identifier,
    version: i64,
    format: &str,
    template_id: &str,
) -> Value {
    json!({
        "record": { "table": table, "id": id.to_string(), "version": version },
        "format": format,
        "template_id": template_id,
    })
}

fn decode_b64(s: &str) -> Vec<u8> {
    let table = |c: u8| -> u8 {
        match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    };
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = table(bytes[i]);
        let b = table(bytes[i + 1]);
        let pad_c = bytes[i + 2] == b'=';
        let pad_d = bytes[i + 3] == b'=';
        let c = if pad_c { 0 } else { table(bytes[i + 2]) };
        let d = if pad_d { 0 } else { table(bytes[i + 3]) };
        out.push((a << 2) | (b >> 4));
        if !pad_c {
            out.push(((b & 0x0f) << 4) | (c >> 2));
        }
        if !pad_d {
            out.push(((c & 0x03) << 6) | d);
        }
        i += 4;
    }
    out
}

fn html_from(body: &Value) -> String {
    let b64 = body["bytes_base64"].as_str().expect("bytes_base64");
    String::from_utf8(decode_b64(b64)).expect("utf8 html")
}

#[test]
fn server_print_handler_does_not_select_print_schema() {
    let src = include_str!("../src/handlers/print.rs");
    let lower = src.to_ascii_lowercase();
    assert!(
        !lower.contains("from print."),
        "server must not SELECT print.*"
    );
    let without_perm = src.replace("print.templates", "");
    assert!(
        !without_perm.contains("print.template"),
        "server must not name print.template"
    );
    assert!(
        !src.contains("print.render_log"),
        "server must not name print.render_log"
    );
    assert!(
        !src.contains("print.install"),
        "server must not name print.install"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_templates_lists_traveler_not_label() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = ready(profile).await;
        let (st, body) = w.get("/api/v1/print/templates").await;
        assert_eq!(st, StatusCode::OK, "templates {body}");
        assert_eq!(body["has_more"], false, "{body}");
        assert!(body["next_cursor"].is_null(), "{body}");
        let data = body["data"].as_array().expect("data");
        let ids: Vec<&str> = data
            .iter()
            .map(|t| t["template_id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                TemplateId::DOCUMENT_REVISION,
                TemplateId::GENERIC_RECORD,
                TemplateId::WORK_ORDER_TRAVELER
            ],
            "{body}"
        );
        assert!(!ids.contains(&"item_label"), "no item_label {body}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn render_html_and_pdf_both_profiles() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = ready(profile).await;
        let rec = Identifier::generate();
        let (st, html) = w
            .post(
                "/api/v1/print/render",
                render_body("generic.record", rec, 1, "html", TemplateId::GENERIC_RECORD),
            )
            .await;
        assert_eq!(st, StatusCode::OK, "html {html}");
        assert_eq!(html["template_version"], 1, "{html}");
        assert!(html["output_hash"].as_str().unwrap().len() == 64, "{html}");
        let decoded = html_from(&html);
        match w.profile {
            wicket_module::ProfileId::RegulatedDevice => {
                assert!(decoded.contains("UNSIGNED"), "regulated unsigned {decoded}");
            }
            wicket_module::ProfileId::PlainShop => {
                assert!(!decoded.contains("UNSIGNED"), "plain unsigned {decoded}");
                assert!(
                    !decoded.contains("signatures"),
                    "plain signatures {decoded}"
                );
            }
        }
        let (st, pdf) = w
            .post(
                "/api/v1/print/render",
                render_body("generic.record", rec, 1, "pdf", TemplateId::GENERIC_RECORD),
            )
            .await;
        assert_eq!(st, StatusCode::OK, "pdf {pdf}");
        let pdf_bytes = decode_b64(pdf["bytes_base64"].as_str().unwrap());
        assert!(pdf_bytes.starts_with(b"%PDF"), "pdf magic {pdf}");
        let (st, traveler) = w
            .post(
                "/api/v1/print/render",
                render_body(
                    "work_order",
                    Identifier::generate(),
                    1,
                    "html",
                    TemplateId::WORK_ORDER_TRAVELER,
                ),
            )
            .await;
        assert_eq!(st, StatusCode::OK, "traveler {traveler}");
        let traveler_html = html_from(&traveler);
        assert!(
            traveler_html.contains("WO-2026-1847"),
            "traveler render {traveler_html}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn render_regulated_signed_includes_manifestation() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = ready(Profile::regulated_device().unwrap()).await;
    let rec = Identifier::generate();
    let (st, minted) = w
        .post(
            "/api/v1/esign/signatures",
            json!({
                "meaning": "Approved",
                "record": {
                    "table": "generic.record",
                    "id": rec.to_string(),
                    "version": 1
                },
                "identification": { "code": USERNAME, "secret": SIGNING_SECRET }
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "mint {minted}");
    let (st, html) = w
        .post(
            "/api/v1/print/render",
            render_body("generic.record", rec, 1, "html", TemplateId::GENERIC_RECORD),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "render {html}");
    let decoded = html_from(&html);
    assert!(decoded.contains("M. Reyes"), "{decoded}");
    assert!(decoded.contains("Approved"), "{decoded}");
    assert!(!decoded.contains("UNSIGNED"), "{decoded}");
    let man = html["manifestation"].as_array().expect("manifestation");
    assert!(!man.is_empty(), "{html}");
    assert_eq!(man[0]["signature"]["printed_name"], "M. Reyes");
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_is_second_route_and_idempotent() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = ready(profile).await;
        let rec = Identifier::generate();
        let body = render_body("generic.record", rec, 1, "html", TemplateId::GENERIC_RECORD);
        let (st, rendered) = w.post("/api/v1/print/render", body).await;
        assert_eq!(st, StatusCode::OK, "render {rendered}");
        let hash = rendered["output_hash"].as_str().unwrap().to_string();
        let archive_body = json!({
            "record": { "table": "generic.record", "id": rec.to_string(), "version": 1 },
            "output_hash": hash,
        });
        let (st, a) = w.post("/api/v1/print/archive", archive_body.clone()).await;
        assert_eq!(st, StatusCode::OK, "archive {a}");
        let blob = a["blob_hash"].as_str().expect("blob_hash");
        assert_eq!(blob.len(), 64, "{a}");
        let key = Uuid::now_v7().to_string();
        let (st, b) = w
            .post_key("/api/v1/print/archive", archive_body.clone(), &key, vec![])
            .await;
        assert_eq!(st, StatusCode::OK, "archive replay first {b}");
        let (st, c) = w
            .post_key("/api/v1/print/archive", archive_body, &key, vec![])
            .await;
        assert_eq!(st, StatusCode::OK, "archive replay {c}");
        assert_eq!(b["blob_hash"], c["blob_hash"]);
        let stored = w.state.blobs().get(BlobHash(parse_hex32(blob)));
        assert!(stored.is_ok(), "composed FsBlobStore holds archived bytes");
    }
}

fn parse_hex32(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap();
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn print_permissions_and_validation() {
    if common::skip_if_no_pg() {
        return;
    }
    let mut w = ready(Profile::plain_shop().unwrap()).await;
    let rec = Identifier::generate();
    let (st, missing) = w
        .post(
            "/api/v1/print/render",
            json!({"record": {"table": "generic.record", "id": rec.to_string(), "version": 1}}),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "missing format {missing}");
    assert_eq!(missing["error"]["code"], "VALIDATION", "{missing}");
    let (st, label) = w
        .post(
            "/api/v1/print/render",
            render_body("generic.record", rec, 1, "html", "item_label"),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "item_label {label}");
    assert_eq!(label["error"]["code"], "VALIDATION", "{label}");
    let (st, unknown) = w
        .post(
            "/api/v1/print/render",
            render_body("generic.record", rec, 1, "html", "no_such_template"),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "unknown template {unknown}");
    assert_eq!(unknown["error"]["code"], "NOT_FOUND", "{unknown}");
    w.login_as(NOPERM_USER, NOPERM_PASSWORD).await;
    let (st, forbidden) = w.get("/api/v1/print/templates").await;
    assert_eq!(st, StatusCode::FORBIDDEN, "noperm templates {forbidden}");
    assert_eq!(forbidden["error"]["code"], "FORBIDDEN", "{forbidden}");
    let (st, forbidden_r) = w
        .post(
            "/api/v1/print/render",
            render_body("generic.record", rec, 1, "html", TemplateId::GENERIC_RECORD),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "noperm render {forbidden_r}");
    let (st, forbidden_a) = w
        .post(
            "/api/v1/print/archive",
            json!({
                "record": { "table": "generic.record", "id": rec.to_string(), "version": 1 },
                "output_hash": "00".repeat(32),
            }),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "noperm archive {forbidden_a}");
    let _ = PASSWORD;
}

#[tokio::test(flavor = "multi_thread")]
async fn print_unauthenticated_is_401() {
    if common::skip_if_no_pg() {
        return;
    }
    let mut w = ready(Profile::plain_shop().unwrap()).await;
    w.cookie.clear();
    w.csrf.clear();
    let (st, body) = w.get("/api/v1/print/templates").await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "UNAUTHENTICATED", "{body}");
    let rec = Identifier::generate();
    let (st, body) = w
        .post(
            "/api/v1/print/render",
            render_body("generic.record", rec, 1, "html", TemplateId::GENERIC_RECORD),
        )
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "{body}");
}

fn spawn_blob_root_child(kind: &str, blob_root: Option<&str>) {
    let exe = std::env::current_exe().expect("current_exe");
    let mut cmd = Command::new(&exe);
    cmd.args(["boot_refuses_without_blob_root", "--exact", "--nocapture"]);
    cmd.env("WICKET_TEST_CHILD", kind);
    match blob_root {
        Some(root) => {
            cmd.env("WICKET_BLOB_ROOT", root);
        }
        None => {
            cmd.env_remove("WICKET_BLOB_ROOT");
        }
    }
    let out = cmd.output().expect("spawn child");
    assert!(
        out.status.success(),
        "child {kind} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

async fn assert_boot_refuses_both_profiles(needle: &str) {
    for profile in profiles() {
        let id = profile.id.as_str().to_string();
        let cfg = Config {
            profile,
            bind: "127.0.0.1:0".parse().expect("bind"),
            database_url: "postgres://127.0.0.1:1/none".into(),
            migrate_url: "postgres://127.0.0.1:1/none".into(),
            bootstrap_url: "postgres://127.0.0.1:1/none".into(),
        };
        let Err(err) = App::boot(cfg).await else {
            panic!("App::boot must refuse without a usable WICKET_BLOB_ROOT (profile={id})");
        };
        let msg = err.to_string();
        assert!(
            msg.contains(needle),
            "profile={id} needle={needle:?} err={msg}"
        );
        assert!(
            matches!(err, Error::Config(_)),
            "profile={id} named configuration error, got {err:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn boot_refuses_without_blob_root() {
    match std::env::var("WICKET_TEST_CHILD").ok().as_deref() {
        Some("missing") => {
            assert_boot_refuses_both_profiles("WICKET_BLOB_ROOT is not set").await;
        }
        Some("notdir") => {
            assert_boot_refuses_both_profiles("WICKET_BLOB_ROOT is not a writable directory").await;
        }
        _ => {
            spawn_blob_root_child("missing", None);
            let file = std::env::temp_dir().join(format!("wicket-blob-not-dir-{}", Uuid::now_v7()));
            std::fs::write(&file, b"not-a-dir").expect("notdir file");
            spawn_blob_root_child("notdir", Some(file.to_str().expect("utf8 path")));
            let _ = std::fs::remove_file(&file);
        }
    }
}
