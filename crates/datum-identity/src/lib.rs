//! Identity and principals (stub API).

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

/// Identity-local user id. Not the core identifier family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UserId(pub datum_core::Identifier);

/// Role id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RoleId(pub datum_core::Identifier);

/// Authenticated principal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Principal {
    /// Principal id.
    pub id: UserId,
    /// Actor kind.
    pub kind: datum_core::ActorKind,
}

/// Look up a principal.
pub async fn load_principal(_pool: &datum_db::Pool, _id: UserId) -> Result<Principal> {
    let _ = core::any::type_name::<datum_audit::Error>();
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
