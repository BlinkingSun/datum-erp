//! HTTP contract: routes, error envelope, OpenAPI. No `axum` (CONTRACT §4).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::{Completion, WorkOrder};

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

/// Public routes named in SPEC.
pub const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        path: "/api/v1/work-orders",
        permission: "production.view",
        operation_id: "listWorkOrders",
    },
    Route {
        method: "POST",
        path: "/api/v1/work-orders",
        permission: "production.create",
        operation_id: "createWorkOrder",
    },
    Route {
        method: "GET",
        path: "/api/v1/work-orders/{id}",
        permission: "production.view",
        operation_id: "getWorkOrder",
    },
    Route {
        method: "POST",
        path: "/api/v1/work-orders/{id}/release",
        permission: "production.release",
        operation_id: "releaseWorkOrder",
    },
    Route {
        method: "POST",
        path: "/api/v1/work-orders/{id}/issue",
        permission: "production.issue",
        operation_id: "issueWorkOrder",
    },
    Route {
        method: "POST",
        path: "/api/v1/work-orders/{id}/complete",
        permission: "production.complete",
        operation_id: "completeWorkOrder",
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

/// Wire quantity (`docs/10`: amount is a decimal string).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityBody {
    /// Amount as a decimal string.
    pub amount: String,
    /// Catalog unit id.
    pub unit: i64,
    /// Dimension kind.
    pub dimension: String,
}

/// Wire work order. Lots and serials are entity ids, never text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkOrderBody {
    /// Id.
    pub id: String,
    /// Gap-free number, null until release.
    pub number: Option<String>,
    /// Finished item.
    pub item_id: String,
    /// Ordered quantity.
    pub quantity_ordered: QuantityBody,
    /// Revision.
    pub revision: String,
    /// Status.
    pub status: String,
    /// WIP location.
    pub wip_location_id: Option<String>,
    /// Version.
    pub version: i64,
}

/// Wire completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionBody {
    /// Id.
    pub id: String,
    /// Work order.
    pub work_order_id: String,
    /// Finished lot entity.
    pub finished_lot_id: String,
    /// Good quantity.
    pub quantity_good: QuantityBody,
    /// Scrap quantity.
    pub quantity_scrap: QuantityBody,
    /// TRANSFORMATION group.
    pub group_id: String,
}

impl From<&WorkOrder> for WorkOrderBody {
    fn from(wo: &WorkOrder) -> Self {
        Self {
            id: wo.id.to_string(),
            number: wo.number.clone(),
            item_id: wo.item.to_string(),
            quantity_ordered: QuantityBody {
                amount: wo.quantity_ordered.amount.to_string(),
                unit: wo.quantity_ordered.unit.0,
                dimension: format!("{:?}", wo.quantity_ordered.dimension),
            },
            revision: wo.revision.clone(),
            status: wo.status.as_str().into(),
            wip_location_id: wo.wip_location.map(|l| l.to_string()),
            version: wo.version,
        }
    }
}

impl From<&Completion> for CompletionBody {
    fn from(c: &Completion) -> Self {
        Self {
            id: c.id.to_string(),
            work_order_id: c.work_order.to_string(),
            finished_lot_id: c.finished_lot.to_string(),
            quantity_good: QuantityBody {
                amount: c.quantity_good.amount.to_string(),
                unit: c.quantity_good.unit.0,
                dimension: format!("{:?}", c.quantity_good.dimension),
            },
            quantity_scrap: QuantityBody {
                amount: c.quantity_scrap.amount.to_string(),
                unit: c.quantity_scrap.unit.0,
                dimension: format!("{:?}", c.quantity_scrap.dimension),
            },
            group_id: c.group_id.to_string(),
        }
    }
}

/// OpenAPI document generated from [`ROUTES`].
pub fn openapi_document() -> Value {
    let mut paths = serde_json::Map::new();
    for route in ROUTES {
        let item = json!({
            route.method.to_ascii_lowercase(): {
                "operationId": route.operation_id,
                "security": [{"permission": [route.permission]}],
            }
        });
        paths
            .entry(route.path.to_string())
            .and_modify(|existing| {
                if let Some(obj) = existing.as_object_mut() {
                    obj.extend(item.as_object().cloned().unwrap_or_default());
                }
            })
            .or_insert(item);
    }
    json!({
        "openapi": "3.1.0",
        "info": {"title": "production-min", "version": "0.1.0"},
        "paths": paths,
        "components": {
            "schemas": {
                "ErrorEnvelope": {
                    "type": "object",
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "code": {
                                    "type": "string",
                                    "enum": [
                                        "VALIDATION",
                                        "UNAUTHENTICATED",
                                        "FORBIDDEN",
                                        "NOT_FOUND",
                                        "CONFLICT",
                                        "IDEMPOTENCY_CONFLICT",
                                        "SIGNATURE_REQUIRED",
                                        "SIGNATURE_NO_PROVIDER"
                                    ]
                                },
                                "message": {"type": "string"},
                                "field": {"type": ["string", "null"]},
                                "request_id": {"type": "string"}
                            },
                            "required": ["code", "message", "request_id"]
                        }
                    },
                    "required": ["error"]
                }
            }
        }
    })
}
