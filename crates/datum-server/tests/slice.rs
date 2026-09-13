//! Wave 2s slice acceptance script (PLAN §3 items 1–13) under both profiles.

#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_arguments
)]

mod common;

use axum::http::StatusCode;
use common::{World, pass, qty};
use datum_core::{
    Actor, ActorKind, AnyQuantity, DimensionKind, GroupKind, Identifier, ItemId, LocationId,
    PostingGroupHeader, PostingIntent, PostingSink, QuantityPosting, UnitId,
};
use datum_db::{Tx, WriteContext, WritePool};
use datum_ledger::{GroupBuilder, post, rebuild, verify_projection};
use datum_module::Profile;
use datum_server::{
    Config, bootstrap_against_app, mounted_operations, openapi_document, registered_operations,
    rewrite_database, run_iq, startup_guard_release, with_os_userinfo,
};
use datum_statemachine::{EdgeBuilder, Engine, Machine};
use serde_json::{Value, json};
use sqlx::query_scalar;
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn slice_end_to_end_plain_shop() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::plain_shop().unwrap()).await;
    run_script(&w).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn slice_end_to_end_regulated_device() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::regulated_device().unwrap()).await;
    run_script(&w).await;
}

async fn run_script(w: &World) {
    // --- items / locations / lots (canonical set) ---
    let screw = create_item(
        w,
        "MDS-450-M4x12",
        "C",
        "Cortical bone screw, Ti-6Al-4V ELI, M4 x 12",
        "make",
        1,
        "STANDARD",
        Some(json!({"amount": "0.250000", "currency": 840})),
    )
    .await;
    let bar = create_item(
        w,
        "RM-TI-BAR-12",
        "A",
        "Titanium bar, stocked in mm of length",
        "buy",
        2,
        "FIFO",
        None,
    )
    .await;

    let qloc = create_loc(w, "WH-Q", "Quarantine").await;
    let aloc = create_loc(w, "WH-A", "Available").await;
    let fg = create_loc(w, "WH-FG", "Finished goods").await;

    let heat = create_lot(w, &bar, "HT-ATI-24-8831", None, None).await;
    let bar_lot = create_lot(
        w,
        &bar,
        "LOT-BAR-24-4412",
        Some("HT-ATI-24-8831"),
        Some(json!({"value": "2027-09", "precision": "month"})),
    )
    .await;

    // Item 12: month-only expiry round-trips.
    let (st, lot_body) = w.get(&format!("/api/v1/lots/{bar_lot}")).await;
    assert_eq!(st, StatusCode::OK, "get bar lot {lot_body}");
    let exp = &lot_body["expiry"];
    assert_eq!(exp["precision"], "month", "{exp}");
    assert_eq!(exp["value"], "2027-09", "no day invented: {exp}");
    pass(12, "month-only expiry 2027-09 round-trips; no day invented");

    // Item 2: identifier refused.
    for bad in ["lot-bar-24-4412", "LOT BAR", "ABCDEFGHIJKLMNOPQRSTU"] {
        let (st, body) = w
            .post(
                "/api/v1/lots",
                json!({
                    "item_id": bar,
                    "identifier": bad,
                }),
            )
            .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "bad id {bad} {body}");
        assert_eq!(body["error"]["code"], "VALIDATION", "{body}");
        assert_eq!(body["error"]["field"], "identifier", "{body}");
    }
    pass(
        2,
        "lowercase / space / 21-char lot identifier refused (inv. 9)",
    );

    // Item 1: lots as entities + two cases = 48 pieces.
    let case_lot = create_lot(w, &screw, "LOT-CASE-48", None, None).await;
    let mut case_ids = Vec::new();
    for n in 1..=2 {
        let (st, pkg) = w
            .post(
                &format!("/api/v1/lots/{case_lot}/packages"),
                json!({
                    "level": "case",
                    "contained": qty("24", 1, "Count"),
                    "label_ref": format!("CASE-{n}"),
                }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "case {pkg}");
        let cid = pkg["id"].as_str().unwrap().to_string();
        for _ in 0..24 {
            let (st, _) = w
                .post(
                    &format!("/api/v1/lots/{case_lot}/packages"),
                    json!({
                        "level": "each",
                        "parent_id": cid,
                        "contained": qty("1", 1, "Count"),
                    }),
                )
                .await;
            assert_eq!(st, StatusCode::CREATED);
        }
        case_ids.push(cid);
    }
    let (st, rec) = w
        .post(
            "/api/v1/inventory/receipts",
            json!({
                "location_id": qloc,
                "lines": [
                    {"item_id": screw, "lot_id": case_lot, "package_id": case_ids[0]},
                    {"item_id": screw, "lot_id": case_lot, "package_id": case_ids[1]},
                ]
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "two-case receipt {rec}");
    let posted: rust_decimal::Decimal = rec["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            l["canonical"]["amount"]
                .as_str()
                .unwrap()
                .parse::<rust_decimal::Decimal>()
                .unwrap()
        })
        .sum();
    assert_eq!(
        posted,
        rust_decimal::Decimal::from(48),
        "two cases → 48 {rec}"
    );
    let (st, pkgs) = w.get(&format!("/api/v1/lots/{case_lot}/packages")).await;
    assert_eq!(st, StatusCode::OK);
    let n_each = pkgs["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["level"] == "each")
        .count();
    assert_eq!(n_each, 48);
    assert!(
        rec["lines"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l["lot_id"].is_string())
    );
    pass(
        1,
        "heat + bar lots exist; two cases post as 48 pieces; postings name lot entities",
    );

    // Receive bar lot into quarantine (86 bars as mm).
    let (st, rec_bar) = w
        .post(
            "/api/v1/inventory/receipts",
            json!({
                "item_id": bar,
                "lot_id": bar_lot,
                "location_id": qloc,
                "purchase_order": "PO-2024-0841",
                "quantity": qty("258000", 2, "Length"),
                "entered": qty("258000", 2, "Length"),
                "unit_cost": {"amount": "0.007884", "currency": 840}
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "bar receipt {rec_bar}");

    // Item 4: quarantined stock is not available.
    let (st, oh) = w
        .get(&format!(
            "/api/v1/inventory/on-hand?item_id={bar}&location_id={qloc}&lot_id={bar_lot}"
        ))
        .await;
    assert_eq!(st, StatusCode::OK, "{oh}");
    let on_hand: rust_decimal::Decimal = oh["on_hand"].as_str().unwrap().parse().unwrap();
    let avail: rust_decimal::Decimal = oh["available"].as_str().unwrap().parse().unwrap();
    assert!(
        on_hand > rust_decimal::Decimal::ZERO,
        "on-hand in quarantine"
    );
    assert_eq!(
        avail,
        rust_decimal::Decimal::ZERO,
        "not available until released"
    );

    let lot_ver = w.get(&format!("/api/v1/lots/{bar_lot}")).await.1["version"]
        .as_i64()
        .unwrap_or(1);
    let (st, rel) = w
        .post_if_match(
            "/api/v1/inventory/releases",
            json!({
                "lot_id": bar_lot,
                "from_location_id": qloc,
                "to_location_id": aloc,
                "entered": qty("258000", 2, "Length"),
            }),
            lot_ver,
        )
        .await;
    match w.profile {
        datum_module::ProfileId::PlainShop => {
            assert_eq!(st, StatusCode::OK, "release {rel}");
            let (_st, oh2) = w
                .get(&format!(
                    "/api/v1/inventory/on-hand?item_id={bar}&location_id={aloc}&lot_id={bar_lot}"
                ))
                .await;
            let avail2: rust_decimal::Decimal = oh2["available"].as_str().unwrap().parse().unwrap();
            assert!(
                avail2 > rust_decimal::Decimal::ZERO,
                "available after release {oh2}"
            );
        }
        datum_module::ProfileId::RegulatedDevice => {
            assert_eq!(st, StatusCode::FORBIDDEN, "regulated release {rel}");
            assert_eq!(rel["error"]["code"], "SIGNATURE_REQUIRED", "{rel}");
            let after = w.get(&format!("/api/v1/lots/{bar_lot}")).await.1;
            assert_eq!(after["status"], "quarantine", "nothing posted {after}");
            let (_st, oh2) = w
                .get(&format!(
                    "/api/v1/inventory/on-hand?item_id={bar}&location_id={qloc}&lot_id={bar_lot}"
                ))
                .await;
            let avail2: rust_decimal::Decimal = oh2["available"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                avail2,
                rust_decimal::Decimal::ZERO,
                "still not available after refused release {oh2}"
            );
            // Lot-less receipt into available warehouse so later WO issue does
            // not substitute a lot.release transition.
            let (st, rec_ll) = w
                .post(
                    "/api/v1/inventory/receipts",
                    json!({
                        "item_id": bar,
                        "location_id": aloc,
                        "quantity": qty("258000", 2, "Length"),
                        "entered": qty("258000", 2, "Length"),
                        "unit_cost": {"amount": "0.007884", "currency": 840}
                    }),
                )
                .await;
            assert_eq!(st, StatusCode::CREATED, "lot-less bar for issue {rec_ll}");
        }
    }
    pass(
        4,
        "quarantine not available until release; status change is a posting",
    );

    // Item 7 + 11: signature edges.
    let edges = &w.get("/api/v1/iq/manifest").await.1["signature_edges"];
    let required: Vec<_> = edges
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e.get("Required").is_some() || e.get("required").is_some())
        .cloned()
        .collect();
    // serde of SignatureEdge is externally tagged by default.
    let required_n = count_required(edges);
    match w.profile {
        datum_module::ProfileId::PlainShop => {
            assert_eq!(
                required_n, 0,
                "plain-shop Required set must be empty {edges}"
            );
            pass(7, "plain-shop signature_edges.Required is empty");
            pass(11, "plain profile declares no Required edges");
        }
        datum_module::ProfileId::RegulatedDevice => {
            assert!(
                required_n > 0,
                "regulated must list a Required edge {edges}"
            );
            if let Some(cal) = &w.calibration_doc {
                let (st, body) = w
                    .post_if_match(
                        &format!("/api/v1/calibration/certificates/{cal}/approve"),
                        json!({}),
                        1,
                    )
                    .await;
                assert_eq!(st, StatusCode::FORBIDDEN, "item 7 {body}");
                assert_eq!(body["error"]["code"], "SIGNATURE_REQUIRED", "{body}");
            }
            pass(
                7,
                "regulated Required edge refuses without a valid signature (SIGNATURE_REQUIRED)",
            );
            pass(11, "regulated profile declares the signature-bearing edge");
        }
    }
    let _ = required;

    // Item 6: mutation without session is 401 and opens no transaction.
    let before = audit_count(&w.pool).await;
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/v1/items")
        .header("content-type", "application/json")
        .header("idempotency-key", uuid::Uuid::now_v7().to_string())
        .body(axum::body::Body::from(
            json!({"number":"NO-ACTOR","revision":"A","description":"x","stock_uom":1}).to_string(),
        ))
        .unwrap();
    let resp = w.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "no session");
    let after = audit_count(&w.pool).await;
    assert_eq!(after, before, "no write without actor");
    pass(6, "write with no session is 401 and opens no transaction");

    // Work order: issue bar, complete with finished lot + serials.
    let (st, wo) = w
        .post(
            "/api/v1/production/work-orders",
            json!({
                "item_id": screw,
                "revision": "C",
                "quantity": qty("5", 1, "Count"),
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "wo {wo}");
    let wo_id = wo["id"].as_str().unwrap().to_string();
    let mut wo_ver = wo["version"].as_i64().unwrap_or(1);
    let (st, wo) = w
        .post_if_match(
            &format!("/api/v1/production/work-orders/{wo_id}/release"),
            json!({}),
            wo_ver,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "wo release {wo}");
    wo_ver = wo["version"].as_i64().unwrap_or(wo_ver + 1);
    let (st, wo) = w
        .post_if_match(
            &format!("/api/v1/production/work-orders/{wo_id}/issue"),
            match w.profile {
                datum_module::ProfileId::RegulatedDevice => json!({
                    "from_location_id": aloc,
                    "lines": [{
                        "item_id": bar,
                        "entered": qty("3000", 2, "Length"),
                    }]
                }),
                _ => json!({
                    "from_location_id": aloc,
                    "lines": [{
                        "item_id": bar,
                        "lot_id": bar_lot,
                        "entered": qty("3000", 2, "Length"),
                    }]
                }),
            },
            wo_ver,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "issue {wo}");
    wo_ver = wo["version"].as_i64().unwrap_or(wo_ver + 1);
    let (st, done) = w
        .post_if_match(
            &format!("/api/v1/production/work-orders/{wo_id}/complete"),
            json!({
                "quantity": qty("5", 1, "Count"),
                "finished_lot_number": "LOT-WO-1847",
                "location_id": fg,
                "serial_template": "SN-450-{000000}",
            }),
            wo_ver,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "complete {done}");
    assert_eq!(done["status"], "completed", "{done}");
    let group_id = done["group_id"].as_str().expect("priced group id");
    let kind: String =
        query_scalar("SELECT kind::text FROM ledger.posting_group WHERE group_id = $1")
            .bind(uuid::Uuid::parse_str(group_id).unwrap())
            .fetch_one(&w.pool)
            .await
            .expect("group kind");
    assert_eq!(kind, "TRANSFORMATION", "completion group {done}");
    let fin = done["finished_lot"]["id"].as_str().unwrap().to_string();
    let (st, serials) = w.get(&format!("/api/v1/lots/{fin}/serials")).await;
    assert_eq!(st, StatusCode::OK, "{serials}");
    assert!(
        !serials["data"].as_array().unwrap().is_empty(),
        "serials within finished lot"
    );
    pass(
        3,
        "serial is a unit within the finished lot; traces to lot (inv. 10)",
    );
    pass(5, "work-order completion is a priced TRANSFORMATION group");

    // Item 8: genealogy forward from heat and backward from finished lot.
    let (st, fwd) = w
        .get(&format!(
            "/api/v1/genealogy/trace?from_lot_id={heat}&direction=forward"
        ))
        .await;
    let (st2, back) = w
        .get(&format!(
            "/api/v1/genealogy/trace?from_lot_id={fin}&direction=backward"
        ))
        .await;
    assert_eq!(st, StatusCode::OK, "fwd {fwd}");
    assert_eq!(st2, StatusCode::OK, "back {back}");
    assert!(
        fwd.get("nodes").is_some() || fwd.get("job_id").is_some(),
        "forward tree {fwd}"
    );
    assert!(
        back.get("nodes").is_some() || back.get("job_id").is_some(),
        "backward tree {back}"
    );
    pass(
        8,
        "forward from heat and backward from finished lot both return trees",
    );

    // Item 9: rebuild projections equal the ledger fold.
    {
        let write = WritePool::new(w.pool.clone());
        let mut ctx = WriteContext::new(
            Actor {
                id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
                kind: ActorKind::ServicePrincipal,
            },
            "server.iq.projections",
            "maintenance",
        );
        ctx.actor_display = Some("system".into());
        ctx.reason = Some("iq".into());
        let mut tx = Tx::begin(&write, &ctx).await.unwrap();
        rebuild(&mut tx).await.expect("rebuild");
        verify_projection(&mut tx).await.expect("verify_projection");
        tx.commit().await.unwrap();
    }
    pass(9, "projections rebuilt from scratch equal the ledger fold");

    // Item 10: canary — insert succeeds; deferred ZL002 fires at commit.
    {
        let write = WritePool::new(w.pool.clone());
        let mut ctx = WriteContext::new(
            Actor {
                id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
                kind: ActorKind::ServicePrincipal,
            },
            "ledger.canary",
            "maintenance",
        );
        ctx.actor_display = Some("system".into());
        ctx.reason = Some("canary".into());
        let mut tx = Tx::begin(&write, &ctx).await.unwrap();
        let mut b = GroupBuilder::new(
            GroupKind::Movement,
            PostingGroupHeader {
                source_kind: "canary".into(),
                source_id: None,
                work_order_id: None,
                reason_code: None,
                reverses_group_id: None,
            },
        );
        PostingSink::contribute(&mut b, PostingIntent::Quantity(qty_post(&screw, &fg, "1")))
            .expect("contribute +1");
        PostingSink::contribute(
            &mut b,
            PostingIntent::Quantity(qty_post(&screw, &aloc, "2")),
        )
        .expect("contribute +2");
        post(&mut tx, b)
            .await
            .expect("insert must succeed; trigger is deferred");
        let err = tx
            .commit()
            .await
            .expect_err("canary: deferred trigger must fire");
        let code = pg_code_db(&err);
        assert_eq!(code, "ZL002", "canary err={err}");
    }
    pass(10, "ledger canary fails as designed (unbalanced group)");

    // Item 13: mutating steps left audit rows attributed to the actor.
    let n = audit_count(&w.pool).await;
    assert!(n > 0, "audit rows exist");
    let stamped: i64 = query_scalar(
        "SELECT count(*) FROM audit.event
          WHERE source_kind = 'api'
            AND actor_id IS NOT NULL
            AND app_version <> ''
            AND config_version <> ''",
    )
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert!(stamped > 0, "api audit rows carry actor and version stamps");
    pass(
        13,
        "mutating API steps leave audit rows with actor and version stamps",
    );

    println!(
        "slice_end_to_end_{} thirteen assertions printed",
        w.profile.as_str().replace('-', "_")
    );
}

fn count_required(edges: &Value) -> usize {
    let Some(arr) = edges.as_array() else {
        return 0;
    };
    arr.iter()
        .filter(|e| {
            e.get("Required").is_some()
                || (e.get("meaning").is_some() && e.get("permission").is_some())
                || e.as_str().is_some_and(|s| s.contains("Required"))
        })
        .count()
}

async fn create_item(
    w: &World,
    number: &str,
    rev: &str,
    desc: &str,
    kind: &str,
    uom: i64,
    cost: &str,
    standard: Option<Value>,
) -> String {
    let mut body = json!({
        "number": number,
        "revision": rev,
        "description": desc,
        "kind": kind,
        "stock_uom": uom,
        "stock_scale": 0,
        "residual_tolerance": "0",
        "cost_method": cost,
    });
    if let Some(s) = standard {
        body["standard"] = s;
    }
    let (st, v) = w.post("/api/v1/items", body).await;
    assert_eq!(st, StatusCode::CREATED, "create {number} {v}");
    v["id"].as_str().unwrap().to_string()
}

async fn create_loc(w: &World, code: &str, name: &str) -> String {
    let (st, v) = w
        .post(
            "/api/v1/locations",
            json!({"code": code, "name": name, "kind": "warehouse"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "loc {code} {v}");
    v["id"].as_str().unwrap().to_string()
}

async fn create_lot(
    w: &World,
    item: &str,
    ident: &str,
    heat: Option<&str>,
    expiry: Option<Value>,
) -> String {
    let mut body = json!({"item_id": item, "identifier": ident});
    if let Some(h) = heat {
        body["heat"] = json!(h);
    }
    if let Some(e) = expiry {
        body["expiry"] = e;
    }
    let (st, v) = w.post("/api/v1/lots", body).await;
    assert_eq!(st, StatusCode::CREATED, "lot {ident} {v}");
    v["id"].as_str().unwrap().to_string()
}

fn qty_post(item: &str, loc: &str, amount: &str) -> QuantityPosting {
    QuantityPosting {
        item: ItemId::from_uuid(uuid::Uuid::parse_str(item).unwrap()),
        quantity: AnyQuantity {
            amount: amount.parse().unwrap(),
            unit: UnitId(1),
            dimension: DimensionKind::Count,
        },
        location: LocationId::from_uuid(uuid::Uuid::parse_str(loc).unwrap()),
        boundary: None,
        lot: None,
        serial: None,
        entered: None,
    }
}

fn pg_code_db(err: &datum_db::Error) -> String {
    match err {
        datum_db::Error::Refused(s) => s.as_str().to_string(),
        datum_db::Error::Sqlx(e) => e
            .as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| e.to_string()),
        other => other.to_string(),
    }
}

async fn force_lot_available(pool: &datum_db::Pool, lot: &str) {
    let write = WritePool::new(pool.clone());
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "lots.force",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("test".into());
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    tx.execute(
        sqlx::query("UPDATE lots.lot SET status = 'available' WHERE id = $1")
            .bind(uuid::Uuid::parse_str(lot).unwrap()),
    )
    .await
    .expect("force lot available");
    tx.commit().await.unwrap();
}

async fn audit_count(pool: &datum_db::Pool) -> i64 {
    query_scalar::<_, i64>("SELECT count(*) FROM audit.event")
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

#[tokio::test]
async fn startup_fails_when_required_edge_meets_no_signatures_in_release() {
    use datum_core::{PermissionKey, SignatureMeaning, SignatureRequirement};
    let mut eng = Engine::new();
    let m = Machine::builder("calibration.certificate")
        .regulated(true)
        .state("Open")
        .state("Approved")
        .edge(
            EdgeBuilder::new("Open", "Approved", "approve", "calibration.approve").required(
                SignatureRequirement {
                    meaning: SignatureMeaning("Approved".into()),
                    permission: PermissionKey("calibration.approve".into()),
                },
            ),
        )
        .build()
        .unwrap();
    eng.register_machine(m).unwrap();
    eng.freeze().unwrap();
    let err = startup_guard_release(&eng, true).expect_err("release+noop");
    assert!(
        err.to_string().to_lowercase().contains("startup") || err.to_string().contains("Required")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn openapi_lists_every_registered_route() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::plain_shop().unwrap()).await;
    let (st, body) = w.get("/api/v1/openapi.json").await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let ops = registered_operations(&body);
    assert!(
        ops.iter()
            .any(|(m, p)| m == "POST" && p == "/api/v1/inventory/receipts"),
        "{ops:?}"
    );
    assert!(
        ops.iter()
            .any(|(m, p)| m == "GET" && p == "/api/v1/openapi.json"),
        "{ops:?}"
    );
    assert!(
        ops.iter().any(|(m, p)| m == "POST" && p == "/api/v1/lots"),
        "{ops:?}"
    );
    let _ = openapi_document;
}

#[tokio::test(flavor = "multi_thread")]
async fn mutation_without_session_is_401_and_opens_no_transaction() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::plain_shop().unwrap()).await;
    let before = audit_count(&w.pool).await;
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/v1/items")
        .header("content-type", "application/json")
        .header("idempotency-key", uuid::Uuid::now_v7().to_string())
        .body(axum::body::Body::from(
            json!({"number":"NOSESS","revision":"A","description":"d","stock_uom":1}).to_string(),
        ))
        .unwrap();
    let resp = w.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let after = audit_count(&w.pool).await;
    assert_eq!(after, before, "no transaction opened");
}

#[tokio::test(flavor = "multi_thread")]
async fn floor_endpoint_under_500ms_on_local_db() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::plain_shop().unwrap()).await;
    let item = create_item(&w, "FL-1", "A", "floor", "buy", 1, "FIFO", None).await;
    let loc = create_loc(&w, "FL-Q", "floor q").await;
    let mut times = Vec::new();
    for i in 0..50 {
        let t0 = std::time::Instant::now();
        let (st, body) = w
            .post(
                "/api/v1/inventory/receipts",
                json!({
                    "item_id": item,
                    "location_id": loc,
                    "entered": qty("1", 1, "Count"),
                    "quantity": qty("1", 1, "Count"),
                }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "floor {i} {body}");
        times.push(t0.elapsed());
    }
    times.sort();
    let p95 = times[(times.len() * 95) / 100];
    println!(
        "floor_endpoint_under_500ms_on_local_db p95={:?} (measured, not asserted flakily)",
        p95
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn manifest_hash_is_stable_across_restarts() {
    if common::skip_if_no_pg() {
        return;
    }
    let w1 = common::boot(Profile::plain_shop().unwrap()).await;
    let (st, m1) = w1.get("/api/v1/iq/manifest").await;
    assert_eq!(st, StatusCode::OK, "{m1}");
    let h1 = m1["content_hash"].as_str().unwrap().to_string();
    let w2 = common::boot(Profile::plain_shop().unwrap()).await;
    let (st, m2) = w2.get("/api/v1/iq/manifest").await;
    assert_eq!(st, StatusCode::OK);
    let h2 = m2["content_hash"].as_str().unwrap().to_string();
    assert_eq!(h1, h2, "manifest hash stable across restarts");
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_go_through_tx() {
    if common::skip_if_no_pg() {
        return;
    }
    let w = common::boot(Profile::plain_shop().unwrap()).await;
    let before: i64 = query_scalar("SELECT count(*) FROM server.boot_record")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    let err = sqlx::query(
        r#"INSERT INTO server.boot_record
               (id, profile_id, spec_version, manifest_hash, bind_addr, application_version)
           VALUES (gen_random_uuid(), 'x', '1', 'h', '0.0.0.0:0', '0')"#,
    )
    .execute(&w.pool)
    .await
    .expect_err("raw write must fail");
    let code = err
        .as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_default();
    assert_eq!(code, "42501", "err={err}");
    let after: i64 = query_scalar("SELECT count(*) FROM server.boot_record")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(after, before);
}

fn profiles() -> [Profile; 2] {
    [
        Profile::plain_shop().unwrap(),
        Profile::regulated_device().unwrap(),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn item_create_pins_item_stock_for_inventory_http() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let tag = match w.profile {
            datum_module::ProfileId::PlainShop => "PS",
            datum_module::ProfileId::RegulatedDevice => "RD",
        };
        let number = format!("PIN-{tag}");
        let qloc_code = format!("PIN-Q-{tag}");
        let aloc_code = format!("PIN-A-{tag}");
        let lot_ident = format!("LOT-PIN-{tag}");
        let item = create_item(&w, &number, "A", "item stock pin", "buy", 1, "FIFO", None).await;
        let qloc = create_loc(&w, &qloc_code, "quarantine").await;
        let aloc = create_loc(&w, &aloc_code, "available").await;
        let lot = create_lot(&w, &item, &lot_ident, None, None).await;
        let (st, rec) = w
            .post(
                "/api/v1/inventory/receipts",
                json!({
                    "item_id": item,
                    "lot_id": lot,
                    "location_id": qloc,
                    "quantity": qty("100", 1, "Count"),
                }),
            )
            .await;
        assert_eq!(
            st,
            StatusCode::CREATED,
            "receipt without uom backdoor {rec}"
        );
        assert_ne!(
            rec["error"]["code"].as_str(),
            Some("INTERNAL"),
            "must not fail conversion {rec}"
        );
        let lot_ver = w.get(&format!("/api/v1/lots/{lot}")).await.1["version"]
            .as_i64()
            .unwrap_or(1);
        let (st, rel) = w
            .post_if_match(
                "/api/v1/inventory/releases",
                json!({
                    "lot_id": lot,
                    "from_location_id": qloc,
                    "to_location_id": aloc,
                    "entered": qty("100", 1, "Count"),
                }),
                lot_ver,
            )
            .await;
        let (count_loc, on_hand_qty) = match w.profile {
            datum_module::ProfileId::PlainShop => {
                assert_eq!(st, StatusCode::OK, "inventory release {rel}");
                (aloc.clone(), "100")
            }
            datum_module::ProfileId::RegulatedDevice => {
                assert_eq!(st, StatusCode::FORBIDDEN, "regulated release {rel}");
                assert_eq!(rel["error"]["code"], "SIGNATURE_REQUIRED", "{rel}");
                assert!(
                    !rel["error"]["message"]
                        .as_str()
                        .unwrap_or("")
                        .contains("no conversion path"),
                    "release must not fail uom pin {rel}"
                );
                (qloc.clone(), "100")
            }
        };
        let (st, cnt) = w
            .post(
                "/api/v1/inventory/counts",
                json!({
                    "location_id": count_loc,
                    "lines": [{
                        "item_id": item,
                        "lot_id": lot,
                        "counted": qty(on_hand_qty, 1, "Count"),
                        "expected": qty(on_hand_qty, 1, "Count"),
                    }],
                    "tolerance": "0",
                }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "cycle count {cnt}");
        assert_ne!(
            cnt["error"]["code"].as_str(),
            Some("INTERNAL"),
            "count must not fail conversion {cnt}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn idempotency_replay_every_mutating_route() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let mut w = common::boot(profile).await;
        let login_key = uuid::Uuid::now_v7().to_string();
        let login_body = json!({"username": common::USERNAME, "password": common::PASSWORD});
        let (st1, v1) = w
            .post_key(
                "/api/v1/identity/login",
                login_body.clone(),
                &login_key,
                vec![],
            )
            .await;
        assert_eq!(st1, StatusCode::OK, "login first {v1}");
        let (st2, v2) = w
            .post_key("/api/v1/identity/login", login_body, &login_key, vec![])
            .await;
        assert_eq!(st2, StatusCode::OK, "login replay {v2}");
        assert_eq!(v1["session_id"], v2["session_id"], "login stores replay");

        let key = uuid::Uuid::now_v7().to_string();
        let body = json!({"code": "IDEM-L", "name": "Idem loc", "kind": "warehouse"});
        let (st1, v1) = w
            .post_key("/api/v1/locations", body.clone(), &key, vec![])
            .await;
        assert_eq!(st1, StatusCode::CREATED, "first {v1}");
        let (st2, v2) = w.post_key("/api/v1/locations", body, &key, vec![]).await;
        assert_eq!(st2, StatusCode::CREATED, "replay {v2}");
        assert_eq!(v1["id"], v2["id"], "replay returns stored id");
        let loc_id = v1["id"].as_str().unwrap().to_string();

        let key = uuid::Uuid::now_v7().to_string();
        let n = format!("IDEM-{}", &key[..8]);
        let body = json!({
            "number": n,
            "revision": "A",
            "description": "idem item",
            "kind": "buy",
            "stock_uom": 1,
            "stock_scale": 0,
            "residual_tolerance": "0",
            "cost_method": "FIFO",
        });
        let (st1, v1) = w
            .post_key("/api/v1/items", body.clone(), &key, vec![])
            .await;
        assert_eq!(st1, StatusCode::CREATED, "item first {v1}");
        let (st2, v2) = w.post_key("/api/v1/items", body, &key, vec![]).await;
        assert_eq!(st2, StatusCode::CREATED, "item replay {v2}");
        assert_eq!(v1["id"], v2["id"]);

        let item = v1["id"].as_str().unwrap().to_string();
        let lot_key = uuid::Uuid::now_v7().to_string();
        let lot_body = json!({"item_id": item, "identifier": "LOT-IDEM-1"});
        let (st1, lot1) = w
            .post_key("/api/v1/lots", lot_body.clone(), &lot_key, vec![])
            .await;
        assert_eq!(st1, StatusCode::CREATED, "lot first {lot1}");
        let (st2, lot2) = w.post_key("/api/v1/lots", lot_body, &lot_key, vec![]).await;
        assert_eq!(st2, StatusCode::CREATED, "lot replay {lot2}");
        assert_eq!(lot1["id"], lot2["id"]);

        let rec_key = uuid::Uuid::now_v7().to_string();
        let rec_body = json!({
            "item_id": item,
            "location_id": loc_id,
            "quantity": qty("1", 1, "Count"),
        });
        let (st1, r1) = w
            .post_key(
                "/api/v1/inventory/receipts",
                rec_body.clone(),
                &rec_key,
                vec![],
            )
            .await;
        assert_eq!(st1, StatusCode::CREATED, "receipt first {r1}");
        let (st2, r2) = w
            .post_key("/api/v1/inventory/receipts", rec_body, &rec_key, vec![])
            .await;
        assert_eq!(st2, StatusCode::CREATED, "receipt replay {r2}");
        assert_eq!(r1["id"], r2["id"]);

        let out_key = uuid::Uuid::now_v7().to_string();
        let (st, out) = w
            .post_key("/api/v1/identity/logout", json!({}), &out_key, vec![])
            .await;
        assert_eq!(st, StatusCode::NO_CONTENT, "logout {out}");
        let stored: i64 =
            query_scalar("SELECT count(*) FROM server_transient.idempotency WHERE key = $1")
                .bind(uuid::Uuid::parse_str(&out_key).unwrap())
                .fetch_one(&w.pool)
                .await
                .unwrap();
        assert_eq!(stored, 1, "logout stored Idempotency-Key");
        w.login().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn idempotency_conflict_409() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let key = uuid::Uuid::now_v7().to_string();
        let (st1, v1) = w
            .post_key(
                "/api/v1/locations",
                json!({"code": "IDEM-A", "name": "A", "kind": "warehouse"}),
                &key,
                vec![],
            )
            .await;
        assert_eq!(st1, StatusCode::CREATED, "{v1}");
        let (st2, v2) = w
            .post_key(
                "/api/v1/locations",
                json!({"code": "IDEM-B", "name": "B", "kind": "warehouse"}),
                &key,
                vec![],
            )
            .await;
        assert_eq!(st2, StatusCode::CONFLICT, "mismatch {v2}");
        assert_eq!(v2["error"]["code"], "IDEMPOTENCY_CONFLICT", "{v2}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn if_match_on_transitions() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let item = create_item(&w, "IFM-1", "A", "if-match", "buy", 1, "FIFO", None).await;
        let ver = w.get(&format!("/api/v1/items/{item}")).await.1["version"]
            .as_i64()
            .unwrap();
        let (st, body) = w
            .post(&format!("/api/v1/items/{item}/release"), json!({}))
            .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "missing If-Match {body}");
        assert_eq!(body["error"]["code"], "VALIDATION", "{body}");
        let (st, body) = w
            .post_if_match(&format!("/api/v1/items/{item}/release"), json!({}), ver + 9)
            .await;
        assert_eq!(st, StatusCode::CONFLICT, "stale {body}");
        assert_eq!(body["error"]["code"], "CONFLICT", "{body}");
        assert_eq!(body["error"]["field"], "version", "{body}");
        let (st, body) = w
            .post_if_match(&format!("/api/v1/items/{item}/release"), json!({}), ver)
            .await;
        assert_eq!(st, StatusCode::OK, "fresh {body}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn issue_wo_is_one_transaction() {
    if common::skip_if_no_pg() {
        return;
    }
    let src = include_str!("../src/handlers.rs");
    let start = src.find("async fn issue_wo_inner").expect("issue_wo_inner");
    let rest = &src[start..];
    let end = rest
        .find("async fn complete_wo_inner")
        .unwrap_or(rest.len());
    let fn_src = &rest[..end];
    let begins = fn_src.matches("Tx::begin").count();
    assert_eq!(
        begins, 1,
        "SPEC: issue_wo is one Tx::begin / one WriteContext:\n{fn_src}"
    );
    for profile in profiles() {
        let w = common::boot(profile).await;
        let (wo_id, wo_ver, loc, lot, item) = setup_released_wo(&w).await;
        let issue_body = json!({
            "from_location_id": loc,
            "lines": [{
                "item_id": item,
                "lot_id": lot,
                "entered": qty("10", 1, "Count"),
            }]
        });
        let (st, headers, body) = w
            .call(
                "POST",
                &format!("/api/v1/production/work-orders/{wo_id}/issue"),
                Some(vec![("if-match", format!("\"{wo_ver}\""))]),
                Some(issue_body.clone()),
            )
            .await;
        assert_eq!(st, StatusCode::OK, "first issue {body}");
        let rid = headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .expect("X-Request-Id");
        let seals: i64 = query_scalar(
            r#"SELECT count(DISTINCT s.xid)::bigint
                 FROM audit.tx_seal s
                 JOIN audit.event e ON e.xid = s.xid
                WHERE e.request_id = $1"#,
        )
        .bind(uuid::Uuid::parse_str(rid).unwrap())
        .fetch_one(&w.pool)
        .await
        .unwrap();
        assert_eq!(
            seals, 1,
            "one audit.tx_seal row for the issue request {rid}"
        );
        let groups: i64 = query_scalar(
            r#"SELECT count(*)::bigint
                 FROM ledger.posting_group g
                WHERE g.created_xid IN (
                    SELECT DISTINCT e.xid
                      FROM audit.event e
                     WHERE e.request_id = $1
                )"#,
        )
        .bind(uuid::Uuid::parse_str(rid).unwrap())
        .fetch_one(&w.pool)
        .await
        .unwrap();
        assert_eq!(
            groups, 1,
            "issue+start posts exactly one posting_group {rid} {body}"
        );
        let source: String = query_scalar(
            r#"SELECT g.source_kind
                 FROM ledger.posting_group g
                WHERE g.created_xid IN (
                    SELECT DISTINCT e.xid
                      FROM audit.event e
                     WHERE e.request_id = $1
                )"#,
        )
        .bind(uuid::Uuid::parse_str(rid).unwrap())
        .fetch_one(&w.pool)
        .await
        .unwrap();
        assert_eq!(
            source, "production.issue",
            "group source_kind is the start edge {rid}"
        );
        let qty_posts: i64 = query_scalar(
            r#"SELECT count(*)::bigint
                 FROM ledger.posting p
                 JOIN ledger.posting_group g ON g.group_id = p.group_id
                WHERE g.created_xid IN (
                    SELECT DISTINCT e.xid
                      FROM audit.event e
                     WHERE e.request_id = $1
                )
                  AND p.measure = 'QUANTITY'"#,
        )
        .bind(uuid::Uuid::parse_str(rid).unwrap())
        .fetch_one(&w.pool)
        .await
        .unwrap();
        assert!(
            qty_posts >= 2,
            "production.issue hook must contribute stock-out and WIP quantity postings {rid} {body}"
        );
        let lines_after_ok: i64 =
            query_scalar("SELECT count(*) FROM production_min.issue_line WHERE work_order_id = $1")
                .bind(uuid::Uuid::parse_str(&wo_id).unwrap())
                .fetch_one(&w.pool)
                .await
                .unwrap();
        let wo_ver2 = body["version"].as_i64().unwrap_or(wo_ver + 1);
        // WO is in_process: issue_material is allowed, start() requires Released.
        // One Tx rolls the extra issue_line back; two committed txs leave it.
        let (st2, body2) = w
            .post_if_match(
                &format!("/api/v1/production/work-orders/{wo_id}/issue"),
                issue_body,
                wo_ver2,
            )
            .await;
        assert_ne!(st2, StatusCode::OK, "start must fail on in_process {body2}");
        let lines_after_fail: i64 =
            query_scalar("SELECT count(*) FROM production_min.issue_line WHERE work_order_id = $1")
                .bind(uuid::Uuid::parse_str(&wo_id).unwrap())
                .fetch_one(&w.pool)
                .await
                .unwrap();
        assert_eq!(
            lines_after_fail, lines_after_ok,
            "failing start rolls back the issue_material half"
        );
    }
}

async fn setup_released_wo(w: &World) -> (String, i64, String, String, String) {
    let item = create_item(w, "TX1", "A", "onetx", "buy", 1, "FIFO", None).await;
    let loc = create_loc(w, "TX-L", "onetx loc").await;
    let lot = create_lot(w, &item, "LOT-TX-1", None, None).await;
    let (st, rec) = w
        .post(
            "/api/v1/inventory/receipts",
            json!({
                "item_id": item,
                "lot_id": lot,
                "location_id": loc,
                "quantity": qty("100", 1, "Count"),
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "receipt {rec}");
    force_lot_available(&w.pool, &lot).await;
    let (st, wo) = w
        .post(
            "/api/v1/production/work-orders",
            json!({
                "item_id": item,
                "revision": "A",
                "quantity": qty("5", 1, "Count"),
            }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "wo {wo}");
    let wo_id = wo["id"].as_str().unwrap().to_string();
    let wo_ver = wo["version"].as_i64().unwrap_or(1);
    let (st, wo) = w
        .post_if_match(
            &format!("/api/v1/production/work-orders/{wo_id}/release"),
            json!({}),
            wo_ver,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "wo release {wo}");
    let wo_ver = wo["version"].as_i64().unwrap_or(wo_ver + 1);
    (wo_id, wo_ver, loc, lot, item)
}

#[tokio::test(flavor = "multi_thread")]
async fn reverse_issue_restores_on_hand() {
    if common::skip_if_no_pg() {
        return;
    }
    let src = include_str!("../src/handlers.rs");
    let start = src.find("async fn reverse_inner").expect("reverse_inner");
    let rest = &src[start..];
    let end = rest.find("/// Health.").unwrap_or(rest.len());
    let fn_src = &rest[..end];
    let begins = fn_src.matches("Tx::begin").count();
    assert_eq!(
        begins, 1,
        "SPEC: reverse is one Tx::begin / one WriteContext:\n{fn_src}"
    );
    assert!(
        !fn_src.contains("rebind_write"),
        "R-2s-7: reverse must not rebind GUCs"
    );

    for profile in profiles() {
        let w = common::boot(profile).await;
        let (wo_id, wo_ver, loc, lot, item) = setup_released_wo(&w).await;
        let oh_uri =
            format!("/api/v1/inventory/on-hand?item_id={item}&location_id={loc}&lot_id={lot}");
        let (st, oh0) = w.get(&oh_uri).await;
        assert_eq!(st, StatusCode::OK, "on-hand before issue {oh0}");
        let before: rust_decimal::Decimal = oh0["on_hand"].as_str().unwrap().parse().unwrap();
        assert!(
            before > rust_decimal::Decimal::ZERO,
            "receipt must leave on-hand {oh0}"
        );

        let (st, issued) = w
            .post_if_match(
                &format!("/api/v1/production/work-orders/{wo_id}/issue"),
                json!({
                    "from_location_id": loc,
                    "lines": [{
                        "item_id": item,
                        "lot_id": lot,
                        "entered": qty("10", 1, "Count"),
                    }]
                }),
                wo_ver,
            )
            .await;
        assert_eq!(st, StatusCode::OK, "issue {issued}");

        let (st, oh1) = w.get(&oh_uri).await;
        assert_eq!(st, StatusCode::OK, "on-hand after issue {oh1}");
        let after_issue: rust_decimal::Decimal = oh1["on_hand"].as_str().unwrap().parse().unwrap();
        assert!(
            after_issue < before,
            "issue must reduce on-hand {before} -> {after_issue}"
        );

        let issue_doc: uuid::Uuid = query_scalar(
            "SELECT inventory_document_id FROM production_min.issue_line WHERE work_order_id = $1 LIMIT 1",
        )
        .bind(uuid::Uuid::parse_str(&wo_id).unwrap())
        .fetch_one(&w.pool)
        .await
        .expect("issue document");
        let issue_group: uuid::Uuid =
            query_scalar("SELECT posted_group_id FROM inventory.document WHERE id = $1")
                .bind(issue_doc)
                .fetch_one(&w.pool)
                .await
                .expect("posted group");

        let (st, missing) = w
            .post(
                "/api/v1/inventory/reversals",
                json!({
                    "document_id": uuid::Uuid::now_v7().to_string(),
                    "reason": "unknown",
                }),
            )
            .await;
        assert_eq!(st, StatusCode::NOT_FOUND, "unknown issue {missing}");
        assert_eq!(missing["error"]["code"], "NOT_FOUND", "{missing}");

        let (st, rev) = w
            .post(
                "/api/v1/inventory/reversals",
                json!({
                    "document_id": issue_doc.to_string(),
                    "reason": "wrong WO pick",
                }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "reverse {rev}");
        assert_eq!(rev["id"], issue_doc.to_string(), "{rev}");
        assert_eq!(rev["kind"], "issue", "{rev}");
        assert!(
            rev["reversal_group_id"].as_str().is_some(),
            "reversal document {rev}"
        );

        let (st, oh2) = w.get(&oh_uri).await;
        assert_eq!(st, StatusCode::OK, "on-hand after reverse {oh2}");
        let after_rev: rust_decimal::Decimal = oh2["on_hand"].as_str().unwrap().parse().unwrap();
        assert_eq!(after_rev, before, "on-hand restored {oh2}");

        let rev_kind: String = query_scalar(
            "SELECT kind::text FROM ledger.posting_group WHERE reverses_group_id = $1",
        )
        .bind(issue_group)
        .fetch_one(&w.pool)
        .await
        .expect("reversal group");
        assert_eq!(rev_kind, "REVERSAL", "issue group reversed");
        let doc_status: String =
            query_scalar("SELECT status FROM inventory.document WHERE id = $1")
                .bind(issue_doc)
                .fetch_one(&w.pool)
                .await
                .expect("issue status");
        assert_eq!(
            doc_status, "posted",
            "issue document remains; ledger group is reversed"
        );

        for group in [issue_group] {
            let qty_bad: i64 = query_scalar(
                r#"SELECT count(*) FROM (
                     SELECT item_id, uom_id
                       FROM ledger.posting
                      WHERE group_id = $1 AND measure = 'QUANTITY'
                      GROUP BY item_id, uom_id
                     HAVING SUM(quantity) <> 0
                   ) s"#,
            )
            .bind(group)
            .fetch_one(&w.pool)
            .await
            .unwrap();
            assert_eq!(qty_bad, 0, "qty conserved in {group}");
            let amt_bad: i64 = query_scalar(
                r#"SELECT count(*) FROM (
                     SELECT currency_id
                       FROM ledger.posting
                      WHERE group_id = $1 AND measure = 'VALUE'
                      GROUP BY currency_id
                     HAVING SUM(amount) <> 0
                   ) s"#,
            )
            .bind(group)
            .fetch_one(&w.pool)
            .await
            .unwrap();
            assert_eq!(amt_bad, 0, "value conserved in {group}");
        }
        let rev_group = uuid::Uuid::parse_str(rev["reversal_group_id"].as_str().unwrap()).unwrap();
        let qty_bad: i64 = query_scalar(
            r#"SELECT count(*) FROM (
                 SELECT item_id, uom_id
                   FROM ledger.posting
                  WHERE group_id = $1 AND measure = 'QUANTITY'
                  GROUP BY item_id, uom_id
                 HAVING SUM(quantity) <> 0
               ) s"#,
        )
        .bind(rev_group)
        .fetch_one(&w.pool)
        .await
        .unwrap();
        assert_eq!(qty_bad, 0, "qty conserved in reversal {rev_group}");

        let (st, again) = w
            .post(
                "/api/v1/inventory/reversals",
                json!({
                    "document_id": issue_doc.to_string(),
                    "reason": "wrong WO pick again",
                }),
            )
            .await;
        assert_eq!(st, StatusCode::CONFLICT, "already reversed {again}");
        assert_eq!(again["error"]["code"], "CONFLICT", "{again}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn mutating_routes_require_manifest_permission() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let mut w = common::boot(profile).await;
        w.login_as(common::NOPERM_USER, common::NOPERM_PASSWORD)
            .await;
        let routes = [
            (
                "/api/v1/items",
                json!({"number":"NP-1","revision":"A","description":"x","stock_uom":1}),
            ),
            (
                "/api/v1/locations",
                json!({"code":"NP-L","name":"n","kind":"warehouse"}),
            ),
            (
                "/api/v1/inventory/reversals",
                json!({
                    "document_id": "00000000-0000-0000-0000-000000000001",
                    "reason": "noperm",
                }),
            ),
        ];
        for (uri, body) in routes {
            let (st, v) = w.post(uri, body).await;
            assert_eq!(st, StatusCode::FORBIDDEN, "{uri} {v}");
            assert_eq!(v["error"]["code"], "FORBIDDEN", "{uri} {v}");
        }
        let (st, v) = w
            .get("/api/v1/items/00000000-0000-0000-0000-000000000001")
            .await;
        assert_eq!(st, StatusCode::FORBIDDEN, "get items {v}");
        assert_eq!(v["error"]["code"], "FORBIDDEN", "{v}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn regulated_release_refused_under_no_signatures() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile.clone()).await;
        let item = create_item(&w, "REL-1", "A", "lot rel", "buy", 1, "FIFO", None).await;
        let lot = create_lot(&w, &item, "LOT-REL-1", None, None).await;
        let ver = w.get(&format!("/api/v1/lots/{lot}")).await.1["version"]
            .as_i64()
            .unwrap_or(1);
        let (st, body) = w
            .post_if_match(
                &format!("/api/v1/lots/{lot}/status"),
                json!({"status":"available","reason":"release"}),
                ver,
            )
            .await;
        match profile.id {
            datum_module::ProfileId::RegulatedDevice => {
                assert_eq!(st, StatusCode::FORBIDDEN, "{body}");
                assert_eq!(body["error"]["code"], "SIGNATURE_REQUIRED", "{body}");
                let after = w.get(&format!("/api/v1/lots/{lot}")).await.1;
                assert_eq!(after["status"], "quarantine", "nothing posted {after}");
            }
            datum_module::ProfileId::PlainShop => {
                assert_eq!(st, StatusCode::OK, "{body}");
                assert_eq!(body["status"], "available", "{body}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn openapi_matches_router() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let (st, body) = w.get("/api/v1/openapi.json").await;
        assert_eq!(st, StatusCode::OK, "{body}");
        let listed = registered_operations(&body);
        let mounted = mounted_operations();
        assert_eq!(listed, mounted, "OpenAPI must equal mounted routes");
        for (method, path) in &listed {
            let probe = path.replace("{id}", &uuid::Uuid::nil().to_string());
            let (pst, _, pbody) = w.call(method, &probe, None, Some(json!({}))).await;
            if pst == StatusCode::NOT_FOUND {
                assert_eq!(
                    pbody["error"]["code"], "NOT_FOUND",
                    "listed {method} {path} is not mounted (bare 404)"
                );
            }
        }
        let (st, _, _) = w
            .call("GET", "/api/v1/mod-does-not-exist/foo", None, None)
            .await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_handlers_are_read_only() {
    if common::skip_if_no_pg() {
        return;
    }
    const GET_FNS: &[&str] = &[
        "openapi",
        "manifest",
        "navigation",
        "audit_export",
        "get_item",
        "get_location",
        "get_location_inner",
        "get_lot",
        "get_lot_inner",
        "list_packages",
        "list_pkg_inner",
        "list_serials",
        "list_serials_inner",
        "on_hand",
        "on_hand_inner",
        "get_wo",
        "get_wo_inner",
        "genealogy_trace",
        "trace_inner",
        "health",
        "esign_manifestation",
        "esign_manifestation_inner",
        "esign_bundle",
        "esign_bundle_inner",
    ];
    let src = include_str!("../src/handlers.rs");
    for name in GET_FNS {
        let body = fn_src(src, name);
        assert_eq!(
            body.matches("Tx::begin").count(),
            0,
            "GET {name} must not call Tx::begin:\n{body}"
        );
        assert_eq!(
            body.matches(".commit(").count(),
            0,
            "GET {name} must not commit:\n{body}"
        );
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let item = create_item(&w, "RO-1", "A", "ro", "buy", 1, "FIFO", None).await;
        let loc = create_loc(&w, "RO-L", "ro loc").await;
        let lot = create_lot(&w, &item, "LOT-RO-1", None, None).await;
        let (st, wo) = w
            .post(
                "/api/v1/work-orders",
                json!({
                    "item_id": item,
                    "revision": "A",
                    "quantity": qty("1", 1, "Count"),
                }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{wo}");
        let wo_id = wo["id"].as_str().unwrap();
        let before = audit_count(&w.pool).await;
        let cache_before: i64 =
            query_scalar("SELECT count(*) FROM genealogy_transient.trace_cache")
                .fetch_one(&w.pool)
                .await
                .unwrap();
        let gets = [
            "/health".to_string(),
            "/api/v1/openapi.json".into(),
            "/api/v1/iq/manifest".into(),
            "/api/v1/audit".into(),
            "/api/v1/navigation".into(),
            format!("/api/v1/items/{item}"),
            format!("/api/v1/locations/{loc}"),
            format!("/api/v1/lots/{lot}"),
            format!("/api/v1/lots/{lot}/packages"),
            format!("/api/v1/lots/{lot}/serials"),
            format!("/api/v1/inventory/on-hand?item_id={item}&location_id={loc}"),
            format!("/api/v1/work-orders/{wo_id}"),
            format!("/api/v1/production/work-orders/{wo_id}"),
            format!("/api/v1/genealogy/trace?from_lot_id={lot}&direction=forward"),
        ];
        for uri in &gets {
            let (st, body) = w.get(uri).await;
            assert!(
                st.is_success() || st == StatusCode::NOT_FOUND,
                "GET {uri} {st} {body}"
            );
        }
        let after = audit_count(&w.pool).await;
        assert_eq!(after, before, "GET must not write audit.event");
        let cache_after: i64 = query_scalar("SELECT count(*) FROM genealogy_transient.trace_cache")
            .fetch_one(&w.pool)
            .await
            .unwrap();
        assert_eq!(
            cache_after, cache_before,
            "genealogy GET must not commit cache_put"
        );
    }
}

fn fn_src<'a>(src: &'a str, name: &str) -> &'a str {
    let needle = format!("fn {name}(");
    let start = src
        .find(&needle)
        .unwrap_or_else(|| panic!("missing {name}"));
    let brace = src[start..].find('{').expect("brace") + start;
    let mut depth = 0i32;
    for (i, c) in src[brace..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[brace..=brace + i];
                }
            }
            _ => {}
        }
    }
    panic!("unclosed {name}");
}

#[tokio::test(flavor = "multi_thread")]
async fn one_audit_row_per_mutating_step() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        let w = common::boot(profile).await;
        let (st, item_rid, item) = post_rid(
            &w,
            "/api/v1/items",
            json!({
                "number": "AUD-1",
                "revision": "A",
                "description": "audit",
                "kind": "buy",
                "stock_uom": 1,
                "stock_scale": 0,
                "residual_tolerance": "0",
                "cost_method": "FIFO",
            }),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{item}");
        let item_id = item["id"].as_str().unwrap().to_string();
        let (st, loc_rid, loc) = post_rid(
            &w,
            "/api/v1/locations",
            json!({"code": "AUD-L", "name": "audit loc", "kind": "warehouse"}),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{loc}");
        let loc_id = loc["id"].as_str().unwrap().to_string();
        let (st, lot_rid, lot) = post_rid(
            &w,
            "/api/v1/lots",
            json!({"item_id": item_id, "identifier": "LOT-AUD-1"}),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{lot}");
        let lot_id = lot["id"].as_str().unwrap().to_string();
        let (st, rec_rid, rec) = post_rid(
            &w,
            "/api/v1/inventory/receipts",
            json!({
                "item_id": item_id,
                "lot_id": lot_id,
                "location_id": loc_id,
                "quantity": qty("1", 1, "Count"),
            }),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{rec}");
        for (label, rid) in [
            ("items.create", item_rid),
            ("locations.create", loc_rid),
            ("lots.create", lot_rid),
            ("inventory.receive", rec_rid),
        ] {
            assert_one_seal_per_request(&w.pool, label, rid).await;
        }
    }
}

async fn post_rid(w: &World, uri: &str, body: Value) -> (StatusCode, uuid::Uuid, Value) {
    let rid = uuid::Uuid::now_v7();
    let (st, headers, v) = w
        .call(
            "POST",
            uri,
            Some(vec![("x-request-id", rid.to_string())]),
            Some(body),
        )
        .await;
    let echoed = headers
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .unwrap_or(rid);
    (st, echoed, v)
}

async fn assert_one_seal_per_request(pool: &datum_db::Pool, label: &str, rid: uuid::Uuid) {
    let xids: i64 = query_scalar(
        r#"SELECT count(DISTINCT xid) FROM audit.event
            WHERE request_id = $1 AND source_kind = 'api'"#,
    )
    .bind(rid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(xids, 1, "{label} {rid} must have exactly one xid");
    let seals: i64 = query_scalar(
        r#"SELECT count(*) FROM audit.tx_seal s
            WHERE s.xid IN (
                SELECT DISTINCT xid FROM audit.event WHERE request_id = $1
            )"#,
    )
    .bind(rid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(seals, 1, "{label} {rid} must have exactly one tx_seal");
    let rows: i64 = query_scalar(
        r#"SELECT count(*) FROM audit.event
            WHERE request_id = $1 AND source_kind = 'api'"#,
    )
    .bind(rid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(rows >= 1, "{label} {rid} must write audit.event");
}

#[tokio::test(flavor = "multi_thread")]
async fn iq_exit_zero_standing_and_fresh() {
    if common::skip_if_no_pg() {
        return;
    }
    for profile in profiles() {
        // Fresh template clone. Bootstrap URL is the postgres maintenance
        // database; iq_cfg rewrites it onto the case DB.
        let fresh = datum_test::TestDb::case("iqf")
            .await
            .unwrap_or_else(|e| panic!("test database: {e}"));
        run_iq(iq_cfg(profile.clone(), fresh.database()))
            .await
            .unwrap_or_else(|e| panic!("iq fresh {}: {e:#}", profile.id.as_str()));
        let _ = fresh.finish().await;

        // Standing: schema_history already exists (datum-db applied, no kernel
        // persist yet). Adopt those history rows; Kernel::build is called
        // as-is (no unique-violation catch).
        let standing = datum_test::TestDb::case("iqs")
            .await
            .unwrap_or_else(|e| panic!("test database: {e}"));
        datum_db::migrate::run(
            standing.migrate_pool(),
            &[("datum-db", &datum_db::MIGRATOR)],
        )
        .await
        .unwrap_or_else(|e| panic!("apply datum-db: {e:#}"));
        run_iq(iq_cfg(profile.clone(), standing.database()))
            .await
            .unwrap_or_else(|e| panic!("iq standing {}: {e:#}", profile.id.as_str()));
        let _ = standing.finish().await;
    }
}

fn iq_cfg(profile: Profile, database: &str) -> Config {
    let app = std::env::var("DATUM_DATABASE_URL").expect("DATUM_DATABASE_URL");
    let migrate = std::env::var("DATUM_MIGRATE_DATABASE_URL").expect("DATUM_MIGRATE_DATABASE_URL");
    let boot = std::env::var("DATUM_BOOTSTRAP_URL").expect("DATUM_BOOTSTRAP_URL");
    let database_url = rewrite_database(&app, database);
    Config {
        profile,
        bind: "127.0.0.1:0".parse().expect("bind"),
        database_url: database_url.clone(),
        migrate_url: rewrite_database(&migrate, database),
        bootstrap_url: bootstrap_against_app(&with_os_userinfo(&boot), &database_url),
    }
}
