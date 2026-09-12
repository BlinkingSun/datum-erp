//! Persistence. The only code that touches `items.*` tables.

use chrono::{DateTime, Utc};
use datum_core::{Identifier, ItemId, UnitId};
use datum_db::{Pool, Tx, WriteContext};
use datum_ledger::{cost_method_from_sql, has_postings, upsert_stock_item};
use datum_module::Kernel;
use datum_statemachine::DocRef;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::DOC_TYPE;
use crate::domain::{
    Item, Kind, ListFilter, NewItem, Page, Status, UpdateItem, cost_method_sql, number_is_valid,
};
use crate::error::{Error, Result};
use crate::events::{item_obsoleted, item_released};

type ItemRow = (
    Uuid,
    String,
    String,
    String,
    String,
    i64,
    i16,
    Decimal,
    String,
    String,
    i64,
    String,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
);

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

const GET_SQL: &str = "SELECT id, number, revision, description, kind,
        stock_uom_id, stock_scale, residual_tolerance, cost_method, status,
        version, application_version, configuration_version, created_at, updated_at
   FROM items.item WHERE id = $1";

const LIST_SQL: &str = "SELECT id, number, revision, description, kind,
        stock_uom_id, stock_scale, residual_tolerance, cost_method, status,
        version, application_version, configuration_version, created_at, updated_at
   FROM items.item
  WHERE ($1::text IS NULL OR kind = $1)
    AND ($2::text IS NULL OR status = $2)
    AND ($3::text IS NULL OR number LIKE $3 || '%')
    AND ($4::uuid IS NULL OR id > $4)
  ORDER BY id
  LIMIT $5";

/// Insert `new`, sync `ledger.stock_item`, spawn the draft instance.
pub async fn create(tx: &mut Tx<'_>, kernel: &Kernel, new: NewItem) -> Result<Item> {
    new.validate()?;
    let id = ItemId::generate();
    let method = cost_method_sql(new.cost_method);
    tx.execute(
        sqlx::query(
            "INSERT INTO items.item (
                 id, number, revision, description, kind,
                 stock_uom_id, stock_scale, residual_tolerance, cost_method, status
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'draft')",
        )
        .bind(id.as_uuid())
        .bind(&new.number)
        .bind(&new.revision)
        .bind(&new.description)
        .bind(new.kind.as_str())
        .bind(new.stock_uom.0)
        .bind(new.stock_scale)
        .bind(new.residual_tolerance)
        .bind(method),
    )
    .await
    .map_err(map_number_unique)?;
    append_revision(tx, id, &new.revision).await?;
    upsert_stock_item(
        tx,
        id,
        new.stock_uom,
        new.stock_scale,
        new.residual_tolerance,
        new.cost_method,
        new.standard,
    )
    .await?;
    kernel
        .spawn(tx, &doc_ref(id), Status::Draft.as_str())
        .await?;
    load_in_tx(tx, id).await
}

/// Load one item.
pub async fn get(pool: &Pool, id: ItemId) -> Result<Item> {
    let row: Option<ItemRow> = sqlx::query_as(GET_SQL)
        .bind(id.as_uuid())
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(item_from_row(r)?),
        None => Err(Error::NotFound(id)),
    }
}

/// Cursor-paginated list. Default sort is `id` ascending (UUID v7 create order).
pub async fn list(pool: &Pool, filter: ListFilter) -> Result<Page<Item>> {
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Error::InvalidLimit);
    }
    let kind = filter.kind.map(Kind::as_str);
    let status = filter.status.map(Status::as_str);
    let prefix = filter.number_prefix.clone();
    let cursor = filter.cursor.map(|id| id.as_uuid());
    let fetch = i64::from(limit) + 1;
    let rows: Vec<ItemRow> = sqlx::query_as(LIST_SQL)
        .bind(kind)
        .bind(status)
        .bind(prefix)
        .bind(cursor)
        .bind(fetch)
        .fetch_all(pool)
        .await?;
    let has_more = rows.len() as u32 > limit;
    let mut data = Vec::with_capacity(rows.len().min(limit as usize));
    for row in rows.into_iter().take(limit as usize) {
        data.push(item_from_row(row)?);
    }
    let next_cursor = if has_more {
        data.last().map(|i| i.id.to_string())
    } else {
        None
    };
    Ok(Page {
        data,
        next_cursor,
        has_more,
    })
}

/// Optimistic update. Refuses stock unit/scale/tolerance changes once postings exist.
pub async fn update(tx: &mut Tx<'_>, id: ItemId, patch: UpdateItem) -> Result<Item> {
    let current = load_in_tx(tx, id).await?;
    if current.version != patch.version {
        return Err(Error::VersionConflict);
    }
    if current.status == Status::Obsolete {
        return Err(Error::InvalidTransition {
            edge: "update".into(),
            status: current.status.as_str().into(),
        });
    }
    let revision = patch.revision.as_deref().unwrap_or(&current.revision);
    let description = patch.description.as_deref().unwrap_or(&current.description);
    let kind = patch.kind.unwrap_or(current.kind);
    let stock_uom = patch.stock_uom.unwrap_or(current.stock_uom);
    let stock_scale = patch.stock_scale.unwrap_or(current.stock_scale);
    let residual = patch
        .residual_tolerance
        .unwrap_or(current.residual_tolerance);
    let method = match patch.cost_method {
        Some(m) => m,
        None => cost_method_from_sql(&current.cost_method)
            .ok_or_else(|| Error::Manifest(current.cost_method.clone()))?,
    };
    if !(0..=8).contains(&stock_scale) {
        return Err(Error::Manifest("stock_scale must be 0..=8".into()));
    }
    let measure_changed = stock_uom != current.stock_uom
        || stock_scale != current.stock_scale
        || residual != current.residual_tolerance;
    if measure_changed && has_postings(tx, id).await? {
        return Err(Error::StockMeasureImmutable);
    }
    let n = tx
        .execute(
            sqlx::query(
                "UPDATE items.item SET
                     revision = $2,
                     description = $3,
                     kind = $4,
                     stock_uom_id = $5,
                     stock_scale = $6,
                     residual_tolerance = $7,
                     cost_method = $8,
                     version = version + 1,
                     updated_at = now()
                 WHERE id = $1 AND version = $9",
            )
            .bind(id.as_uuid())
            .bind(revision)
            .bind(description)
            .bind(kind.as_str())
            .bind(stock_uom.0)
            .bind(stock_scale)
            .bind(residual)
            .bind(cost_method_sql(method))
            .bind(patch.version),
        )
        .await?;
    if n.rows_affected() != 1 {
        return Err(Error::VersionConflict);
    }
    if revision != current.revision {
        append_revision(tx, id, revision).await?;
    }
    upsert_stock_item(
        tx,
        id,
        stock_uom,
        stock_scale,
        residual,
        method,
        patch.standard,
    )
    .await?;
    load_in_tx(tx, id).await
}

/// `draft → released`. Emits `items.item_released.v1`.
///
/// `tx` must have been begun with [`Kernel::transition_context`] for edge `release`.
pub async fn release(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &WriteContext,
    id: ItemId,
) -> Result<Item> {
    let current = load_in_tx(tx, id).await?;
    if current.status != Status::Draft {
        return Err(Error::InvalidTransition {
            edge: "release".into(),
            status: current.status.as_str().into(),
        });
    }
    transition_to(tx, kernel, ctx, id, "release", Status::Released).await?;
    let item = load_in_tx(tx, id).await?;
    kernel.publish_event(tx, item_released(&item)?).await?;
    Ok(item)
}

/// `released → obsolete`. Emits `items.item_obsoleted.v1`.
///
/// `tx` must have been begun with [`Kernel::transition_context`] for edge `obsolete`.
pub async fn obsolete(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &WriteContext,
    id: ItemId,
) -> Result<Item> {
    let current = load_in_tx(tx, id).await?;
    if current.status != Status::Released {
        return Err(Error::InvalidTransition {
            edge: "obsolete".into(),
            status: current.status.as_str().into(),
        });
    }
    transition_to(tx, kernel, ctx, id, "obsolete", Status::Obsolete).await?;
    let item = load_in_tx(tx, id).await?;
    kernel.publish_event(tx, item_obsoleted(&item)?).await?;
    Ok(item)
}

async fn transition_to(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &WriteContext,
    id: ItemId,
    edge: &str,
    to: Status,
) -> Result<()> {
    kernel.transition(tx, &doc_ref(id), edge, None, ctx).await?;
    let n = tx
        .execute(
            sqlx::query(
                "UPDATE items.item SET status = $2, version = version + 1, updated_at = now()
                 WHERE id = $1",
            )
            .bind(id.as_uuid())
            .bind(to.as_str()),
        )
        .await?;
    if n.rows_affected() != 1 {
        return Err(Error::NotFound(id));
    }
    Ok(())
}

async fn load_in_tx(tx: &mut Tx<'_>, id: ItemId) -> Result<Item> {
    let row: Option<ItemRow> = tx
        .fetch_optional(sqlx::query_as(GET_SQL).bind(id.as_uuid()))
        .await?;
    match row {
        Some(r) => item_from_row(r),
        None => Err(Error::NotFound(id)),
    }
}

async fn append_revision(tx: &mut Tx<'_>, item: ItemId, revision: &str) -> Result<()> {
    let (app, cfg) = stamps(tx).await?;
    tx.execute(
        sqlx::query(
            "INSERT INTO items.item_revision_history
                 (id, item_id, revision, application_version, configuration_version)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(item.as_uuid())
        .bind(revision)
        .bind(&app)
        .bind(&cfg),
    )
    .await?;
    Ok(())
}

async fn stamps(tx: &mut Tx<'_>) -> Result<(String, String)> {
    let app = datum_db::app_version();
    let cfg = tx.setting("datum.config_version").await?;
    Ok((app, cfg))
}

fn doc_ref(id: ItemId) -> DocRef {
    DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::from_uuid(id.as_uuid()),
    }
}

fn item_from_row(row: ItemRow) -> Result<Item> {
    let (
        id,
        number,
        revision,
        description,
        kind,
        stock_uom,
        stock_scale,
        residual_tolerance,
        cost_method,
        status,
        version,
        application_version,
        configuration_version,
        created_at,
        updated_at,
    ) = row;
    if !number_is_valid(&number) {
        return Err(Error::InvalidNumber);
    }
    Ok(Item {
        id: ItemId::from_uuid(id),
        number,
        revision,
        description,
        kind: Kind::parse(&kind)?,
        stock_uom: UnitId(stock_uom),
        stock_scale,
        residual_tolerance,
        cost_method,
        status: Status::parse(&status)?,
        version,
        application_version,
        configuration_version,
        created_at,
        updated_at,
    })
}

fn map_number_unique(err: datum_db::Error) -> Error {
    if let datum_db::Error::Sqlx(sql) = &err
        && sql.as_database_error().and_then(|d| d.constraint()) == Some("item_number_unique")
    {
        return Error::DuplicateNumber;
    }
    Error::Db(err)
}
