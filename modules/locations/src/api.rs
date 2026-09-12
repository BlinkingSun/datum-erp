//! HTTP-facing DTOs and handlers (`docs/10-api-conventions.md`).
//!
//! Route wiring lives in `datum-server`; this crate exposes typed operations only.

use datum_core::LocationId;
use datum_db::Tx;
use datum_events::SchemaRegistry;

use crate::domain::{CreateLocation, Location, LocationTreeNode, UpdateLocation};
use crate::store;
use crate::{Error, Result};

/// List envelope (`docs/10` §2.3).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListResponse<T> {
    /// Page of rows.
    pub data: Vec<T>,
    /// Cursor when more rows exist (unused until pagination lands).
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

/// `GET /api/v1/locations`.
pub async fn list_locations(tx: &mut Tx<'_>) -> Result<ListResponse<Location>> {
    Ok(ListResponse::all(store::list_flat(tx).await?))
}

/// `GET /api/v1/locations/tree`.
pub async fn list_tree(tx: &mut Tx<'_>) -> Result<ListResponse<LocationTreeNode>> {
    Ok(ListResponse::all(store::list_tree(tx).await?))
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

/// Deactivate (not yet exposed on HTTP in the slice stub).
pub async fn deactivate_location(
    tx: &mut Tx<'_>,
    id: LocationId,
    version: i64,
    registry: &SchemaRegistry,
) -> Result<Location> {
    store::deactivate(tx, id, version, registry).await
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
