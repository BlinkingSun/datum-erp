//! Electronic signatures (stub API; implements SignatureGate in Wave 2b).

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

/// Embedded placeholder migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Signature id local to this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SignatureId(pub datum_core::Identifier);

/// Signature meaning local to this crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SignatureMeaning(pub String);

/// Verify a token through this crate's gate. Unimplemented.
pub fn verify(
    _token: &datum_core::SignatureToken,
    _required: &datum_core::SignatureRequirement,
    _record: &datum_core::RecordRef,
) -> Result<()> {
    let _ = core::any::type_name::<datum_audit::Error>();
    let _ = core::any::type_name::<datum_identity::Error>();
    let _ = core::any::type_name::<datum_db::Tx<'static>>();
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
