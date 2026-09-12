//! Crate error type.

use datum_core::Identifier;

/// Crate error.
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
    /// The worker was given an actor that is not a service principal.
    #[error("worker requires a named service principal")]
    NotServicePrincipal,
    /// No handler is registered for this job kind.
    #[error("no handler registered for job kind {0}")]
    UnknownKind(String),
    /// Job row missing.
    #[error("job {0} not found")]
    NotFound(Identifier),
    /// Illegal state transition.
    #[error("invariant violated: {0}")]
    Invariant(String),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}
