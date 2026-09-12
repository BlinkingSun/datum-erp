//! Crate errors.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from lot/serial identity, expiry, packages, and UDI attachment.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] datum_db::Error),
    /// Numbering / identifier generation.
    #[error(transparent)]
    Numbering(#[from] datum_numbering::Error),
    /// Event publish or schema registration.
    #[error(transparent)]
    Events(#[from] datum_events::Error),
    /// Module manifest / kernel registration.
    #[error(transparent)]
    Module(#[from] datum_module::Error),
    /// Identifier failed `^[0-9A-Z-]{1,20}$`.
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),
    /// Expiry date was supplied without a precision (invariant 12).
    #[error("expiry precision is required when a date is supplied")]
    ExpiryPrecisionRequired,
    /// Precision was supplied without a date.
    #[error("expiry date is required when precision is supplied")]
    ExpiryDateRequired,
    /// Lot or serial was not found.
    #[error("not found")]
    NotFound,
    /// A serial must be a unit within a lot (invariant 10).
    #[error("serial requires a lot")]
    SerialRequiresLot,
    /// Status is not one of quarantine|available|hold|rejected.
    #[error("invalid status: {0}")]
    InvalidStatus(String),
    /// Package parent is missing or belongs to a different lot.
    #[error("package parent is not in this lot")]
    InvalidPackageParent,
    /// Pagination `limit` is outside 1..=200.
    #[error("invalid limit")]
    InvalidLimit,
    /// Lot number `{0}` is not registered.
    #[error("unknown lot number {0}")]
    UnknownNumber(String),
    /// JSON / serde failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// State-machine error.
    #[error(transparent)]
    Statemachine(#[from] datum_statemachine::Error),
    /// Status cannot move along this edge.
    #[error("invalid transition {edge} from status {status}")]
    InvalidTransition {
        /// Edge or jump attempted.
        edge: String,
        /// Current status.
        status: String,
    },
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}
