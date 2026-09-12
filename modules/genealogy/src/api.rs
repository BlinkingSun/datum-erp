//! HTTP contract: routes, error envelope, OpenAPI. No `axum` (CONTRACT §4).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::store::{TraceBody, TraceOutcome};

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
        path: "/api/v1/genealogy/trace",
        permission: "genealogy.view",
        operation_id: "traceGenealogy",
    },
    Route {
        method: "GET",
        path: "/api/v1/genealogy/impact/{lot}",
        permission: "genealogy.view",
        operation_id: "getImpact",
    },
    Route {
        method: "GET",
        path: "/api/v1/genealogy/jobs/{id}",
        permission: "genealogy.view",
        operation_id: "getGenealogyJob",
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

/// 202 Accepted body for a large trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedBody {
    /// Job id.
    pub job_id: String,
    /// Result URL.
    pub result_url: String,
}

/// HTTP status for a trace outcome.
pub fn trace_http_status(outcome: &TraceOutcome) -> u16 {
    match outcome {
        TraceOutcome::Inline(_) => 200,
        TraceOutcome::Accepted { .. } => 202,
    }
}

/// JSON body for a trace outcome.
pub fn trace_http_body(outcome: &TraceOutcome) -> Result<Value, serde_json::Error> {
    match outcome {
        TraceOutcome::Inline(body) => serde_json::to_value(body),
        TraceOutcome::Accepted { job_id, result_url } => Ok(json!({
            "job_id": job_id.0.to_string(),
            "result_url": result_url,
        })),
    }
}

/// Re-export for OpenAPI examples.
pub fn example_tree() -> TraceBody {
    TraceBody::Both {
        backward: crate::domain::Tree {
            direction: crate::domain::Direction::Backward,
            nodes: Vec::new(),
        },
        forward: crate::domain::Tree {
            direction: crate::domain::Direction::Forward,
            nodes: Vec::new(),
        },
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
                "parameters": match route.operation_id {
                    "traceGenealogy" => json!([
                        {"name": "lot", "in": "query", "schema": {"type": "string", "format": "uuid"}},
                        {"name": "serial", "in": "query", "schema": {"type": "string", "format": "uuid"}},
                        {"name": "posting", "in": "query", "schema": {"type": "integer"}},
                        {"name": "direction", "in": "query", "schema": {"type": "string", "enum": ["backward", "forward", "both"]}},
                        {"name": "depth", "in": "query", "schema": {"type": "integer"}},
                        {"name": "format", "in": "query", "schema": {"type": "string", "enum": ["json", "csv"]}}
                    ]),
                    _ => json!([]),
                },
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
        "info": {"title": "genealogy", "version": "0.1.0"},
        "paths": paths,
        "components": {
            "schemas": {
                "ErrorEnvelope": {
                    "type": "object",
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "code": {"type": "string", "enum": ["VALIDATION", "NOT_FOUND"]},
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
