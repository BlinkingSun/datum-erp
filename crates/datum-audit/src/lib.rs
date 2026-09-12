//! Kernel audit trail (stub API).
#![allow(clippy::disallowed_methods, clippy::disallowed_macros)] // D3 §11: audit owns trail SQL.

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

/// Context the persistence layer needs to write an audit row.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditCtx {
    /// Actor.
    pub actor: datum_core::Actor,
    /// Reason.
    pub reason: Option<String>,
    /// Source identifier.
    pub source: Option<datum_core::Identifier>,
}

impl AuditCtx {
    /// Test constructor.
    pub fn test(actor: datum_core::Actor) -> Self {
        Self {
            actor,
            reason: None,
            source: None,
        }
    }
}

/// Recorded change. Persistence is Wave 2.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    /// Entry id.
    pub id: datum_core::Identifier,
    /// Entity name.
    pub entity: String,
}

/// Write path other crates call.
pub async fn record(
    _tx: &mut datum_db::Tx<'_>,
    _ctx: &AuditCtx,
    _entry: AuditEntry,
) -> Result<datum_core::Identifier> {
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
