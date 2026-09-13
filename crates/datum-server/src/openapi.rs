//! OpenAPI document generated from mounted routes (docs/10 §6).
//!
//! Source of truth is [`MOUNTED`]: every listed path+method is on the axum
//! router and every mounted route is listed. Module `openapi_document`s are
//! not merged wholesale (they advertise unmounted extras such as inventory
//! issues). `datum_mod_lots` does not export `openapi_document`; lots paths
//! below are the mounted subset (charter: listed ⇒ mounted).

use serde_json::{Value, json};

use crate::boot::AppState;

/// One mounted HTTP operation. Keep in sync with [`crate::http::router`].
#[derive(Clone, Copy)]
pub struct Mounted {
    /// HTTP method.
    pub method: &'static str,
    /// Path.
    pub path: &'static str,
    /// OpenAPI operationId.
    pub operation_id: &'static str,
    /// Manifest permission (empty = unauthenticated).
    pub permission: &'static str,
    /// State-transition POST (`If-Match` + optional signature).
    pub state_transition: bool,
}

/// Every route the axum router serves (except the catch-all 404).
pub const MOUNTED: &[Mounted] = &[
    Mounted {
        method: "GET",
        path: "/health",
        operation_id: "health",
        permission: "",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/openapi.json",
        operation_id: "getOpenApi",
        permission: "",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/iq/manifest",
        operation_id: "getValidationManifest",
        permission: "validation.manifest.read",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/audit",
        operation_id: "exportAudit",
        permission: "audit.export",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/navigation",
        operation_id: "getNavigation",
        permission: "identity.session",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/identity/login",
        operation_id: "login",
        permission: "",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/identity/logout",
        operation_id: "logout",
        permission: "identity.session",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/items",
        operation_id: "createItem",
        permission: "items.edit",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/items/{id}",
        operation_id: "getItem",
        permission: "items.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/items/{id}/release",
        operation_id: "releaseItem",
        permission: "items.release",
        state_transition: true,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/locations",
        operation_id: "createLocation",
        permission: "locations.edit",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/locations/{id}",
        operation_id: "getLocation",
        permission: "locations.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/lots",
        operation_id: "createLot",
        permission: "lots.edit",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/lots/{id}",
        operation_id: "getLot",
        permission: "lots.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/lots/{id}/status",
        operation_id: "setLotStatus",
        permission: "lots.release",
        state_transition: true,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/lots/{id}/packages",
        operation_id: "listPackages",
        permission: "lots.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/lots/{id}/packages",
        operation_id: "createPackage",
        permission: "lots.edit",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/lots/{id}/serials",
        operation_id: "listSerials",
        permission: "lots.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/inventory/receipts",
        operation_id: "createReceipt",
        permission: "inventory.receive",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/inventory/releases",
        operation_id: "releaseFromQuarantine",
        permission: "lots.release",
        state_transition: true,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/inventory/counts",
        operation_id: "createCount",
        permission: "inventory.count",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/inventory/reversals",
        operation_id: "reverseIssue",
        permission: "inventory.adjust",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/inventory/on-hand",
        operation_id: "getOnHand",
        permission: "inventory.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/work-orders",
        operation_id: "createWorkOrder",
        permission: "production.create",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/work-orders/{id}",
        operation_id: "getWorkOrder",
        permission: "production.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/work-orders/{id}/release",
        operation_id: "releaseWorkOrder",
        permission: "production.release",
        state_transition: true,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/work-orders/{id}/issue",
        operation_id: "issueWorkOrder",
        permission: "production.issue",
        state_transition: true,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/work-orders/{id}/complete",
        operation_id: "completeWorkOrder",
        permission: "production.complete",
        state_transition: true,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/production/work-orders",
        operation_id: "createWorkOrderNs",
        permission: "production.create",
        state_transition: false,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/production/work-orders/{id}",
        operation_id: "getWorkOrderNs",
        permission: "production.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/production/work-orders/{id}/release",
        operation_id: "releaseWorkOrderNs",
        permission: "production.release",
        state_transition: true,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/production/work-orders/{id}/issue",
        operation_id: "issueWorkOrderNs",
        permission: "production.issue",
        state_transition: true,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/production/work-orders/{id}/complete",
        operation_id: "completeWorkOrderNs",
        permission: "production.complete",
        state_transition: true,
    },
    Mounted {
        method: "GET",
        path: "/api/v1/genealogy/trace",
        operation_id: "traceGenealogy",
        permission: "genealogy.view",
        state_transition: false,
    },
    Mounted {
        method: "POST",
        path: "/api/v1/calibration/certificates/{id}/approve",
        operation_id: "approveCalibration",
        permission: "calibration.approve",
        state_transition: true,
    },
];

/// Merge kernel + mounted routes into one OpenAPI 3 document.
pub fn document(_state: &AppState) -> Value {
    let mut paths = serde_json::Map::new();
    for op in MOUNTED {
        insert(
            &mut paths,
            op.path,
            op.method,
            op.operation_id,
            op.permission,
            op.state_transition,
        );
    }
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Datum HTTP API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Wave 2s slice. The UI consumes this document (ADR 0009)."
        },
        "paths": paths,
        "components": {
            "schemas": {
                "ErrorEnvelope": {
                    "type": "object",
                    "required": ["error"],
                    "properties": {
                        "error": {
                            "type": "object",
                            "required": ["code", "message", "request_id"],
                            "properties": {
                                "code": { "type": "string" },
                                "message": { "type": "string" },
                                "field": { "type": ["string", "null"] },
                                "request_id": { "type": "string", "format": "uuid" }
                            }
                        }
                    }
                }
            }
        }
    })
}

fn insert(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    op: &str,
    permission: &str,
    signature: bool,
) {
    let entry = paths.entry(path.to_string()).or_insert_with(|| json!({}));
    let mut op_v = json!({
        "operationId": op,
        "x-datum-permission": permission,
        "responses": {
            "200": { "description": "ok" },
            "201": { "description": "created" },
            "400": { "description": "validation" },
            "401": { "description": "unauthenticated" },
            "403": { "description": "forbidden" },
            "404": { "description": "not found" },
            "409": { "description": "conflict" }
        }
    });
    if signature {
        op_v["x-datum-signature"] = json!({
            "meaning": "Released",
            "permission": permission
        });
    }
    entry[method.to_ascii_lowercase()] = op_v;
}

/// Every path+method the router serves (for `openapi_lists_every_registered_route`).
pub fn registered_operations(doc: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(paths) = doc.get("paths").and_then(Value::as_object) {
        for (path, item) in paths {
            if let Some(obj) = item.as_object() {
                for method in obj.keys() {
                    if matches!(method.as_str(), "get" | "post" | "patch" | "put" | "delete") {
                        out.push((method.to_ascii_uppercase(), path.clone()));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// Mounted operations as (METHOD, path) pairs.
pub fn mounted_operations() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = MOUNTED
        .iter()
        .map(|m| (m.method.to_string(), m.path.to_string()))
        .collect();
    out.sort();
    out
}
