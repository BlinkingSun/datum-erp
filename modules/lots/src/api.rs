//! HTTP DTOs and handlers. Routes are declared in `module.toml` and registered
//! through [`datum_module::KernelBuilder::apply_manifest`]. `datum-server` is the
//! only crate allowed `axum`; this module exposes typed handlers over [`datum_db::Tx`].

use chrono::Datelike;
use datum_core::{AnyQuantity, ItemId, LotId, SerialId};
use datum_db::{Tx, WriteContext};
use datum_module::Kernel;
use serde::{Deserialize, Serialize};

use crate::domain::{
    CreateLot, Expiry, ExpiryPrecision, Lot, LotStatus, Package, PackageLevel, Serial, StatusTarget,
};
use crate::error::{Error, Result};
use crate::store;

/// Wire expiry `{value, precision}` (`docs/10` §3.4). Month/year omit an invented day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpiryWire {
    /// Value formatted to the claimed precision (`YYYY-MM-DD` / `YYYY-MM` / `YYYY`).
    pub value: String,
    /// `day` / `month` / `year`.
    pub precision: ExpiryPrecision,
}

impl From<Expiry> for ExpiryWire {
    fn from(e: Expiry) -> Self {
        let value = match e.precision {
            ExpiryPrecision::Day => e.date.format("%Y-%m-%d").to_string(),
            ExpiryPrecision::Month => e.date.format("%Y-%m").to_string(),
            ExpiryPrecision::Year => e.date.format("%Y").to_string(),
        };
        Self {
            value,
            precision: e.precision,
        }
    }
}

impl ExpiryWire {
    /// Parse a wire expiry. Month `YYYY-MM` stores the first of the month.
    pub fn into_expiry(self) -> Result<Expiry> {
        match self.precision {
            ExpiryPrecision::Day => {
                let date =
                    chrono::NaiveDate::parse_from_str(&self.value, "%Y-%m-%d").map_err(|e| {
                        Error::Core(datum_core::Error::Invariant(format!("expiry.value: {e}")))
                    })?;
                Ok(Expiry::new(date, ExpiryPrecision::Day))
            }
            ExpiryPrecision::Month => {
                let date =
                    chrono::NaiveDate::parse_from_str(&format!("{}-01", self.value), "%Y-%m-%d")
                        .or_else(|_| chrono::NaiveDate::parse_from_str(&self.value, "%Y-%m-%d"))
                        .map_err(|e| {
                            Error::Core(datum_core::Error::Invariant(format!("expiry.value: {e}")))
                        })?;
                if date.day() != 1 && self.value.len() > 7 {
                    return Err(Error::Core(datum_core::Error::Invariant(
                        "expiry.value carries a day for month precision".into(),
                    )));
                }
                Ok(Expiry::from_year_month(date.year(), date.month())?)
            }
            ExpiryPrecision::Year => {
                let year: i32 = self.value.parse().map_err(|_| {
                    Error::Core(datum_core::Error::Invariant("expiry.value year".into()))
                })?;
                Ok(Expiry::new(
                    chrono::NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(|| {
                        Error::Core(datum_core::Error::Invariant("expiry.value year".into()))
                    })?,
                    ExpiryPrecision::Year,
                ))
            }
        }
    }
}

/// Lot JSON body (GET).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LotBody {
    /// Surrogate id.
    pub id: LotId,
    /// Kernel identifier.
    pub identifier: String,
    /// Item id.
    pub item_id: ItemId,
    /// Supplier lot cross-reference.
    pub supplier_lot: Option<String>,
    /// Heat / source reference.
    pub heat: Option<String>,
    /// Expiry as `{date, precision}`.
    pub expiry: Option<ExpiryWire>,
    /// Certificate reference.
    pub cert_ref: Option<String>,
    /// Status.
    pub status: LotStatus,
    /// UDI-DI attachment (nullable).
    pub udi_device_identifier: Option<String>,
    /// Version.
    pub version: i64,
    /// Server time of record.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Lot> for LotBody {
    fn from(lot: Lot) -> Self {
        Self {
            id: lot.id,
            identifier: lot.number,
            item_id: lot.item,
            supplier_lot: lot.supplier_lot,
            heat: lot.heat_or_source_ref,
            expiry: lot.expiry.map(ExpiryWire::from),
            cert_ref: lot.cert_ref,
            status: lot.status,
            udi_device_identifier: lot.udi_device_identifier,
            version: lot.version,
            created_at: lot.received_at,
        }
    }
}

/// POST `/api/v1/lots` body.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateLotBody {
    /// Item id (client-supplied ids on `id` are refused by the server).
    pub item_id: ItemId,
    /// Optional supplied kernel identifier.
    pub identifier: Option<String>,
    /// Generator template when `identifier` is absent.
    pub template: Option<String>,
    /// Supplier lot cross-reference.
    pub supplier_lot: Option<String>,
    /// Heat / source reference.
    pub heat: Option<String>,
    /// Expiry `{date, precision}`.
    pub expiry: Option<ExpiryWire>,
    /// Certificate reference.
    pub cert_ref: Option<String>,
    /// Initial status.
    pub status: Option<LotStatus>,
}

/// List envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ListBody<T> {
    /// Page.
    pub data: Vec<T>,
    /// Opaque next cursor.
    pub next_cursor: Option<String>,
    /// Whether another page exists.
    pub has_more: bool,
}

/// Serial JSON body.
#[derive(Debug, Clone, Serialize)]
pub struct SerialBody {
    /// Surrogate id.
    pub id: SerialId,
    /// Owning lot.
    pub lot_id: LotId,
    /// Kernel identifier.
    pub identifier: String,
    /// Status.
    pub status: LotStatus,
    /// UDI-PI attachment.
    pub udi_production_identifier: Option<String>,
    /// Version.
    pub version: i64,
}

impl From<Serial> for SerialBody {
    fn from(s: Serial) -> Self {
        Self {
            id: s.id,
            lot_id: s.lot,
            identifier: s.number,
            status: s.status,
            udi_production_identifier: s.udi_production_identifier,
            version: s.version,
        }
    }
}

/// POST `/api/v1/lots/{id}/serials` body.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSerialsBody {
    /// How many serials to allocate.
    pub count: u32,
    /// Generator template.
    pub template: Option<String>,
}

/// POST `/api/v1/lots/{id}/status` body.
#[derive(Debug, Clone, Deserialize)]
pub struct SetStatusBody {
    /// New status.
    pub status: LotStatus,
    /// Reason (required).
    pub reason: String,
}

/// Package JSON body. Quantity is `AnyQuantity`.
#[derive(Debug, Clone, Serialize)]
pub struct PackageBody {
    /// Surrogate id.
    pub id: crate::domain::PackageId,
    /// Owning lot.
    pub lot_id: LotId,
    /// Parent package.
    pub parent_id: Option<crate::domain::PackageId>,
    /// Level.
    pub level: PackageLevel,
    /// Contained quantity.
    pub contained_quantity: AnyQuantity,
    /// Label reference.
    pub label_ref: Option<String>,
}

impl From<Package> for PackageBody {
    fn from(p: Package) -> Self {
        Self {
            id: p.id,
            lot_id: p.lot,
            parent_id: p.parent,
            level: p.level,
            contained_quantity: p.contained,
            label_ref: p.label_ref,
        }
    }
}

/// GET `/api/v1/lots`.
pub async fn list_lots(
    tx: &mut Tx<'_>,
    limit: Option<i64>,
    cursor: Option<&str>,
) -> Result<ListBody<LotBody>> {
    let limit = limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(Error::InvalidLimit);
    }
    let cursor = match cursor {
        Some(c) if !c.is_empty() => Some(parse_lot_id(c)?),
        _ => None,
    };
    let (rows, next, has_more) = store::list_lots(tx, limit, cursor).await?;
    Ok(ListBody {
        data: rows.into_iter().map(LotBody::from).collect(),
        next_cursor: next.map(|id| id.to_string()),
        has_more,
    })
}

/// POST `/api/v1/lots`.
pub async fn create_lot(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &WriteContext,
    body: CreateLotBody,
) -> Result<LotBody> {
    let expiry = match body.expiry {
        Some(w) => Some(w.into_expiry()?),
        None => None,
    };
    let lot = store::create_lot(
        tx,
        kernel,
        ctx,
        CreateLot {
            item: body.item_id,
            number: body.identifier,
            template: body.template,
            supplier_lot: body.supplier_lot,
            heat_or_source_ref: body.heat,
            expiry,
            cert_ref: body.cert_ref,
            status: body.status.unwrap_or(LotStatus::Quarantine),
        },
    )
    .await?;
    Ok(LotBody::from(lot))
}

/// GET `/api/v1/lots/{id}`.
pub async fn get_lot(tx: &mut Tx<'_>, id: LotId) -> Result<LotBody> {
    Ok(LotBody::from(store::load_lot(tx, id).await?))
}

/// GET `/api/v1/lots/{id}/serials`.
pub async fn list_serials(
    tx: &mut Tx<'_>,
    id: LotId,
    limit: Option<i64>,
    cursor: Option<&str>,
) -> Result<ListBody<SerialBody>> {
    let limit = limit.unwrap_or(50);
    let cursor = match cursor {
        Some(c) if !c.is_empty() => Some(parse_serial_id(c)?),
        _ => None,
    };
    let (rows, next, has_more) = store::list_serials(tx, id, limit, cursor).await?;
    Ok(ListBody {
        data: rows.into_iter().map(SerialBody::from).collect(),
        next_cursor: next.map(|id| id.to_string()),
        has_more,
    })
}

/// POST `/api/v1/lots/{id}/serials`.
pub async fn create_serials(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    id: LotId,
    body: CreateSerialsBody,
) -> Result<ListBody<SerialBody>> {
    let rows = store::create_serials(tx, kernel, id, body.count, body.template.as_deref()).await?;
    Ok(ListBody {
        data: rows.into_iter().map(SerialBody::from).collect(),
        next_cursor: None,
        has_more: false,
    })
}

/// POST `/api/v1/lots/{id}/status`.
pub async fn set_status(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &WriteContext,
    id: LotId,
    body: SetStatusBody,
) -> Result<LotBody> {
    store::set_status(
        tx,
        kernel,
        ctx.actor,
        StatusTarget::Lot(id),
        body.status,
        &body.reason,
    )
    .await?;
    get_lot(tx, id).await
}

/// GET `/api/v1/lots/{id}/packages`.
pub async fn list_packages(
    tx: &mut Tx<'_>,
    id: LotId,
    limit: Option<i64>,
    cursor: Option<&str>,
) -> Result<ListBody<PackageBody>> {
    let limit = limit.unwrap_or(50);
    let cursor = match cursor {
        Some(c) if !c.is_empty() => Some(parse_package_id(c)?),
        _ => None,
    };
    let (rows, next, has_more) = store::list_packages(tx, id, limit, cursor).await?;
    Ok(ListBody {
        data: rows.into_iter().map(PackageBody::from).collect(),
        next_cursor: next.map(|id| id.as_uuid().to_string()),
        has_more,
    })
}

fn parse_lot_id(s: &str) -> Result<LotId> {
    let u = uuid::Uuid::parse_str(s)
        .map_err(|e| Error::Core(datum_core::Error::Invariant(e.to_string())))?;
    Ok(LotId::from_uuid(u))
}

fn parse_serial_id(s: &str) -> Result<SerialId> {
    let u = uuid::Uuid::parse_str(s)
        .map_err(|e| Error::Core(datum_core::Error::Invariant(e.to_string())))?;
    Ok(SerialId::from_uuid(u))
}

fn parse_package_id(s: &str) -> Result<crate::domain::PackageId> {
    let u = uuid::Uuid::parse_str(s)
        .map_err(|e| Error::Core(datum_core::Error::Invariant(e.to_string())))?;
    Ok(crate::domain::PackageId::from_uuid(u))
}
