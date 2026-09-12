//! Persistence for `locations.*` (sole writer).

use datum_core::{Boundary, Identifier, LocationId};
use datum_db::Tx;
use datum_events::{Event, SchemaRegistry};
use datum_ledger::upsert_location;
use serde_json::json;

use crate::domain::{
    BOUNDARY_VARIANTS, CreateLocation, Location, LocationKind, LocationStatus, LocationTreeNode,
    UpdateLocation, boundary_code, validate_code,
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
        Some(label) => Some(datum_ledger::boundary_from_sql(&label)?),
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

/// Default site id (stable v5).
pub fn default_site_id() -> Identifier {
    Identifier::from_uuid(uuid::Uuid::from_bytes([
        0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ]))
}

/// Idempotent install seed: default site and seven virtual boundary locations.
pub async fn seed_install(tx: &mut Tx<'_>) -> Result<()> {
    let site = default_site_id();
    tx.execute(
        sqlx::query(
            "INSERT INTO locations.site (id, code, name, version)
             VALUES ($1, 'MAIN', 'Main site', 1)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(site.as_uuid()),
    )
    .await?;
    for boundary in BOUNDARY_VARIANTS {
        let code = boundary_code(boundary);
        let name = format!("Virtual {code}");
        let id = boundary_location_id(boundary);
        tx.execute(
            sqlx::query(
                "INSERT INTO locations.location
                    (id, code, name, site_id, parent_id, kind, boundary_class, status, version)
                 VALUES ($1, $2, $3, $4, NULL, 'virtual', $5::ledger.boundary, 'active', 1)
                 ON CONFLICT (boundary_class) WHERE boundary_class IS NOT NULL DO NOTHING",
            )
            .bind(id.as_uuid())
            .bind(code)
            .bind(name)
            .bind(site.as_uuid())
            .bind(datum_ledger::boundary_sql(boundary)?),
        )
        .await?;
        upsert_location(tx, id, Some(boundary)).await?;
    }
    Ok(())
}

/// Stable id for a seeded virtual boundary row.
pub fn boundary_location_id(boundary: Boundary) -> LocationId {
    let bytes: [u8; 16] = match boundary {
        Boundary::Supplier => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01],
        Boundary::Customer => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02],
        Boundary::Scrap => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x03],
        Boundary::Adjustment => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x04],
        Boundary::Rounding => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x05],
        Boundary::Consumed => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x06],
        Boundary::Produced => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x07],
        _ => [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x08],
    };
    LocationId::from_uuid(uuid::Uuid::from_bytes(bytes))
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

/// Flat list ordered by code.
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

/// Build a forest of active locations.
pub async fn list_tree(tx: &mut Tx<'_>) -> Result<Vec<LocationTreeNode>> {
    let all = list_flat(tx).await?;
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
pub async fn deactivate(
    tx: &mut Tx<'_>,
    id: LocationId,
    version: i64,
    registry: &SchemaRegistry,
) -> Result<Location> {
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
        .build_with(registry)?;
    datum_events::publish(tx, event).await?;
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
    let site = default_site_id();
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

/// Whether any rebuildable balance row shows on-hand at `location`.
pub async fn location_has_on_hand(tx: &mut Tx<'_>, location: LocationId) -> Result<bool> {
    let row: (bool,) = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT EXISTS (
                   SELECT 1 FROM transient.balance_projection
                    WHERE location_id = $1 AND quantity <> 0
                 )",
            )
            .bind(location.as_uuid()),
        )
        .await?;
    Ok(row.0)
}
