//! Inventory document entities and posting inputs. No I/O.

use datum_core::{AnyQuantity, Identifier, ItemId, LocationId, LotId, Money, SerialId};
use lots::PackageId;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Document type registered on the inventory state machine.
pub const DOC_TYPE: &str = "inventory";

/// Reason stamped on every signature declaration.
pub const NOT_REQUIRED_REASON: &str = "inventory posting is not a regulated signature point in v1";

/// Document kind (`docs/04` Phase 1 `inventory` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentKind {
    /// Receipt from a supplier.
    Receipt,
    /// Issue to a work order.
    Issue,
    /// Location-to-location move.
    Move,
    /// Reason-coded adjustment.
    Adjustment,
    /// Cycle count.
    Count,
}

impl DocumentKind {
    /// SQL / wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receipt => "receipt",
            Self::Issue => "issue",
            Self::Move => "move",
            Self::Adjustment => "adjustment",
            Self::Count => "count",
        }
    }

    /// Parse a SQL / wire label.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "receipt" => Ok(Self::Receipt),
            "issue" => Ok(Self::Issue),
            "move" => Ok(Self::Move),
            "adjustment" => Ok(Self::Adjustment),
            "count" => Ok(Self::Count),
            other => Err(Error::Manifest(format!("unknown kind {other}"))),
        }
    }

    /// State-machine edge used to post this kind.
    pub fn post_edge(self) -> &'static str {
        match self {
            Self::Receipt => "receive",
            Self::Issue => "issue",
            Self::Move => "move",
            Self::Adjustment => "adjust",
            Self::Count => "count",
        }
    }
}

/// Document lifecycle. Advanced only by the declared state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentStatus {
    /// Working copy, not yet posted.
    Draft,
    /// Posted to the ledger.
    Posted,
    /// Voided; the ledger group remains.
    Voided,
}

impl DocumentStatus {
    /// SQL / wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Posted => "posted",
            Self::Voided => "voided",
        }
    }

    /// Parse a SQL / wire label.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "draft" => Ok(Self::Draft),
            "posted" => Ok(Self::Posted),
            "voided" => Ok(Self::Voided),
            other => Err(Error::Manifest(format!("unknown status {other}"))),
        }
    }
}

/// Persisted inventory document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// Surrogate key.
    pub id: Identifier,
    /// Kind.
    pub kind: DocumentKind,
    /// Lifecycle status.
    pub status: DocumentStatus,
    /// PO / WO / order reference.
    pub reference: Option<String>,
    /// Ledger group this document produced.
    pub posted_group_id: Option<Identifier>,
    /// Optimistic version.
    pub version: i64,
    /// Application version that wrote the row (invariant 17).
    pub application_version: String,
    /// Configuration version that wrote the row (invariant 17).
    pub configuration_version: String,
    /// Lines.
    pub lines: Vec<DocumentLine>,
}

/// Persisted document line. `lot_id` / `serial_id` are kernel entities, never text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentLine {
    /// Surrogate key.
    pub id: Identifier,
    /// Owning document.
    pub document_id: Identifier,
    /// Item.
    pub item: ItemId,
    /// Kernel lot entity.
    pub lot: Option<LotId>,
    /// Kernel serial entity.
    pub serial: Option<SerialId>,
    /// Source location.
    pub from_location: Option<LocationId>,
    /// Destination location.
    pub to_location: Option<LocationId>,
    /// Operator-entered quantity (provenance).
    pub entered: AnyQuantity,
    /// Canonical stock quantity.
    pub canonical: AnyQuantity,
    /// Conversion factor applied at the boundary (D2 R2).
    pub conversion_factor: rust_decimal::Decimal,
    /// Reason on an adjustment line.
    pub reason_code: Option<String>,
    /// Package entity, if the operator scanned a package.
    pub package: Option<PackageId>,
}

/// One line of a posting request.
#[derive(Debug, Clone)]
pub struct LineInput {
    /// Item.
    pub item: ItemId,
    /// Operator-entered quantity.
    pub entered: AnyQuantity,
    /// Kernel lot entity when the operator named one.
    pub lot: Option<LotId>,
    /// Kernel serial entity when the operator named one.
    pub serial: Option<SerialId>,
    /// Source location.
    pub from_location: Option<LocationId>,
    /// Destination location.
    pub to_location: Option<LocationId>,
    /// Package entity.
    pub package: Option<PackageId>,
    /// Optional inventory value for this line (material).
    pub amount: Option<Money>,
    /// Line-level reason (adjustments).
    pub reason_code: Option<String>,
}

/// Receive against a named location (quarantine by default per the lot).
#[derive(Debug, Clone)]
pub struct ReceiveRequest {
    /// Destination location.
    pub to_location: LocationId,
    /// PO / source reference.
    pub reference: Option<String>,
    /// Lines.
    pub lines: Vec<LineInput>,
    /// Expected quantity on the source document (PO). Tolerance is document-level.
    pub expected: Option<AnyQuantity>,
    /// Over-receipt tolerance in the stock unit. `None` means no check.
    pub tolerance: Option<rust_decimal::Decimal>,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Issue to a work order.
#[derive(Debug, Clone)]
pub struct IssueRequest {
    /// Work order id (`WO-2026-1847` is the human number; this is the entity).
    pub work_order: Identifier,
    /// Source location (available stock).
    pub from_location: LocationId,
    /// WO / source reference.
    pub reference: Option<String>,
    /// Lines. Named lots contribute explicit `Consumption` intents (D-W1-3 (c)).
    pub lines: Vec<LineInput>,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Location-to-location move.
#[derive(Debug, Clone)]
pub struct MoveRequest {
    /// Source.
    pub from_location: LocationId,
    /// Destination.
    pub to_location: LocationId,
    /// Reference.
    pub reference: Option<String>,
    /// Lines.
    pub lines: Vec<LineInput>,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Reason-coded adjustment.
#[derive(Debug, Clone)]
pub struct AdjustRequest {
    /// Required reason code.
    pub reason: String,
    /// Location whose on-hand changes.
    pub location: LocationId,
    /// Reference.
    pub reference: Option<String>,
    /// Lines. Canonical quantity is the signed variance at `location`.
    pub lines: Vec<LineInput>,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Cycle count at one location.
#[derive(Debug, Clone)]
pub struct CountRequest {
    /// Counted location.
    pub location: LocationId,
    /// Reference (count sheet).
    pub reference: Option<String>,
    /// Lines: `entered` is the counted quantity; `expected` is the source-document quantity.
    pub lines: Vec<CountLine>,
    /// Tolerance against the source document, never a ledger invariant.
    pub tolerance: rust_decimal::Decimal,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// One counted item.
#[derive(Debug, Clone)]
pub struct CountLine {
    /// Item.
    pub item: ItemId,
    /// Lot.
    pub lot: Option<LotId>,
    /// Serial.
    pub serial: Option<SerialId>,
    /// Counted quantity (entered).
    pub counted: AnyQuantity,
    /// Source-document quantity the count is checked against.
    pub expected: AnyQuantity,
    /// Optional inventory value of the variance.
    pub amount: Option<Money>,
}

/// Release a lot from quarantine into available (D2 case b).
#[derive(Debug, Clone)]
pub struct ReleaseRequest {
    /// Lot entity.
    pub lot: LotId,
    /// Quarantine location.
    pub from_location: LocationId,
    /// Available location.
    pub to_location: LocationId,
    /// Quantity to release (entered).
    pub entered: AnyQuantity,
    /// Optional inventory value.
    pub amount: Option<Money>,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Ship to a customer (D2 case g).
#[derive(Debug, Clone)]
pub struct ShipRequest {
    /// Sales order entity.
    pub order: Identifier,
    /// Source (typically FG).
    pub from_location: LocationId,
    /// Reference.
    pub reference: Option<String>,
    /// Lines.
    pub lines: Vec<LineInput>,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Customer return into quarantine (D2 case h).
#[derive(Debug, Clone)]
pub struct ReturnRequest {
    /// Sales order entity.
    pub order: Identifier,
    /// Quarantine destination.
    pub to_location: LocationId,
    /// Reference.
    pub reference: Option<String>,
    /// Lines.
    pub lines: Vec<LineInput>,
    /// Idempotency key.
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Projection slice for [`crate::store::on_hand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BalanceQuery {
    /// Item.
    pub item: ItemId,
    /// Location filter.
    pub location: Option<LocationId>,
    /// Lot filter.
    pub lot: Option<LotId>,
}
