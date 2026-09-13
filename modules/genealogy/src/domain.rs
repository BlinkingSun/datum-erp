//! Genealogy query types. No I/O.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use wicket_core::{
    AnyQuantity, CurrencyId, Identifier, ItemId, LocationId, LotId, PostingId, SerialId,
};

/// Default posting-count threshold before a trace is enqueued as a job.
///
/// Traversals whose unique posting count is greater than this run as
/// `genealogy.trace` with progress. Override with [`crate::store::inline_max_postings`].
pub const DEFAULT_INLINE_MAX_POSTINGS: u32 = 32;

/// Environment key for [`DEFAULT_INLINE_MAX_POSTINGS`].
pub const INLINE_MAX_ENV: &str = "WICKET_GENEALOGY_INLINE_MAX";

/// Trace direction (SPEC query API).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Recursive CTE down `consumed_posting_id` (D2 §5.3).
    Backward,
    /// Reverse index up consuming postings.
    Forward,
    /// Both directions from the same start.
    Both,
}

impl Direction {
    /// Parse a query-string token.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "backward" => Some(Self::Backward),
            "forward" => Some(Self::Forward),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    /// Wire token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backward => "backward",
            Self::Forward => "forward",
            Self::Both => "both",
        }
    }
}

/// Start of a trace: a kernel lot, serial, or posting (never a text column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceOrigin {
    /// Kernel lot entity.
    Lot(LotId),
    /// Kernel serial entity (unit within a lot).
    Serial(SerialId),
    /// Immutable ledger posting id.
    Posting(PostingId),
}

/// Query parameters for [`crate::store::trace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceRequest {
    /// Start.
    pub origin: TraceOrigin,
    /// Direction.
    pub direction: Direction,
    /// Optional depth cap (root is depth 0).
    pub depth: Option<u32>,
    /// Override for the inline posting threshold. `None` uses the config default.
    pub max_postings: Option<u32>,
}

/// One node of the genealogy tree the mockup draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TreeNode {
    /// Ledger posting.
    pub posting: i64,
    /// Item on the lot, when the posting carries a lot.
    pub item: Option<ItemId>,
    /// Kernel lot.
    pub lot: Option<LotId>,
    /// Kernel serial.
    pub serial: Option<SerialId>,
    /// Location, when known from a published inventory document.
    pub location: Option<LocationId>,
    /// Stock quantity on the node (ledger `Node::quantity`).
    pub quantity: AnyQuantity,
    /// Edge amount into this node (wire shape of [`wicket_core::Money`]).
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    /// Currency of [`Self::amount`].
    pub amount_currency: CurrencyId,
    /// Time of record, when known.
    pub occurred_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Consumption-edge quantity into this node.
    pub edge_quantity: AnyQuantity,
    /// Recursed children.
    pub children: Vec<TreeNode>,
}

/// A forest returned by a single-direction trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tree {
    /// Direction that produced this forest.
    pub direction: Direction,
    /// Root nodes.
    pub nodes: Vec<TreeNode>,
}

/// Recall list: forward closure to CUSTOMER boundary postings (D2 case g).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Impact {
    /// Inventory shipment documents that reached CUSTOMER.
    pub shipments: Vec<Identifier>,
    /// Customer / sales-order references on those documents.
    pub customers: Vec<String>,
    /// Serial units on those shipments.
    pub units: Vec<SerialId>,
}

/// Export format (`?format=csv|json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JSON tree (default).
    Json,
    /// Flattened CSV rows.
    Csv,
}

impl ExportFormat {
    /// Parse `format` query param.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}
