//! Lot and serial entities, expiry precision, package hierarchy, and status.
//!
//! No I/O. Identifier charset is `datum_numbering::lot::validate` (invariant 9).

use chrono::{Datelike, NaiveDate};
use datum_core::{AnyQuantity, ItemId, LotId, SerialId};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Default generator template when `create_lot` is not given a number.
pub const DEFAULT_LOT_TEMPLATE: &str = "LOT-{yyyy}-{0000}";
/// Default generator template for [`crate::store::create_serials`].
pub const DEFAULT_SERIAL_TEMPLATE: &str = "SN-{000000}";

/// Lot / serial status. Inventory posts the corresponding movement; this module
/// records the status and its history only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LotStatus {
    /// Received, not yet released.
    Quarantine,
    /// Released for use.
    Available,
    /// Temporarily unavailable.
    Hold,
    /// Rejected; not available.
    Rejected,
}

impl LotStatus {
    /// Wire / SQL token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quarantine => "quarantine",
            Self::Available => "available",
            Self::Hold => "hold",
            Self::Rejected => "rejected",
        }
    }

    /// Parse a stored token.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "quarantine" => Ok(Self::Quarantine),
            "available" => Ok(Self::Available),
            "hold" => Ok(Self::Hold),
            "rejected" => Ok(Self::Rejected),
            other => Err(Error::InvalidStatus(other.to_owned())),
        }
    }
}

/// Expiry precision. A bare `DATE` is forbidden (invariant 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpiryPrecision {
    /// Calendar day.
    Day,
    /// Calendar month; stored as the first of that month.
    Month,
    /// Calendar year; stored as 1 January.
    Year,
}

impl ExpiryPrecision {
    /// Wire / SQL token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    /// Parse a stored token.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "day" => Ok(Self::Day),
            "month" => Ok(Self::Month),
            "year" => Ok(Self::Year),
            other => Err(Error::Core(datum_core::Error::Invariant(format!(
                "unknown expiry precision {other}"
            )))),
        }
    }
}

/// Expiry as stored: a date plus the precision that produced it.
///
/// Month precision stores the first of the month; year precision stores 1 January.
/// The API must not invent a day on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expiry {
    /// Stored calendar date (first-of-month / 1 January when precision is coarser).
    pub date: NaiveDate,
    /// Precision the caller supplied.
    pub precision: ExpiryPrecision,
}

impl Expiry {
    /// Build from a caller date and a mandatory precision.
    ///
    /// Month → day forced to 1. Year → 1 January.
    pub fn new(date: NaiveDate, precision: ExpiryPrecision) -> Self {
        let date = match precision {
            ExpiryPrecision::Day => date,
            ExpiryPrecision::Month => {
                NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date)
            }
            ExpiryPrecision::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap_or(date),
        };
        Self { date, precision }
    }

    /// Parse a month-only value `YYYY-MM` into `{first-of-month, month}`.
    pub fn from_year_month(year: i32, month: u32) -> Result<Self> {
        let date = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| {
            Error::Core(datum_core::Error::Invariant(format!(
                "invalid year-month {year:04}-{month:02}"
            )))
        })?;
        Ok(Self {
            date,
            precision: ExpiryPrecision::Month,
        })
    }
}

/// Package level (invariant 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageLevel {
    /// Each / piece.
    Each,
    /// Inner pack.
    Inner,
    /// Case.
    Case,
    /// Pallet.
    Pallet,
}

impl PackageLevel {
    /// Wire / SQL token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Each => "each",
            Self::Inner => "inner",
            Self::Case => "case",
            Self::Pallet => "pallet",
        }
    }

    /// Parse a stored token.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "each" => Ok(Self::Each),
            "inner" => Ok(Self::Inner),
            "case" => Ok(Self::Case),
            "pallet" => Ok(Self::Pallet),
            other => Err(Error::Core(datum_core::Error::Invariant(format!(
                "unknown package level {other}"
            )))),
        }
    }
}

/// Package identifier (uuid v7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackageId(pub datum_core::Identifier);

impl PackageId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(datum_core::Identifier::generate())
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0.as_uuid()
    }

    /// Wrap an existing uuid.
    pub fn from_uuid(u: uuid::Uuid) -> Self {
        Self(datum_core::Identifier::from_uuid(u))
    }
}

/// A lot record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lot {
    /// Surrogate id.
    pub id: LotId,
    /// Item this lot is of. Not a FK into another module's tables.
    pub item: ItemId,
    /// Kernel identifier (`^[0-9A-Z-]{1,20}$`).
    pub number: String,
    /// Supplier lot cross-reference; never the kernel id.
    pub supplier_lot: Option<String>,
    /// Heat / source reference (free text).
    pub heat_or_source_ref: Option<String>,
    /// Server time of record.
    pub received_at: chrono::DateTime<chrono::Utc>,
    /// Expiry with precision, if any.
    pub expiry: Option<Expiry>,
    /// Certificate reference.
    pub cert_ref: Option<String>,
    /// Status.
    pub status: LotStatus,
    /// UDI device identifier attachment point (nullable, module-populated).
    pub udi_device_identifier: Option<String>,
    /// Optimistic concurrency version.
    pub version: i64,
}

/// A serial: always a unit within a lot (invariant 10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Serial {
    /// Surrogate id.
    pub id: SerialId,
    /// Owning lot. NOT NULL.
    pub lot: LotId,
    /// Kernel identifier.
    pub number: String,
    /// Status.
    pub status: LotStatus,
    /// UDI production identifier attachment point.
    pub udi_production_identifier: Option<String>,
    /// Optimistic concurrency version.
    pub version: i64,
}

/// One node in the package hierarchy (invariant 11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Surrogate id.
    pub id: PackageId,
    /// Owning lot.
    pub lot: LotId,
    /// Parent package, if any.
    pub parent: Option<PackageId>,
    /// Level.
    pub level: PackageLevel,
    /// Contained quantity.
    pub contained: AnyQuantity,
    /// Label reference.
    pub label_ref: Option<String>,
}

/// Append-only status history row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusHistory {
    /// Surrogate id.
    pub id: datum_core::Identifier,
    /// Lot subject, if this row is a lot status change.
    pub lot: Option<LotId>,
    /// Serial subject, if this row is a serial status change.
    pub serial: Option<SerialId>,
    /// Previous status, if any.
    pub from: Option<LotStatus>,
    /// New status.
    pub to: LotStatus,
    /// Caller reason.
    pub reason: String,
}

/// Arguments for [`crate::store::create_lot`].
#[derive(Debug, Clone)]
pub struct CreateLot {
    /// Item this lot is of.
    pub item: ItemId,
    /// Supplied kernel identifier. Validated; never taken from `supplier_lot`.
    pub number: Option<String>,
    /// Generator template when `number` is absent.
    pub template: Option<String>,
    /// Supplier lot cross-reference.
    pub supplier_lot: Option<String>,
    /// Heat / source reference.
    pub heat_or_source_ref: Option<String>,
    /// Expiry. Precision is mandatory when a date is present.
    pub expiry: Option<Expiry>,
    /// Certificate reference.
    pub cert_ref: Option<String>,
    /// Initial status. Defaults to quarantine.
    pub status: LotStatus,
}

impl Default for CreateLot {
    fn default() -> Self {
        Self {
            item: ItemId::from_uuid(uuid::Uuid::nil()),
            number: None,
            template: None,
            supplier_lot: None,
            heat_or_source_ref: None,
            expiry: None,
            cert_ref: None,
            status: LotStatus::Quarantine,
        }
    }
}

/// Status change target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTarget {
    /// A lot.
    Lot(LotId),
    /// A serial (unit within a lot).
    Serial(SerialId),
}

/// UDI attachment target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdiTarget {
    /// Lot: writes `udi_device_identifier`.
    Lot(LotId),
    /// Serial: writes `udi_production_identifier`.
    Serial(SerialId),
}

/// Validate a kernel lot/serial identifier (invariant 9).
pub fn validate_identifier(id: &str) -> Result<()> {
    datum_numbering::lot::validate(id).map_err(|_| Error::InvalidIdentifier(id.to_owned()))
}
