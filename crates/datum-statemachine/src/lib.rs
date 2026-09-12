//! State machines and transitions (stub API).

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

/// Machine id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MachineId(pub datum_core::Identifier);

/// State name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct State(pub String);

/// Transition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Transition {
    /// Machine.
    pub machine: MachineId,
    /// From state.
    pub from: State,
    /// To state.
    pub to: State,
}

/// Apply a transition. Uses core traits, never ledger or esign.
pub async fn apply(
    _tx: &mut datum_db::Tx<'_>,
    _sink: &mut dyn datum_core::PostingSink,
    _gate: &dyn datum_core::SignatureGate,
    _transition: Transition,
) -> Result<State> {
    let _ = core::any::type_name::<datum_audit::Error>();
    let _ = core::any::type_name::<datum_identity::Error>();
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
