//! Kernel units of measure (stub API; Wave 2 fills the engine).

use serde::{Deserialize, Serialize};

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
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Marker so serde stays linked in the stub.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UomStub;

/// Embedded placeholder migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Convert a quantity to another unit. Always unimplemented in the stub.
pub fn convert(
    _qty: datum_core::AnyQuantity,
    _to: datum_core::UnitId,
) -> Result<datum_core::AnyQuantity> {
    let _ = rust_decimal::Decimal::ZERO;
    let _ = core::any::type_name::<datum_audit::Error>();
    Err(Error::Unimplemented)
}

/// Load a unit from the catalog.
pub async fn load_unit(
    _pool: &datum_db::Pool,
    _id: datum_core::UnitId,
) -> Result<datum_core::UnitId> {
    Err(Error::Unimplemented)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(!MIGRATOR.migrations.is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
