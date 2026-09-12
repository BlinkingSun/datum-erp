//! HTTP contract: routes, error envelope, OpenAPI. No `axum` (CONTRACT §4).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::Item;

/// One route this module registers (`docs/03` §3.4, `docs/10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// HTTP method.
    pub method: &'static str,
    /// Versioned path.
    pub path: &'static str,
    /// Permission that gates the route.
    pub permission: &'static str,
    /// OpenAPI operation id.
    pub operation_id: &'static str,
}

/// The four public routes (five operations) named in SPEC.
pub const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        path: "/api/v1/items",
        permission: "items.view",
        operation_id: "listItems",
    },
    Route {
        method: "POST",
        path: "/api/v1/items",
        permission: "items.edit",
        operation_id: "createItem",
    },
    Route {
        method: "GET",
        path: "/api/v1/items/{id}",
        permission: "items.view",
        operation_id: "getItem",
    },
    Route {
        method: "PATCH",
        path: "/api/v1/items/{id}",
        permission: "items.edit",
        operation_id: "updateItem",
    },
    Route {
        method: "POST",
        path: "/api/v1/items/{id}/release",
        permission: "items.release",
        operation_id: "releaseItem",
    },
];

/// Error envelope (`docs/10` §2.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Envelope.
    pub error: ErrorFields,
}

/// Fields of [`ErrorBody`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorFields {
    /// Machine-stable token.
    pub code: String,
    /// Human message.
    pub message: String,
    /// Field path, if any.
    pub field: Option<String>,
    /// Request id.
    pub request_id: String,
}

/// List envelope (`docs/10` §2.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBody<T> {
    /// Page of items.
    pub data: Vec<T>,
    /// Opaque next cursor.
    pub next_cursor: Option<String>,
    /// Whether another page exists.
    pub has_more: bool,
}

/// Wire item (`docs/10` snake_case).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemBody {
    /// Id.
    pub id: String,
    /// Number.
    pub number: String,
    /// Revision.
    pub revision: String,
    /// Description.
    pub description: String,
    /// Kind.
    pub kind: String,
    /// Stocking unit.
    pub stock_uom: i64,
    /// Scale.
    pub stock_scale: i16,
    /// Residual tolerance as a decimal string.
    pub residual_tolerance: String,
    /// Cost method.
    pub cost_method: String,
    /// Status.
    pub status: String,
    /// Version.
    pub version: i64,
    /// Application version.
    pub application_version: String,
    /// Configuration version.
    pub configuration_version: String,
    /// Created at (RFC 3339 UTC).
    pub created_at: String,
}

impl From<&Item> for ItemBody {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id.to_string(),
            number: item.number.clone(),
            revision: item.revision.clone(),
            description: item.description.clone(),
            kind: item.kind.as_str().to_string(),
            stock_uom: item.stock_uom.0,
            stock_scale: item.stock_scale,
            residual_tolerance: item.residual_tolerance.to_string(),
            cost_method: item.cost_method.clone(),
            status: item.status.as_str().to_string(),
            version: item.version,
            application_version: item.application_version.clone(),
            configuration_version: item.configuration_version.clone(),
            created_at: item.created_at.to_rfc3339(),
        }
    }
}

/// OpenAPI 3 document generated from [`ROUTES`].
pub fn openapi_document() -> Value {
    let mut paths = serde_json::Map::new();
    for route in ROUTES {
        let path_item = paths
            .entry(route.path.to_string())
            .or_insert_with(|| json!({}));
        let method = route.method.to_ascii_lowercase();
        path_item[method] = json!({
            "operationId": route.operation_id,
            "x-datum-permission": route.permission,
            "responses": {
                "200": { "description": "ok" },
                "201": { "description": "created" },
                "400": {
                    "description": "validation",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorEnvelope" }
                        }
                    }
                },
                "409": {
                    "description": "conflict",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorEnvelope" }
                        }
                    }
                }
            }
        });
        if route.operation_id == "releaseItem" {
            path_item["post"]["x-datum-signature"] = json!({
                "required": false,
                "reason": crate::domain::NOT_REQUIRED_REASON
            });
        }
    }
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Datum items API",
            "version": "0.1.0"
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
                                "code": { "type": "string", "enum": [
                                    "VALIDATION", "UNAUTHENTICATED", "FORBIDDEN",
                                    "NOT_FOUND", "CONFLICT", "IDEMPOTENCY_CONFLICT",
                                    "SIGNATURE_REQUIRED", "SIGNATURE_NO_PROVIDER",
                                    "RATE_LIMITED", "PAYLOAD_TOO_LARGE", "TIMEOUT"
                                ]},
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
