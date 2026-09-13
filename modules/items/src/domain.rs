//! Item entity and rules. No I/O.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use wicket_core::{ItemId, Money, UnitId};
use wicket_ledger::CostMethod;

use crate::error::{Error, Result};

/// Document type registered on the items state machine.
pub const DOC_TYPE: &str = "items";

/// Reason stamped on every signature declaration (SPEC release).
pub const NOT_REQUIRED_REASON: &str = "item release is not a regulated signature point in v1";

/// Maximum length of [`Item::number`].
pub const NUMBER_MAX_LEN: usize = 40;

/// Part kind (`docs/04` Phase 1 `items` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Manufactured.
    Make,
    /// Purchased.
    Buy,
    /// Non-stocked service.
    Service,
    /// Phantom (blow-through) assembly.
    Phantom,
}

impl Kind {
    /// SQL / wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Make => "make",
            Self::Buy => "buy",
            Self::Service => "service",
            Self::Phantom => "phantom",
        }
    }

    /// Parse a SQL / wire label.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "make" => Ok(Self::Make),
            "buy" => Ok(Self::Buy),
            "service" => Ok(Self::Service),
            "phantom" => Ok(Self::Phantom),
            other => Err(Error::Manifest(format!("unknown kind {other}"))),
        }
    }
}

/// Lifecycle status. Advanced only by the declared state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Editable working copy.
    Draft,
    /// Released for use.
    Released,
    /// Retired; not releasable again.
    Obsolete,
}

impl Status {
    /// SQL / wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Released => "released",
            Self::Obsolete => "obsolete",
        }
    }

    /// Parse a SQL / wire label.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "draft" => Ok(Self::Draft),
            "released" => Ok(Self::Released),
            "obsolete" => Ok(Self::Obsolete),
            other => Err(Error::Manifest(format!("unknown status {other}"))),
        }
    }
}

/// `true` when `number` is A–Z / a–z / digits / hyphen / `.`, length 1..=40.
///
/// SPEC names uppercase; PLAN §3's binding example `MDS-450-M4x12` includes a
/// lowercase `x`, so a–z is accepted and not coerced.
pub fn number_is_valid(number: &str) -> bool {
    let len = number.len();
    (1..=NUMBER_MAX_LEN).contains(&len)
        && number
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Create payload. The server mints [`NewItem`]'s id; callers must not set one.
#[derive(Debug, Clone)]
pub struct NewItem {
    /// Human item number (`MDS-450-M4x12`).
    pub number: String,
    /// Revision (`C`).
    pub revision: String,
    /// Description.
    pub description: String,
    /// Make / buy / service / phantom.
    pub kind: Kind,
    /// Stocking unit (`UnitId` of a kernel dimension, D1).
    pub stock_uom: UnitId,
    /// Declared scale (0–8).
    pub stock_scale: i16,
    /// Dust bound.
    pub residual_tolerance: Decimal,
    /// Cost method written through `wicket_ledger::registry`.
    pub cost_method: CostMethod,
    /// Required iff [`CostMethod::Standard`].
    pub standard: Option<Money>,
}

impl NewItem {
    /// Validate number charset and standard-cost pairing.
    pub fn validate(&self) -> Result<()> {
        if !number_is_valid(&self.number) {
            return Err(Error::InvalidNumber);
        }
        if (self.stock_scale < 0) || (self.stock_scale > 8) {
            return Err(Error::Manifest("stock_scale must be 0..=8".into()));
        }
        if self.residual_tolerance.is_sign_negative() {
            return Err(Error::Manifest("residual_tolerance must be >= 0".into()));
        }
        let standard = matches!(self.cost_method, CostMethod::Standard);
        if standard != self.standard.is_some() {
            return Err(Error::StandardCostRequired);
        }
        Ok(())
    }
}

/// Patch payload. `version` is the optimistic-concurrency token.
#[derive(Debug, Clone)]
pub struct UpdateItem {
    /// Expected version.
    pub version: i64,
    /// Optional new revision (appends history when it changes).
    pub revision: Option<String>,
    /// Optional new description.
    pub description: Option<String>,
    /// Optional new kind.
    pub kind: Option<Kind>,
    /// Optional new stocking unit (refused after the first posting).
    pub stock_uom: Option<UnitId>,
    /// Optional new scale (refused after the first posting).
    pub stock_scale: Option<i16>,
    /// Optional new residual tolerance (refused after the first posting).
    pub residual_tolerance: Option<Decimal>,
    /// Optional new cost method.
    pub cost_method: Option<CostMethod>,
    /// Standard cost when switching to or remaining on [`CostMethod::Standard`].
    pub standard: Option<Money>,
}

/// List filters. Unknown names are rejected at the HTTP boundary.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Kind equality.
    pub kind: Option<Kind>,
    /// Status equality.
    pub status: Option<Status>,
    /// Number prefix (`MDS-`).
    pub number_prefix: Option<String>,
    /// Opaque cursor (item id).
    pub cursor: Option<ItemId>,
    /// Page size. Default 50, max 200.
    pub limit: Option<u32>,
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

/// Persisted item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Surrogate key.
    pub id: ItemId,
    /// Human number.
    pub number: String,
    /// Current revision.
    pub revision: String,
    /// Description.
    pub description: String,
    /// Kind.
    pub kind: Kind,
    /// Stocking unit.
    pub stock_uom: UnitId,
    /// Declared scale.
    pub stock_scale: i16,
    /// Dust bound.
    pub residual_tolerance: Decimal,
    /// Cost method (SQL label).
    pub cost_method: String,
    /// Lifecycle status.
    pub status: Status,
    /// Optimistic version.
    pub version: i64,
    /// Application version that wrote the row (invariant 17).
    pub application_version: String,
    /// Configuration version that wrote the row (invariant 17).
    pub configuration_version: String,
    /// Server create time.
    pub created_at: DateTime<Utc>,
    /// Server update time.
    pub updated_at: DateTime<Utc>,
}

/// Map a [`CostMethod`] to the ledger SQL label.
pub fn cost_method_sql(method: CostMethod) -> &'static str {
    wicket_ledger::cost_method_sql(method)
}

/// Parse a ledger SQL cost-method label.
pub fn cost_method_from_sql(label: &str) -> Result<CostMethod> {
    wicket_ledger::cost_method_from_sql(label)
        .ok_or_else(|| Error::Manifest(format!("unknown cost_method {label}")))
}
