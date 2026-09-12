//! HTTP contract: routes, error envelope, OpenAPI. No `axum` (CONTRACT §4).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::{Document, DocumentLine};
use crate::error::Error;

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
        method: "POST",
        path: "/api/v1/inventory/receipts",
        permission: "inventory.receive",
        operation_id: "createReceipt",
    },
    Route {
        method: "POST",
        path: "/api/v1/inventory/issues",
        permission: "inventory.issue",
        operation_id: "createIssue",
    },
    Route {
        method: "POST",
        path: "/api/v1/inventory/moves",
        permission: "inventory.move",
        operation_id: "createMove",
    },
    Route {
        method: "POST",
        path: "/api/v1/inventory/adjustments",
        permission: "inventory.adjust",
        operation_id: "createAdjustment",
    },
    Route {
        method: "POST",
        path: "/api/v1/inventory/counts",
        permission: "inventory.count",
        operation_id: "createCount",
    },
    Route {
        method: "GET",
        path: "/api/v1/inventory/on-hand",
        permission: "inventory.view",
        operation_id: "getOnHand",
    },
    Route {
        method: "GET",
        path: "/api/v1/inventory/documents/{id}",
        permission: "inventory.view",
        operation_id: "getDocument",
    },
];

/// Module error-code table (`docs/10` §2.6). HTTP status is [`http_status`].
pub const ERROR_CODES: &[&str] = &[
    "VALIDATION",
    "UNAUTHENTICATED",
    "FORBIDDEN",
    "NOT_FOUND",
    "CONFLICT",
    "IDEMPOTENCY_CONFLICT",
    "SIGNATURE_REQUIRED",
    "SIGNATURE_NO_PROVIDER",
    "RATE_LIMITED",
    "PAYLOAD_TOO_LARGE",
    "TIMEOUT",
    "INTERNAL",
];

/// Map crate errors to the `docs/10` code string.
pub fn error_code(err: &Error) -> &'static str {
    match err {
        Error::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
        Error::VersionConflict => "CONFLICT",
        Error::NotFound => "NOT_FOUND",
        Error::Document(_)
        | Error::ReasonRequired
        | Error::IdempotencyRequired
        | Error::InvalidLimit
        | Error::UnknownDimension
        | Error::NoEligibleLayer
        | Error::LotNotIssuable
        | Error::Manifest(_) => "VALIDATION",
        _ => "INTERNAL",
    }
}

/// HTTP status for [`error_code`] (`docs/10` §2.6).
pub fn http_status(err: &Error) -> u16 {
    match error_code(err) {
        "VALIDATION" => 400,
        "UNAUTHENTICATED" => 401,
        "FORBIDDEN" => 403,
        "NOT_FOUND" => 404,
        "CONFLICT" | "IDEMPOTENCY_CONFLICT" | "SIGNATURE_NO_PROVIDER" => 409,
        "PAYLOAD_TOO_LARGE" => 413,
        "RATE_LIMITED" => 429,
        "TIMEOUT" => 504,
        _ => 500,
    }
}

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

/// Wire document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentBody {
    /// Id.
    pub id: String,
    /// Kind.
    pub kind: String,
    /// Status.
    pub status: String,
    /// PO / WO / order reference.
    pub reference: Option<String>,
    /// Ledger group produced by posting.
    pub posted_group_id: Option<String>,
    /// Version.
    pub version: i64,
    /// Lines.
    pub lines: Vec<LineBody>,
}

/// Wire document line. Lots and serials are entity ids, never text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineBody {
    /// Id.
    pub id: String,
    /// Item.
    pub item_id: String,
    /// Lot entity.
    pub lot_id: Option<String>,
    /// Serial entity.
    pub serial_id: Option<String>,
    /// Source location.
    pub from_location_id: Option<String>,
    /// Destination location.
    pub to_location_id: Option<String>,
    /// Entered quantity.
    pub entered: QuantityBody,
    /// Canonical stock quantity.
    pub canonical: QuantityBody,
    /// Conversion factor.
    pub conversion_factor: String,
    /// Reason code.
    pub reason_code: Option<String>,
    /// Package entity.
    pub package_id: Option<String>,
}

impl From<&Document> for DocumentBody {
    fn from(doc: &Document) -> Self {
        Self {
            id: doc.id.to_string(),
            kind: doc.kind.as_str().into(),
            status: doc.status.as_str().into(),
            reference: doc.reference.clone(),
            posted_group_id: doc.posted_group_id.map(|g| g.to_string()),
            version: doc.version,
            lines: doc.lines.iter().map(LineBody::from).collect(),
        }
    }
}

impl From<&DocumentLine> for LineBody {
    fn from(line: &DocumentLine) -> Self {
        Self {
            id: line.id.to_string(),
            item_id: line.item.to_string(),
            lot_id: line.lot.map(|l| l.to_string()),
            serial_id: line.serial.map(|s| s.to_string()),
            from_location_id: line.from_location.map(|l| l.to_string()),
            to_location_id: line.to_location.map(|l| l.to_string()),
            entered: QuantityBody {
                amount: line.entered.amount.to_string(),
                unit: line.entered.unit.0,
                dimension: format!("{:?}", line.entered.dimension),
            },
            canonical: QuantityBody {
                amount: line.canonical.amount.to_string(),
                unit: line.canonical.unit.0,
                dimension: format!("{:?}", line.canonical.dimension),
            },
            conversion_factor: line.conversion_factor.to_string(),
            reason_code: line.reason_code.clone(),
            package_id: line.package.map(|p| p.as_uuid().to_string()),
        }
    }
}

/// On-hand projection body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnHandBody {
    /// Folded quantity as a decimal string.
    pub quantity: String,
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
        "info": {"title": "inventory", "version": "0.1.0"},
        "paths": paths,
        "components": {
            "schemas": {
                "ErrorEnvelope": {
                    "type": "object",
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "code": {"type": "string", "enum": ERROR_CODES},
                                "message": {"type": "string"},
                                "field": {"type": "string"},
                                "request_id": {"type": "string"}
                            }
                        }
                    }
                }
            }
        }
    })
}
