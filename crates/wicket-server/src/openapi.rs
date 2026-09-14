//! OpenAPI document generated from the capability table (ADR 0010 / T-25).

use serde_json::{Value, json};

use crate::boot::AppState;
use crate::capabilities::{self, CapabilityKind};

/// Merge the capability table into one OpenAPI 3 document.
pub fn document(_state: &AppState) -> Value {
    let mut paths = serde_json::Map::new();
    for cap in capabilities::table() {
        insert(
            &mut paths,
            cap.path,
            cap.method,
            cap.id,
            cap.permission,
            cap.kind == CapabilityKind::Transition,
        );
    }
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Wicket HTTP API",
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
        "x-wicket-permission": permission,
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
        op_v["x-wicket-signature"] = json!({
            "meaning": "Released",
            "permission": permission
        });
    }
    entry[method.to_ascii_lowercase()] = op_v;
}

/// Every path+method in a served OpenAPI document.
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

/// Capability-table operations as (METHOD, path) pairs.
pub fn mounted_operations() -> Vec<(String, String)> {
    capabilities::operations()
}
