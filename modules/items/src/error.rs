//! Crate error type.

use wicket_core::ItemId;

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from validation, persistence, release, and registry sync.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
    /// Ledger registry or posting error.
    #[error(transparent)]
    Ledger(#[from] wicket_ledger::Error),
    /// Unit catalog error.
    #[error(transparent)]
    Uom(#[from] wicket_uom::Error),
    /// Event publish / schema error.
    #[error(transparent)]
    Events(#[from] wicket_events::Error),
    /// State-machine error.
    #[error(transparent)]
    Statemachine(#[from] wicket_statemachine::Error),
    /// Composition-root error.
    #[error(transparent)]
    Module(#[from] wicket_module::Error),
    /// JSON error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Item number failed the charset / length rule.
    #[error("item number must be A-Z, a-z, digits, hyphen, or '.', at most 40 characters")]
    InvalidNumber,
    /// Item number already exists (`item_number_unique`).
    #[error("item number already exists")]
    DuplicateNumber,
    /// Client supplied an id on create.
    #[error("clients must not mint identifiers")]
    ClientMintedId,
    /// List `limit` outside 1..=200.
    #[error("limit must be between 1 and 200")]
    InvalidLimit,
    /// Item was not found.
    #[error("item not found: {0}")]
    NotFound(ItemId),
    /// Optimistic version did not match.
    #[error("version conflict")]
    VersionConflict,
    /// Stock unit, scale, or residual tolerance cannot change while postings exist (D2 R5).
    #[error("stock unit, scale, and residual tolerance are immutable while postings exist")]
    StockMeasureImmutable,
    /// `STANDARD` costing requires a standard cost; other methods forbid one.
    #[error("standard cost is required if and only if cost_method is STANDARD")]
    StandardCostRequired,
    /// Status cannot move along this edge.
    #[error("cannot {edge} item in status {status}")]
    InvalidTransition {
        /// Requested edge.
        edge: String,
        /// Live status.
        status: String,
    },
    /// Manifest failed to parse.
    #[error("manifest: {0}")]
    Manifest(String),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}

impl Error {
    /// `docs/10` §2.6 `code` token.
    pub fn code(&self) -> &'static str {
        match self {
            Error::DuplicateNumber | Error::VersionConflict => "CONFLICT",
            Error::NotFound(_) => "NOT_FOUND",
            Error::InvalidNumber
            | Error::ClientMintedId
            | Error::InvalidLimit
            | Error::StockMeasureImmutable
            | Error::StandardCostRequired
            | Error::InvalidTransition { .. }
            | Error::Manifest(_) => "VALIDATION",
            _ => "INTERNAL",
        }
    }
}
