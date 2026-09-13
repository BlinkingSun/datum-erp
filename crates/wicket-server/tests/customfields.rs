//! Named HTTP tests for custom-fields routes, both installation profiles.

#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_arguments
)]

mod common;

use axum::http::StatusCode;
use common::World;
use serde_json::{Value, json};
use uuid::Uuid;
use wicket_module::Profile;

fn profiles() -> [Profile; 2] {
    [
        Profile::plain_shop().unwrap(),
        Profile::regulated_device().unwrap(),
    ]
}

async fn for_each_profile<F, Fut>(f: F)
where
    F: Fn(Profile) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for profile in profiles() {
        f(profile).await;
    }
}

async fn create_item(w: &World, number: &str) -> String {
    let (st, v) = w
        .post(
            "/api/v1/items",
            json!({
                "number": number,
                "revision": "A",
                "description": "customfields fixture",
                "kind": "buy",
                "stock_uom": 1,
                "stock_scale": 0,
                "residual_tolerance": "0",
                "cost_method": "FIFO",
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "create item {number} {v}");
    v["id"].as_str().unwrap().to_string()
}

async fn define_field(w: &World, key: &str, label: &str) -> String {
    let (st, v) = w
        .post(
            "/api/v1/customfields/definitions",
            json!({
                "entity": "items.item",
                "key": key,
                "type": "string",
                "label": label,
                "validation_rule": "",
                "required": false,
                "indexed": false,
                "owner_module": "mod-items",
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "define {key} {v}");
    assert_eq!(v["key"], key, "{v}");
    v["id"].as_str().unwrap().to_string()
}

fn data_by_key(body: &Value) -> std::collections::BTreeMap<String, Value> {
    let mut map = std::collections::BTreeMap::new();
    for row in body["data"].as_array().expect("data") {
        map.insert(row["key"].as_str().unwrap().to_string(), row.clone());
    }
    map
}

async fn put_fields(w: &World, item: &str, body: Value) -> (StatusCode, Value) {
    let (s, _, v) = w
        .call(
            "PUT",
            &format!("/api/v1/items/{item}/custom-fields"),
            Some(vec![
                ("idempotency-key", Uuid::now_v7().to_string()),
                ("content-type", "application/json".into()),
            ]),
            Some(body),
        )
        .await;
    (s, v)
}

#[tokio::test(flavor = "multi_thread")]
async fn post_customfields_definitions() {
    if common::skip_if_no_pg() {
        return;
    }
    for_each_profile(|profile| async move {
        let w = common::boot(profile).await;
        let id = define_field(&w, "shop_note", "Shop note").await;
        assert!(!id.is_empty());
        let (st, body) = w
            .get("/api/v1/customfields/definitions?entity=items.item")
            .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        let by_key = data_by_key(&body);
        assert_eq!(by_key["shop_note"]["id"], id, "{body}");
        assert!(
            by_key.contains_key("udi_device_identifier"),
            "manifest field remains listed: {body}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_customfields_definitions() {
    if common::skip_if_no_pg() {
        return;
    }
    for_each_profile(|profile| async move {
        let w = common::boot(profile).await;
        let (st, body) = w
            .get("/api/v1/customfields/definitions?entity=items.item")
            .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        let before = data_by_key(&body);
        assert!(
            before.contains_key("udi_device_identifier"),
            "kernel seeds UDI-DI: {body}"
        );
        assert!(!before.contains_key("gtin"), "{body}");
        define_field(&w, "gtin", "GTIN").await;
        let (st, body) = w
            .get("/api/v1/customfields/definitions?entity=items.item")
            .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        let after = data_by_key(&body);
        assert!(after.contains_key("gtin"), "{body}");
        assert!(after.contains_key("udi_device_identifier"), "{body}");
        let (st, body) = w
            .get("/api/v1/customfields/definitions?entity=lots.lot")
            .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["data"], json!([]), "entity filter {body}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_customfields_definitions_requires_entity() {
    if common::skip_if_no_pg() {
        return;
    }
    for_each_profile(|profile| async move {
        let w = common::boot(profile).await;
        let (st, body) = w.get("/api/v1/customfields/definitions").await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "VALIDATION", "{body}");
        assert_eq!(body["error"]["field"], "entity", "{body}");
        let (st, body) = w.get("/api/v1/customfields/definitions?entity=").await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "VALIDATION", "{body}");
        assert_eq!(body["error"]["field"], "entity", "{body}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn post_customfields_definitions_retire() {
    if common::skip_if_no_pg() {
        return;
    }
    for_each_profile(|profile| async move {
        let w = common::boot(profile).await;
        let id = define_field(&w, "legacy", "Legacy").await;
        let (st, body) = w
            .post_if_match(
                &format!("/api/v1/customfields/definitions/{id}/retire"),
                json!({}),
                1,
            )
            .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "retired", "{body}");
        let (st, listed) = w
            .get("/api/v1/customfields/definitions?entity=items.item")
            .await;
        assert_eq!(st, StatusCode::OK, "{listed}");
        let by_key = data_by_key(&listed);
        assert!(
            !by_key.contains_key("legacy"),
            "retired is not ACTIVE {listed}"
        );
        assert!(
            by_key.contains_key("udi_device_identifier"),
            "other ACTIVE defs remain {listed}"
        );
        let (st, again) = w
            .post_if_match(
                &format!("/api/v1/customfields/definitions/{id}/retire"),
                json!({}),
                1,
            )
            .await;
        assert_eq!(st, StatusCode::CONFLICT, "{again}");
        assert_eq!(again["error"]["code"], "CONFLICT", "{again}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn put_item_custom_fields() {
    if common::skip_if_no_pg() {
        return;
    }
    for_each_profile(|profile| async move {
        let w = common::boot(profile).await;
        let item = create_item(&w, "CF-PUT-1").await;
        define_field(&w, "shop_note", "Shop note").await;
        let (st, body) = put_fields(
            &w,
            &item,
            json!({
                "fields": [{
                    "key": "shop_note",
                    "type": "string",
                    "value": "hello"
                }]
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        let by_key = data_by_key(&body);
        assert_eq!(by_key["shop_note"]["value"], "hello", "{body}");
        assert_eq!(by_key["shop_note"]["type"], "string", "{body}");
        assert!(
            by_key["udi_device_identifier"]["value"].is_null(),
            "unset ACTIVE definition is merged: {body}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_item_custom_fields() {
    if common::skip_if_no_pg() {
        return;
    }
    for_each_profile(|profile| async move {
        let w = common::boot(profile).await;
        let item = create_item(&w, "CF-GET-1").await;
        define_field(&w, "set_me", "Set me").await;
        define_field(&w, "leave_unset", "Leave unset").await;
        let (st, _) = put_fields(
            &w,
            &item,
            json!({
                "fields": [{
                    "key": "set_me",
                    "type": "string",
                    "value": "present"
                }]
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (st, body) = w.get(&format!("/api/v1/items/{item}/custom-fields")).await;
        assert_eq!(st, StatusCode::OK, "{body}");
        let by_key = data_by_key(&body);
        assert_eq!(by_key["set_me"]["value"], "present", "{body}");
        assert!(
            by_key["leave_unset"]["value"].is_null(),
            "unset ACTIVE definition must still appear: {body}"
        );
        assert!(
            by_key["udi_device_identifier"]["value"].is_null(),
            "definitions_for + get, not list_for_record alone: {body}"
        );
    })
    .await;
}
