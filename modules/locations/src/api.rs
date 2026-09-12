//! HTTP-facing DTOs and handlers (`docs/10-api-conventions.md`).
//!
//! Route wiring lives in `datum-server`; this crate exposes typed operations only.

use datum_core::LocationId;
use datum_db::Tx;
use serde_json::{Value, json};

use crate::domain::{CreateLocation, ListFilter, Location, LocationTreeNode, UpdateLocation};
use crate::store;
use crate::{Error, Result};

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

/// Public routes. Method-level permissions: view for GET, edit for POST/PATCH/deactivate.
pub const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        path: "/api/v1/locations",
        permission: "locations.view",
        operation_id: "listLocations",
    },
    Route {
        method: "POST",
        path: "/api/v1/locations",
        permission: "locations.edit",
        operation_id: "createLocation",
    },
    Route {
        method: "GET",
        path: "/api/v1/locations/{id}",
        permission: "locations.view",
        operation_id: "getLocation",
    },
    Route {
        method: "PATCH",
        path: "/api/v1/locations/{id}",
        permission: "locations.edit",
        operation_id: "updateLocation",
    },
    Route {
        method: "GET",
        path: "/api/v1/locations/tree",
        permission: "locations.view",
        operation_id: "listLocationTree",
    },
    Route {
        method: "POST",
        path: "/api/v1/locations/{id}/deactivate",
        permission: "locations.edit",
        operation_id: "deactivateLocation",
    },
];

/// Module error-code table (`docs/10` §2.6 plus `REFUSED` for on-hand / protected).
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
    "REFUSED",
    "INTERNAL",
];

/// List envelope (`docs/10` §2.3).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListResponse<T> {
    /// Page of rows.
    pub data: Vec<T>,
    /// Opaque next cursor (`id` of the last row on this page).
    pub next_cursor: Option<String>,
    /// Whether another page exists.
    pub has_more: bool,
}

impl<T> ListResponse<T> {
    /// Single page with no cursor.
    pub fn all(data: Vec<T>) -> Self {
        Self {
            data,
            next_cursor: None,
            has_more: false,
        }
    }
}

/// `GET /api/v1/locations` — cursor pagination, default sort `id` ascending.
pub async fn list_locations(
    tx: &mut Tx<'_>,
    limit: Option<u32>,
    cursor: Option<&str>,
) -> Result<ListResponse<Location>> {
    let cursor = match cursor {
        Some(c) if !c.is_empty() => Some(parse_location_id(c)?),
        _ => None,
    };
    let (data, next_cursor, has_more) = store::list(tx, ListFilter { limit, cursor }).await?;
    Ok(ListResponse {
        data,
        next_cursor,
        has_more,
    })
}

fn parse_location_id(s: &str) -> Result<LocationId> {
    uuid::Uuid::parse_str(s)
        .map(LocationId::from_uuid)
        .map_err(|_| Error::Validation("cursor".into()))
}

/// `GET /api/v1/locations/tree`. Inactive nodes omitted unless `include_inactive`.
pub async fn list_tree(
    tx: &mut Tx<'_>,
    include_inactive: bool,
) -> Result<ListResponse<LocationTreeNode>> {
    Ok(ListResponse::all(
        store::list_tree(tx, include_inactive).await?,
    ))
}

/// `GET /api/v1/locations/{id}`.
pub async fn get_location(tx: &mut Tx<'_>, id: LocationId) -> Result<Location> {
    store::get(tx, id).await
}

/// `POST /api/v1/locations`.
pub async fn create_location(tx: &mut Tx<'_>, input: CreateLocation) -> Result<Location> {
    store::create(tx, input).await
}

/// `PATCH /api/v1/locations/{id}`.
pub async fn patch_location(
    tx: &mut Tx<'_>,
    id: LocationId,
    patch: UpdateLocation,
) -> Result<Location> {
    store::update(tx, id, patch).await
}

/// `POST /api/v1/locations/{id}/deactivate`.
pub async fn deactivate_location(
    tx: &mut Tx<'_>,
    id: LocationId,
    version: i64,
) -> Result<Location> {
    store::deactivate(tx, id, version).await
}

/// Map module errors to HTTP-style codes for server-slice.
pub fn error_code(err: &Error) -> &'static str {
    match err {
        Error::NotFound(_) => "NOT_FOUND",
        Error::Validation(_) | Error::Immutable(_) | Error::Cycle => "VALIDATION",
        Error::Conflict(_) => "CONFLICT",
        Error::OnHand | Error::Protected => "REFUSED",
        _ => "INTERNAL",
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
                "404": {
                    "description": "not found",
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
    }
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Datum locations API",
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
                                "code": { "type": "string", "enum": ERROR_CODES },
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
