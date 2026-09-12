//! Crate errors.

use datum_core::UnitId;

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Crate error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Unknown unit in the catalog.
    #[error("unknown unit {0:?}")]
    UnknownUnit(UnitId),
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] datum_db::Error),
}

impl From<datum_core::QuantityError> for Error {
    fn from(e: datum_core::QuantityError) -> Self {
        Self::Core(datum_core::Error::Quantity(e))
    }
}
