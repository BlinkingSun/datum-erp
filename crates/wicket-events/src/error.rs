//! Crate error type.

use wicket_core::Identifier;

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
    /// The dispatcher was given an actor that is not a service principal.
    #[error("dispatcher requires a named service principal")]
    NotServicePrincipal,
    /// No payload schema is registered for this `(name, version)`.
    #[error("no schema registered for {name}.v{version}")]
    UnknownSchema {
        /// Event name (`inventory.lot_received`).
        name: String,
        /// Schema version.
        version: i16,
    },
    /// A later registration of the same `(name, version)` dropped a field.
    #[error("schema {name}.v{version} removed field {field}")]
    RemovedField {
        /// Event name.
        name: String,
        /// Schema version.
        version: i16,
        /// Field that disappeared.
        field: String,
    },
    /// Payload is not a JSON object.
    #[error("payload for {name}.v{version} is not a JSON object")]
    PayloadNotObject {
        /// Event name.
        name: String,
        /// Schema version.
        version: i16,
    },
    /// Payload is missing a field the schema requires.
    #[error("payload for {name}.v{version} missing field {field}")]
    MissingField {
        /// Event name.
        name: String,
        /// Schema version.
        version: i16,
        /// Missing field.
        field: String,
    },
    /// Payload includes a field the schema does not declare.
    #[error("payload for {name}.v{version} has unexpected field {field}")]
    UnexpectedField {
        /// Event name.
        name: String,
        /// Schema version.
        version: i16,
        /// Extra field.
        field: String,
    },
    /// A subscriber handler returned a failure.
    #[error("handler {subscriber} failed on {event_id}: {message}")]
    Handler {
        /// Subscriber id.
        subscriber: String,
        /// Event id being delivered.
        event_id: Identifier,
        /// Handler error text.
        message: String,
    },
    /// An invariant the caller violated.
    #[error("invariant violated: {0}")]
    Invariant(String),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}
