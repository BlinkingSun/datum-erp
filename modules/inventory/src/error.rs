//! Crate errors.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from inventory documents, posting, conversion, and projections.
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
    /// Ledger posting or projection.
    #[error(transparent)]
    Ledger(#[from] wicket_ledger::Error),
    /// Unit conversion.
    #[error(transparent)]
    Uom(#[from] wicket_uom::Error),
    /// Event publish or schema registration.
    #[error(transparent)]
    Events(#[from] wicket_events::Error),
    /// State-machine error.
    #[error(transparent)]
    Statemachine(#[from] wicket_statemachine::Error),
    /// Composition root.
    #[error(transparent)]
    Module(#[from] wicket_module::Error),
    /// Locations published interface.
    #[error(transparent)]
    Locations(#[from] wicket_mod_locations::Error),
    /// Lots published interface.
    #[error(transparent)]
    Lots(#[from] wicket_mod_lots::Error),
    /// JSON / serde failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Document was not found.
    #[error("document not found")]
    NotFound,
    /// Optimistic version did not match.
    #[error("version conflict")]
    VersionConflict,
    /// Document-level rule (over-receipt, count tolerance) — never a ledger invariant.
    #[error("document error: {0}")]
    Document(String),
    /// `reason_code` is required on an adjustment.
    #[error("reason_code is required")]
    ReasonRequired,
    /// Idempotency key replayed with a different body.
    #[error("idempotency conflict")]
    IdempotencyConflict,
    /// Idempotency key missing on a mutating POST.
    #[error("Idempotency-Key is required")]
    IdempotencyRequired,
    /// List `limit` outside 1..=200.
    #[error("limit must be between 1 and 200")]
    InvalidLimit,
    /// Manifest failed to parse.
    #[error("manifest: {0}")]
    Manifest(String),
    /// Unknown dimension for conversion.
    #[error("unknown dimension")]
    UnknownDimension,
    /// Operator named a lot or serial with no matching open layer.
    #[error("no eligible layer for the named lot or serial")]
    NoEligibleLayer,
    /// Lot is quarantined, on hold, or otherwise not issuable.
    #[error("lot is not available for issue")]
    LotNotIssuable,
}

impl From<wicket_core::PostingError> for Error {
    fn from(err: wicket_core::PostingError) -> Self {
        Error::Ledger(err.into())
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}
