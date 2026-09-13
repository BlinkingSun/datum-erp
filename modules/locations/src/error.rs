//! Module errors.

/// Locations module error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented (stub surface).
    #[error("unimplemented")]
    Unimplemented,
    /// Core invariant.
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    /// Database layer.
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
    /// Ledger registry / balance.
    #[error(transparent)]
    Ledger(#[from] wicket_ledger::Error),
    /// Events outbox.
    #[error(transparent)]
    Events(#[from] wicket_events::Error),
    /// Composition root.
    #[error(transparent)]
    Module(#[from] wicket_module::Error),
    /// Validation / business rule.
    #[error("validation: {0}")]
    Validation(String),
    /// Missing row.
    #[error("not found: {0}")]
    NotFound(String),
    /// Optimistic concurrency.
    #[error("conflict: {0}")]
    Conflict(String),
    /// Deactivate refused while inventory remains.
    #[error("on hand at location")]
    OnHand,
    /// Boundary or system location cannot be deactivated.
    #[error("protected location")]
    Protected,
    /// Tree would cycle.
    #[error("cycle in location tree")]
    Cycle,
    /// Immutable field change.
    #[error("immutable: {0}")]
    Immutable(String),
}

/// Module result alias.
pub type Result<T> = core::result::Result<T, Error>;

impl From<sqlx::Error> for Error {
    fn from(value: sqlx::Error) -> Self {
        Self::Db(value.into())
    }
}
