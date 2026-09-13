//! Persistence for `locations.*` (sole writer).

use serde_json::json;
use wicket_core::{Boundary, Identifier, LocationId};
use wicket_db::Tx;
use wicket_events::Event;
use wicket_ledger::{has_quantity_at, upsert_location};

use crate::domain::{
    BOUNDARY_VARIANTS, CreateLocation, ListFilter, Location, LocationKind, LocationStatus,
    LocationTreeNode, UpdateLocation, boundary_code, validate_code,
};
use crate::events;
use crate::{Error, Result};

type LocationRow = (
    uuid::Uuid,
    String,
    String,
    uuid::Uuid,
    Option<uuid::Uuid>,
    String,
    Option<String>,
    String,
    Option<uuid::Uuid>,
    i64,
);

fn row_to_location(row: LocationRow) -> Result<Location> {
    let (id, code, name, site_id, parent_id, kind, boundary, status, work_order_id, version) = row;
    let kind = LocationKind::from_sql(&kind)
        .ok_or_else(|| Error::Validation(format!("unknown kind {kind}")))?;
    let status = LocationStatus::from_sql(&status)
        .ok_or_else(|| Error::Validation(format!("unknown status {status}")))?;
    let boundary_class = match boundary {
        None => None,
        Some(label) => Some(wicket_ledger::boundary_from_sql(&label)?),
    };
    Ok(Location {
        id: LocationId::from_uuid(id),
        code,
        name,
        site_id: Identifier::from_uuid(site_id),
        parent_id: parent_id.map(LocationId::from_uuid),
        kind,
        boundary_class,
        status,
        work_order_id: work_order_id.map(Identifier::from_uuid),
        version,
    })
}

async fn fetch_row(tx: &mut Tx<'_>, id: LocationId) -> Result<Option<Location>> {
    let row: Option<LocationRow> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT id, code, name, site_id, parent_id, kind, boundary_class::text,
                        status, work_order_id, version
                   FROM locations.location WHERE id = $1",
            )
            .bind(id.as_uuid()),
        )
        .await?;
    row.map(row_to_location).transpose()
}

/// Default site code seeded at install (looked up; ids are minted UUID v7).
pub const DEFAULT_SITE_CODE: &str = "MAIN";

/// Load the seeded default site by code.
pub async fn default_site_id(tx: &mut Tx<'_>) -> Result<Identifier> {
    site_id_by_code(tx, DEFAULT_SITE_CODE).await
}

/// Load a site id by unique code.
pub async fn site_id_by_code(tx: &mut Tx<'_>, code: &str) -> Result<Identifier> {
    let row: Option<(uuid::Uuid,)> = tx
        .fetch_optional(sqlx::query_as("SELECT id FROM locations.site WHERE code = $1").bind(code))
        .await?;
    row.map(|(id,)| Identifier::from_uuid(id))
        .ok_or_else(|| Error::NotFound(code.into()))
}

/// Load a location id by unique code.
pub async fn location_id_by_code(tx: &mut Tx<'_>, code: &str) -> Result<LocationId> {
    let row: Option<(uuid::Uuid,)> = tx
        .fetch_optional(
            sqlx::query_as("SELECT id FROM locations.location WHERE code = $1").bind(code),
        )
        .await?;
    row.map(|(id,)| LocationId::from_uuid(id))
        .ok_or_else(|| Error::NotFound(code.into()))
}

/// Idempotent install seed: default site and seven virtual boundary locations.
///
/// SPEC-common does not mandate stable seed ids, so rows are minted as UUID v7
/// and subsequently looked up by code.
pub async fn seed_install(tx: &mut Tx<'_>) -> Result<()> {
    let minted_site = Identifier::generate();
    tx.execute(
        sqlx::query(
            "INSERT INTO locations.site (id, code, name, version)
             VALUES ($1, 'MAIN', 'Main site', 1)
             ON CONFLICT (code) DO NOTHING",
        )
        .bind(minted_site.as_uuid()),
    )
    .await?;
    let site = default_site_id(tx).await?;
    for boundary in BOUNDARY_VARIANTS {
        let code = boundary_code(boundary);
        let name = format!("Virtual {code}");
        let minted = LocationId::generate();
        tx.execute(
            sqlx::query(
                "INSERT INTO locations.location
                    (id, code, name, site_id, parent_id, kind, boundary_class, status, version)
                 VALUES ($1, $2, $3, $4, NULL, 'virtual', $5::ledger.boundary, 'active', 1)
                 ON CONFLICT (boundary_class) WHERE boundary_class IS NOT NULL DO NOTHING",
            )
            .bind(minted.as_uuid())
            .bind(code)
            .bind(name)
            .bind(site.as_uuid())
            .bind(wicket_ledger::boundary_sql(boundary)?),
        )
        .await?;
        let id = location_id_by_code(tx, code).await?;
        upsert_location(tx, id, Some(boundary)).await?;
    }
    Ok(())
}

/// Seeded virtual boundary location, looked up by code after install.
pub async fn boundary_location_id(tx: &mut Tx<'_>, boundary: Boundary) -> Result<LocationId> {
    location_id_by_code(tx, boundary_code(boundary)).await
}

/// Create a real location and sync the ledger registry.
pub async fn create(tx: &mut Tx<'_>, input: CreateLocation) -> Result<Location> {
    if input.kind == LocationKind::Virtual {
        return Err(Error::Validation(
            "virtual locations are seeded at install".into(),
        ));
    }
    validate_code(&input.code)?;
    let id = LocationId::generate();
    tx.execute(
        sqlx::query(
            "INSERT INTO locations.location
                (id, code, name, site_id, parent_id, kind, boundary_class, status, version)
             VALUES ($1, $2, $3, $4, $5, $6, NULL, 'active', 1)",
        )
        .bind(id.as_uuid())
        .bind(&input.code)
        .bind(&input.name)
        .bind(input.site_id.as_uuid())
        .bind(input.parent_id.map(|p| p.as_uuid()))
        .bind(input.kind.as_sql()),
    )
    .await?;
    upsert_location(tx, id, None).await?;
    fetch_row(tx, id)
        .await?
        .ok_or_else(|| Error::NotFound(id.to_string()))
}

/// Load one location.
pub async fn get(tx: &mut Tx<'_>, id: LocationId) -> Result<Location> {
    fetch_row(tx, id)
        .await?
        .ok_or_else(|| Error::NotFound(id.to_string()))
}

/// Flat list ordered by code (tree builder; not the HTTP collection).
pub async fn list_flat(tx: &mut Tx<'_>) -> Result<Vec<Location>> {
    let rows: Vec<LocationRow> = tx
        .fetch_all(sqlx::query_as(
            "SELECT id, code, name, site_id, parent_id, kind, boundary_class::text,
                        status, work_order_id, version
                   FROM locations.location
                  ORDER BY code",
        ))
        .await?;
    rows.into_iter().map(row_to_location).collect()
}

const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: u32 = 200;

/// Cursor-paginated list. Default sort is `id` ascending (UUID v7 create order).
pub async fn list(
    tx: &mut Tx<'_>,
    filter: ListFilter,
) -> Result<(Vec<Location>, Option<String>, bool)> {
    let limit = filter.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    if !(1..=MAX_LIST_LIMIT).contains(&limit) {
        return Err(Error::Validation("limit".into()));
    }
    let fetch = i64::from(limit) + 1;
    let rows: Vec<LocationRow> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT id, code, name, site_id, parent_id, kind, boundary_class::text,
                        status, work_order_id, version
                   FROM locations.location
                  WHERE ($1::uuid IS NULL OR id > $1)
                  ORDER BY id
                  LIMIT $2",
            )
            .bind(filter.cursor.map(|c| c.as_uuid()))
            .bind(fetch),
        )
        .await?;
    let has_more = rows.len() as u32 > limit;
    let data: Vec<Location> = rows
        .into_iter()
        .take(limit as usize)
        .map(row_to_location)
        .collect::<Result<Vec<_>>>()?;
    let next_cursor = if has_more {
        data.last().map(|loc| loc.id.to_string())
    } else {
        None
    };
    Ok((data, next_cursor, has_more))
}

/// Build a forest of locations. Inactive rows are omitted unless `include_inactive`.
pub async fn list_tree(tx: &mut Tx<'_>, include_inactive: bool) -> Result<Vec<LocationTreeNode>> {
    let all = list_flat(tx).await?;
    let all: Vec<Location> = if include_inactive {
        all
    } else {
        all.into_iter()
            .filter(|loc| loc.status == LocationStatus::Active)
            .collect()
    };
    let mut by_parent: std::collections::BTreeMap<Option<LocationId>, Vec<Location>> =
        std::collections::BTreeMap::new();
    for loc in all {
        by_parent.entry(loc.parent_id).or_default().push(loc);
    }
    fn build(
        parent: Option<LocationId>,
        map: &mut std::collections::BTreeMap<Option<LocationId>, Vec<Location>>,
    ) -> Vec<LocationTreeNode> {
        let Some(mut nodes) = map.remove(&parent) else {
            return Vec::new();
        };
        nodes.sort_by(|a, b| a.code.cmp(&b.code));
        nodes
            .into_iter()
            .map(|location| LocationTreeNode {
                children: build(Some(location.id), map),
                location,
            })
            .collect()
    }
    Ok(build(None, &mut by_parent))
}

/// Update name and/or parent (never `boundary_class`).
pub async fn update(tx: &mut Tx<'_>, id: LocationId, patch: UpdateLocation) -> Result<Location> {
    let current = get(tx, id).await?;
    if current.version != patch.version {
        return Err(Error::Conflict("version mismatch".into()));
    }
    if current.boundary_class.is_some() && patch.parent_id.is_some() {
        return Err(Error::Immutable("boundary location parent".into()));
    }
    if let Some(ref name) = patch.name {
        if name.trim().is_empty() {
            return Err(Error::Validation("name".into()));
        }
        tx.execute(
            sqlx::query(
                "UPDATE locations.location SET name = $2, version = version + 1 WHERE id = $1",
            )
            .bind(id.as_uuid())
            .bind(name),
        )
        .await?;
    }
    if let Some(new_parent) = patch.parent_id {
        if current.boundary_class.is_some() {
            return Err(Error::Immutable("boundary location parent".into()));
        }
        if let Some(p) = new_parent {
            if p == id {
                return Err(Error::Cycle);
            }
            if would_cycle(tx, id, p).await? {
                return Err(Error::Cycle);
            }
        }
        tx.execute(
            sqlx::query(
                "UPDATE locations.location SET parent_id = $2, version = version + 1 WHERE id = $1",
            )
            .bind(id.as_uuid())
            .bind(new_parent.map(|p| p.as_uuid())),
        )
        .await?;
    }
    get(tx, id).await
}

async fn would_cycle(tx: &mut Tx<'_>, id: LocationId, mut cursor: LocationId) -> Result<bool> {
    loop {
        if cursor == id {
            return Ok(true);
        }
        let parent: Option<(Option<uuid::Uuid>,)> = tx
            .fetch_optional(
                sqlx::query_as("SELECT parent_id FROM locations.location WHERE id = $1")
                    .bind(cursor.as_uuid()),
            )
            .await?;
        let Some((parent_id,)) = parent else {
            return Ok(false);
        };
        let Some(p) = parent_id else {
            return Ok(false);
        };
        cursor = LocationId::from_uuid(p);
    }
}

/// Mark inactive when empty; emits [`events::LOCATION_DEACTIVATED`].
///
/// Schemas must already be on the process-global registry (`install` registers them).
pub async fn deactivate(tx: &mut Tx<'_>, id: LocationId, version: i64) -> Result<Location> {
    let current = get(tx, id).await?;
    if current.version != version {
        return Err(Error::Conflict("version mismatch".into()));
    }
    if current.boundary_class.is_some() {
        return Err(Error::Protected);
    }
    if location_has_on_hand(tx, id).await? {
        return Err(Error::OnHand);
    }
    tx.execute(
        sqlx::query(
            "UPDATE locations.location SET status = 'inactive', version = version + 1 WHERE id = $1",
        )
        .bind(id.as_uuid()),
    )
    .await?;
    let event = Event::builder()
        .name(events::LOCATION_DEACTIVATED)
        .version(1)
        .payload(json!({ "location_id": id.as_uuid().to_string() }))
        .build()?;
    wicket_events::publish(tx, event).await?;
    get(tx, id).await
}

/// Idempotent WIP location for a work order.
pub async fn ensure_wip(tx: &mut Tx<'_>, work_order_id: Identifier) -> Result<LocationId> {
    let existing: Option<(uuid::Uuid,)> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT id FROM locations.location WHERE work_order_id = $1 AND kind = 'wip'",
            )
            .bind(work_order_id.as_uuid()),
        )
        .await?;
    if let Some((id,)) = existing {
        return Ok(LocationId::from_uuid(id));
    }
    let site = default_site_id(tx).await?;
    let id = LocationId::generate();
    let raw = work_order_id.as_uuid().simple().to_string();
    let code = format!("WIP-{}", raw[..12].to_uppercase());
    validate_code(&code)?;
    tx.execute(
        sqlx::query(
            "INSERT INTO locations.location
                (id, code, name, site_id, parent_id, kind, boundary_class, status,
                 work_order_id, version)
             VALUES ($1, $2, $3, $4, NULL, 'wip', NULL, 'active', $5, 1)",
        )
        .bind(id.as_uuid())
        .bind(&code)
        .bind(format!("WIP for {work_order_id}"))
        .bind(site.as_uuid())
        .bind(work_order_id.as_uuid()),
    )
    .await?;
    upsert_location(tx, id, None).await?;
    Ok(id)
}

/// Whether any quantity slice at `location` nets above zero (R-2s-3 ledger seam).
pub async fn location_has_on_hand(tx: &mut Tx<'_>, location: LocationId) -> Result<bool> {
    Ok(has_quantity_at(tx, location).await?)
}
