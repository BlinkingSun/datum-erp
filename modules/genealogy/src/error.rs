//! Crate errors.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from genealogy traces, cache, and jobs.
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
    /// Ledger trace API.
    #[error(transparent)]
    Ledger(#[from] wicket_ledger::Error),
    /// Event subscribe / publish.
    #[error(transparent)]
    Events(#[from] wicket_events::Error),
    /// Background jobs.
    #[error(transparent)]
    Jobs(#[from] wicket_jobs::Error),
    /// Composition root.
    #[error(transparent)]
    Module(#[from] wicket_module::Error),
    /// Inventory published interface.
    #[error(transparent)]
    Inventory(#[from] wicket_mod_inventory::Error),
    /// Items published interface.
    #[error(transparent)]
    Items(#[from] wicket_mod_items::Error),
    /// Locations published interface.
    #[error(transparent)]
    Locations(#[from] wicket_mod_locations::Error),
    /// Lots published interface.
    #[error(transparent)]
    Lots(#[from] wicket_mod_lots::Error),
    /// JSON / serde failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Trace origin was missing or malformed.
    #[error("trace origin is required (lot, serial, or posting)")]
    OriginRequired,
    /// `direction` was not backward|forward|both.
    #[error("invalid direction")]
    InvalidDirection,
    /// `format` was not csv|json.
    #[error("invalid format")]
    InvalidFormat,
    /// Job or lot was not found.
    #[error("not found")]
    NotFound,
    /// Manifest failed to parse.
    #[error("manifest: {0}")]
    Manifest(String),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}
