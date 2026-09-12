//! Crate errors.

use datum_core::{ItemId, UnitId};

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
    /// No `uom.item_stock` row for the item.
    #[error("unknown item stock for {0}")]
    UnknownItemStock(ItemId),
    /// Stock scale must be 0..=8.
    #[error("stock_scale must be 0..=8")]
    InvalidStockScale,
    /// Residual tolerance must be non-negative.
    #[error("residual_tolerance must be >= 0")]
    InvalidResidualTolerance,
    /// D2 R5: stock unit, scale, and tolerance are immutable while postings exist.
    #[error("stock unit, scale, and residual tolerance are immutable while postings exist")]
    StockMeasureImmutable,
}

impl From<datum_core::QuantityError> for Error {
    fn from(e: datum_core::QuantityError) -> Self {
        Self::Core(datum_core::Error::Quantity(e))
    }
}
