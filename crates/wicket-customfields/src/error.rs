//! Crate errors.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Crate error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
    /// State-machine / executor error.
    #[error(transparent)]
    StateMachine(wicket_statemachine::Error),
    /// Unknown installation profile.
    #[error("unknown profile: {0}")]
    UnknownProfile(String),
    /// Unknown validation rule at define time.
    #[error("unknown validation rule: {rule}")]
    UnknownValidationRule {
        /// Rule name.
        rule: String,
    },
    /// Value failed a named validation rule at set time.
    #[error("validation failed for rule {rule}")]
    ValidationFailed {
        /// Rule name.
        rule: String,
    },
    /// Required field has no value.
    #[error("required custom field missing: {key}")]
    RequiredFieldMissing {
        /// Field key.
        key: String,
    },
    /// Value type does not match the definition.
    #[error("value type mismatch for field {key}")]
    TypeMismatch {
        /// Field key.
        key: String,
    },
    /// Definition type cannot change.
    #[error("definition type cannot change for {entity}.{key}")]
    TypeChangeRefused {
        /// Entity name.
        entity: String,
        /// Field key.
        key: String,
    },
    /// `audit = false` is refused for app-class entities.
    #[error("audit=false is refused for app-class custom fields")]
    AuditRefused,
    /// Caller may not retire this definition.
    #[error("retire refused for definition owned by {owner}")]
    RetireForbidden {
        /// Owning module id.
        owner: String,
    },
    /// Definition not found.
    #[error("custom field definition not found")]
    NotFound,
    /// Definition is retired.
    #[error("custom field definition is retired")]
    Retired,
    /// Second retire of a terminal instance (R-2s-5).
    #[error("custom field definition already retired")]
    AlreadyRetired,
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}

impl From<wicket_statemachine::Error> for Error {
    fn from(err: wicket_statemachine::Error) -> Self {
        match err {
            wicket_statemachine::Error::InvalidState { ref actual, .. }
                if actual == crate::domain::DefinitionStatus::Retired.as_str() =>
            {
                Error::AlreadyRetired
            }
            wicket_statemachine::Error::InstanceNotFound { .. } => Error::NotFound,
            other => Error::StateMachine(other),
        }
    }
}
