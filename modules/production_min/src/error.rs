//! Crate errors.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from work-order persistence, posting, conversion, and transitions.
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
    /// Ledger posting or projection.
    #[error(transparent)]
    Ledger(#[from] datum_ledger::Error),
    /// Unit conversion.
    #[error(transparent)]
    Uom(#[from] datum_uom::Error),
    /// Event publish or schema registration.
    #[error(transparent)]
    Events(#[from] datum_events::Error),
    /// State-machine error.
    #[error(transparent)]
    Statemachine(#[from] datum_statemachine::Error),
    /// Composition root.
    #[error(transparent)]
    Module(#[from] datum_module::Error),
    /// Numbering.
    #[error(transparent)]
    Numbering(#[from] datum_numbering::Error),
    /// Items published interface.
    #[error(transparent)]
    Items(#[from] datum_mod_items::Error),
    /// Locations published interface.
    #[error(transparent)]
    Locations(#[from] datum_mod_locations::Error),
    /// Lots published interface.
    #[error(transparent)]
    Lots(#[from] datum_mod_lots::Error),
    /// Inventory published interface.
    #[error(transparent)]
    Inventory(#[from] datum_mod_inventory::Error),
    /// JSON / serde failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Work order was not found.
    #[error("work order not found")]
    NotFound,
    /// Optimistic version did not match.
    #[error("version conflict")]
    VersionConflict,
    /// Status cannot move along this edge.
    #[error("cannot {edge} work order in status {status}")]
    InvalidTransition {
        /// Requested edge.
        edge: String,
        /// Live status.
        status: String,
    },
    /// Ordered quantity must be strictly positive.
    #[error("quantity_ordered must be greater than zero")]
    InvalidQuantity,
    /// List `limit` outside 1..=200.
    #[error("limit must be between 1 and 200")]
    InvalidLimit,
    /// Manifest failed to parse.
    #[error("manifest: {0}")]
    Manifest(String),
    /// Unknown dimension for conversion.
    #[error("unknown dimension")]
    UnknownDimension,
    /// Conversion residual at complete is refused in the minimal surface.
    #[error("uom conversion residual is not posted by production-min; enter stock units")]
    ConversionResidual,
}

impl From<datum_core::PostingError> for Error {
    fn from(err: datum_core::PostingError) -> Self {
        Error::Ledger(err.into())
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}
