//! Identity: principals, credentials, sessions, and RBAC.
//!
//! Writes go through [`datum_db::Tx`]. SQL uses `sqlx::query` / `query_as` /
//! `query_scalar` (CONTRACT §5a as amended). Session-protocol helpers stay
//! confined to `datum-db` / `datum-audit` / `datum-test`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use datum_audit as _;

#[cfg(test)]
use serde_json as _;

mod argon2id;
mod blake2b;
mod credential;
mod principal;
pub mod rbac;
mod session;

pub use argon2id::{
    M_KIB as ARGON2ID_M_KIB, P_COST as ARGON2ID_P, T_COST as ARGON2ID_T, hash_password,
    hash_password_with_params, verify_password,
};
pub use credential::{
    CredentialKind, complete_reset, request_reset, set_login_credential, set_signing_credential,
    verify_login_secret, verify_signing,
};
pub use principal::{
    MIGRATION_ID, Principal, PrincipalKind, PrincipalStatus, SYSTEM_ID, create_principal,
    deactivate_principal, load_principal, load_principal_on, rename_principal, seed_builtins,
};
pub use session::{PasswordProvider, Provider, Session, login, reauth_signing};

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
    /// Username was used before and may not be recycled (invariant 13).
    #[error("username is never reused")]
    UsernameReused,
    /// Principal is inactive.
    #[error("principal is inactive")]
    Inactive,
    /// Principal was not found.
    #[error("principal not found")]
    NotFound,
    /// Login or signing secret did not match.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Account is locked after too many failures.
    #[error("locked out until {until}")]
    Lockout {
        /// When the lockout expires.
        until: chrono::DateTime<chrono::Utc>,
        /// Recorded failure count.
        failures: i32,
    },
    /// A credential reset must be requested by one principal and completed by another (D3 §9).
    #[error("credential reset requires two principals")]
    ResetRequiresTwoPrincipals,
    /// Reset token missing, spent, or expired.
    #[error("credential reset token is invalid")]
    ResetInvalid,
    /// Argon2id failure.
    #[error("crypto: {0}")]
    Crypto(String),
    /// Role-bundle TOML could not be parsed.
    #[error("role bundle: {0}")]
    Bundle(String),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Unique index / RAISE CONSTRAINT name for `identity.username_history(lower(username))`.
pub(crate) const USERNAME_HISTORY_UNIQUE: &str = "username_history_username_lower";

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        if let Some(db) = err.as_database_error()
            && db.code().as_deref() == Some("23505")
            && db.constraint() == Some(USERNAME_HISTORY_UNIQUE)
        {
            return Error::UsernameReused;
        }
        Error::Db(err.into())
    }
}

impl From<argon2id::Error> for Error {
    fn from(err: argon2id::Error) -> Self {
        Error::Crypto(err.to_string())
    }
}

/// Embedded migrator (`placeholder` + `0001_identity`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Identity-local user id. Not the core identifier family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UserId(pub datum_core::Identifier);

impl UserId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(datum_core::Identifier::generate())
    }

    /// Wrap an existing identifier.
    pub fn from_identifier(id: datum_core::Identifier) -> Self {
        Self(id)
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0.as_uuid()
    }
}

/// Role id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RoleId(pub datum_core::Identifier);

impl RoleId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(datum_core::Identifier::generate())
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0.as_uuid()
    }
}

/// Failed-login lockout after this many consecutive failures.
pub const LOCKOUT_AFTER: i32 = 5;
/// Lockout duration.
pub const LOCKOUT_SECS: i64 = 15 * 60;

pub(crate) fn map_tx(err: datum_db::Error) -> Error {
    match err {
        datum_db::Error::Sqlx(e) => Error::from(e),
        other => Error::Db(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json as _;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(MIGRATOR.migrations.len() >= 2);
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn argon2id_parameters_are_pinned_and_tested() {
        assert_eq!(ARGON2ID_M_KIB, 19_456);
        assert_eq!(ARGON2ID_T, 2);
        assert_eq!(ARGON2ID_P, 1);
        let salt = b"datum-identity16";
        let phc =
            hash_password_with_params(b"password", salt, ARGON2ID_M_KIB, ARGON2ID_T, ARGON2ID_P)
                .expect("hash");
        assert!(phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password(b"password", &phc).expect("verify"));
        assert!(!verify_password(b"wrong", &phc).expect("verify wrong"));
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
