//! Composition root: wires PostingSink and SignatureGate.

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

/// Module identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ModuleId(pub datum_core::Identifier);

/// Enablement manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    /// Module id.
    pub id: ModuleId,
    /// Enabled flag.
    pub enabled: bool,
}

/// Embedded placeholder migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Migrators in dependency order.
pub fn migrators() -> Vec<&'static sqlx::migrate::Migrator> {
    vec![
        &datum_db::MIGRATOR,
        &datum_audit::MIGRATOR,
        &datum_identity::MIGRATOR,
        &datum_numbering::MIGRATOR,
        &datum_uom::MIGRATOR,
        &datum_events::MIGRATOR,
        &datum_jobs::MIGRATOR,
        &datum_ledger::MIGRATOR,
        &datum_statemachine::MIGRATOR,
        &datum_esign::MIGRATOR,
        &datum_customfields::MIGRATOR,
        &datum_documents::MIGRATOR,
        &datum_print::MIGRATOR,
        &MIGRATOR,
    ]
}

/// Wire the kernel. Unimplemented as a composition of live engines.
pub fn compose() -> Result<Manifest> {
    let _ = core::any::type_name::<datum_core::Error>();
    let _ = core::any::type_name::<datum_db::Error>();
    let _ = core::any::type_name::<datum_audit::Error>();
    let _ = core::any::type_name::<datum_identity::Error>();
    let _ = core::any::type_name::<datum_numbering::Error>();
    let _ = core::any::type_name::<datum_uom::Error>();
    let _ = core::any::type_name::<datum_events::Error>();
    let _ = core::any::type_name::<datum_jobs::Error>();
    let _ = core::any::type_name::<datum_ledger::Error>();
    let _ = core::any::type_name::<datum_statemachine::Error>();
    let _ = core::any::type_name::<datum_esign::Error>();
    let _ = core::any::type_name::<datum_customfields::Error>();
    let _ = core::any::type_name::<datum_documents::Error>();
    let _ = core::any::type_name::<datum_print::Error>();
    let _ = core::any::type_name::<dyn datum_core::PostingSink>();
    let _ = core::any::type_name::<dyn datum_core::SignatureGate>();
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
        assert!(!migrators().is_empty());
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
