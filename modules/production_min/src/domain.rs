//! Work-order entities and operation inputs. No I/O.

use chrono::{DateTime, Utc};
use datum_core::{AnyQuantity, Identifier, ItemId, LocationId, LotId, Money, SerialId, UnitId};
use datum_mod_inventory::LineInput;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

/// Document type registered on the work-order state machine.
pub const DOC_TYPE: &str = "production";

/// Reason stamped on every signature declaration (SPEC).
pub const NOT_REQUIRED_REASON: &str =
    "v1 minimal work order; signature points belong to the Phase 3 production module";

/// Default finished-lot generator template (kernel identifier charset, ≤20).
pub const DEFAULT_FINISHED_LOT_TEMPLATE: &str = "LOT-WO-{0000}";

/// Work-order lifecycle. Advanced only by the declared state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Editable working copy; no number yet.
    Draft,
    /// Number allocated; WIP location exists.
    Released,
    /// Material has been issued.
    InProcess,
    /// Finished lot received; transformation posted.
    Completed,
    /// Cancelled from draft or released.
    Cancelled,
}

impl Status {
    /// SQL / wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Released => "released",
            Self::InProcess => "in_process",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse a SQL / wire label.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "draft" => Ok(Self::Draft),
            "released" => Ok(Self::Released),
            "in_process" => Ok(Self::InProcess),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(Error::Manifest(format!("unknown status {other}"))),
        }
    }
}

/// Persisted work order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOrder {
    /// Surrogate key.
    pub id: Identifier,
    /// Gap-free `WO-…` allocated at release, not at create.
    pub number: Option<String>,
    /// Finished item.
    pub item: ItemId,
    /// Ordered quantity as entered.
    pub quantity_ordered: AnyQuantity,
    /// Item revision captured at create.
    pub revision: String,
    /// Lifecycle status (denormalized from the state machine).
    pub status: Status,
    /// WIP location allocated at release.
    pub wip_location: Option<LocationId>,
    /// Server time of release.
    pub released_at: Option<DateTime<Utc>>,
    /// Server time of completion.
    pub completed_at: Option<DateTime<Utc>>,
    /// Optimistic version.
    pub version: i64,
    /// Application version that wrote the row (invariant 17).
    pub application_version: String,
    /// Configuration version that wrote the row (invariant 17).
    pub configuration_version: String,
}

/// Persisted completion row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    /// Surrogate key.
    pub id: Identifier,
    /// Owning work order.
    pub work_order: Identifier,
    /// Finished lot entity (kernel identifier).
    pub finished_lot: LotId,
    /// Good quantity in stock units.
    pub quantity_good: AnyQuantity,
    /// Scrap quantity in stock units.
    pub quantity_scrap: AnyQuantity,
    /// TRANSFORMATION group id.
    pub group_id: Identifier,
}

/// Recorded issue line (this module's copy of what `inventory::issue_to_wip` posted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueLine {
    /// Surrogate key.
    pub id: Identifier,
    /// Owning work order.
    pub work_order: Identifier,
    /// Inventory document produced by the issue.
    pub inventory_document: Identifier,
    /// Component item.
    pub item: ItemId,
    /// Kernel lot entity.
    pub lot: Option<LotId>,
    /// Kernel serial entity.
    pub serial: Option<SerialId>,
    /// Canonical quantity issued.
    pub quantity: AnyQuantity,
    /// Material value, if the operator supplied one.
    pub amount: Option<Money>,
}

/// Create payload. The server mints the id.
#[derive(Debug, Clone)]
pub struct CreateWorkOrder {
    /// Finished item.
    pub item: ItemId,
    /// Ordered quantity (AnyQuantity; converted at posting boundaries).
    pub quantity_ordered: AnyQuantity,
    /// Item revision to pin on the order.
    pub revision: String,
}

/// Issue material to the work order's WIP location.
#[derive(Debug, Clone)]
pub struct IssueMaterialRequest {
    /// Work order.
    pub work_order: Identifier,
    /// Source location (available stock).
    pub from_location: LocationId,
    /// Component lines. Named lots contribute explicit consumption in inventory.
    pub lines: Vec<LineInput>,
    /// Idempotency key forwarded to inventory.
    pub idempotency_key: Option<Uuid>,
}

/// Finished-lot identity for [`CompleteRequest`].
#[derive(Debug, Clone, Default)]
pub struct FinishedLotTemplate {
    /// Supplied kernel identifier. Validated; never a text column on a posting.
    pub number: Option<String>,
    /// Generator template when `number` is absent.
    pub template: Option<String>,
    /// When set, allocate this many serials as units within the finished lot.
    pub serial_template: Option<String>,
}

/// Complete a work order: priced TRANSFORMATION plus optional scrap ADJUSTMENT.
#[derive(Debug, Clone)]
pub struct CompleteRequest {
    /// Work order.
    pub work_order: Identifier,
    /// Finished-goods location.
    pub to_location: LocationId,
    /// Good quantity.
    pub good: AnyQuantity,
    /// Scrap quantity (zero if none). Posted as a separate ADJUSTMENT (D2 case e).
    pub scrap: AnyQuantity,
    /// Finished lot identity.
    pub finished_lot: FinishedLotTemplate,
}

/// Cursor page.
#[derive(Debug, Clone)]
pub struct Page<T> {
    /// Rows.
    pub data: Vec<T>,
    /// Next cursor, if [`Self::has_more`].
    pub next_cursor: Option<String>,
    /// True when another page exists.
    pub has_more: bool,
}

/// List filters.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Status equality.
    pub status: Option<Status>,
    /// Opaque cursor (work-order id).
    pub cursor: Option<Identifier>,
    /// Page size. Default 50, max 200.
    pub limit: Option<u32>,
}

/// Parse a stored dimension token (`Debug` of [`datum_core::DimensionKind`]).
pub fn dimension_from_sql(s: &str) -> Result<datum_core::DimensionKind> {
    match s {
        "Count" => Ok(datum_core::DimensionKind::Count),
        "Length" => Ok(datum_core::DimensionKind::Length),
        "Mass" => Ok(datum_core::DimensionKind::Mass),
        "Time" => Ok(datum_core::DimensionKind::Time),
        "Volume" => Ok(datum_core::DimensionKind::Volume),
        "Area" => Ok(datum_core::DimensionKind::Area),
        other => Err(Error::Manifest(format!("unknown dimension {other}"))),
    }
}

/// Reconstruct an [`AnyQuantity`] from stored columns.
pub fn quantity_from_parts(amount: Decimal, uom: i64, dimension: &str) -> Result<AnyQuantity> {
    Ok(AnyQuantity {
        amount,
        unit: UnitId(uom),
        dimension: dimension_from_sql(dimension)?,
    })
}
