//! Persistence errors.

use std::borrow::Cow;
use std::fmt;

/// SQLSTATE code returned by PostgreSQL.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SqlState(Cow<'static, str>);

impl SqlState {
    /// Privilege / context refusal (`42501`).
    pub const REFUSED: Self = Self(Cow::Borrowed("42501"));
    /// Serialization failure (`40001`).
    pub const SERIALIZATION: Self = Self(Cow::Borrowed("40001"));

    /// Wrap a static SQLSTATE.
    pub const fn from_static(code: &'static str) -> Self {
        Self(Cow::Borrowed(code))
    }

    /// The five-character SQLSTATE.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SqlState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persistence error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented (DDL attach, missing `audit.log_event`).
    #[error("unimplemented")]
    Unimplemented,
    /// Write refused: missing or stale transaction-local context (`42501`).
    #[error("refused ({0})")]
    Refused(SqlState),
    /// Serialization failure (`40001`); retry with [`crate::retry_serializable`].
    #[error("serialization failure")]
    Serialization,
    /// Catalogue lint failed; each string names a violating object.
    #[error("ddl check failed:\n{}", .0.join("\n"))]
    Ddl(Vec<String>),
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// SQLx error.
    #[error(transparent)]
    Sqlx(sqlx::Error),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        map_sqlx(err)
    }
}

impl From<sqlx::migrate::MigrateError> for Error {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Error::Sqlx(sqlx::Error::Migrate(Box::new(err)))
    }
}

/// Map a SQLx error, lifting `42501` and `40001`.
pub(crate) fn map_sqlx(err: sqlx::Error) -> Error {
    if let Some(code) = sqlstate_of(&err) {
        match code.as_ref() {
            "42501" => return Error::Refused(SqlState::REFUSED),
            "40001" => return Error::Serialization,
            _ => {}
        }
    }
    Error::Sqlx(err)
}

pub(crate) fn sqlstate_of(err: &sqlx::Error) -> Option<Cow<'_, str>> {
    err.as_database_error().and_then(|d| d.code())
}

pub(crate) fn is_undefined_function(err: &sqlx::Error) -> bool {
    matches!(sqlstate_of(err).as_deref(), Some("42883"))
}

pub(crate) fn is_undefined_table(err: &sqlx::Error) -> bool {
    matches!(sqlstate_of(err).as_deref(), Some("42P01" | "3F000"))
}
