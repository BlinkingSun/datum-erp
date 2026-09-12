//! Typed event and builder.

use chrono::{DateTime, Utc};
use datum_core::Identifier;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::schema::SchemaRegistry;

/// Event kind. Kept from the Wave 1 surface; the outbox keys on [`Event::name`] plus version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventKind {
    /// Record created.
    Created,
    /// Record updated.
    Updated,
    /// Named domain event (`inventory.lot_received`).
    Custom(String),
}

/// Domain event written to `app.event`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event id (uuid v7).
    pub id: Identifier,
    /// Kind. Informational; storage uses [`Self::name`].
    pub kind: EventKind,
    /// Event name (`inventory.lot_received`).
    pub name: String,
    /// Payload contract version.
    pub version: i16,
    /// Payload. Must match the registered schema for `(name, version)`.
    pub payload: Value,
    /// Server transaction time, stamped on insert. Client values are ignored by [`crate::publish`].
    pub occurred_at: DateTime<Utc>,
    /// Actor stamped from the publishing transaction.
    pub actor_id: Identifier,
    /// Source kind stamped from the publishing transaction.
    pub source_kind: String,
    /// Optional document type.
    pub doc_type: Option<String>,
    /// Optional document id.
    pub doc_id: Option<Identifier>,
}

impl Event {
    /// Start a builder. [`EventBuilder::build`] refuses payloads that do not match the registry
    /// (unknown schema, missing required field, or undeclared field).
    pub fn builder() -> EventBuilder {
        EventBuilder::default()
    }
}

/// Builder that validates the payload against a [`SchemaRegistry`].
#[derive(Debug, Clone, Default)]
pub struct EventBuilder {
    name: Option<String>,
    version: i16,
    payload: Option<Value>,
    kind: Option<EventKind>,
    doc_type: Option<String>,
    doc_id: Option<Identifier>,
}

impl EventBuilder {
    /// Event name (`inventory.lot_received`).
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Schema version. Defaults to 1.
    pub fn version(mut self, version: i16) -> Self {
        self.version = version;
        self
    }

    /// JSON payload.
    pub fn payload(mut self, payload: Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Override [`EventKind`]. Defaults to [`EventKind::Custom`] of the name.
    pub fn kind(mut self, kind: EventKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Optional document provenance stored on the outbox row.
    pub fn document(mut self, doc_type: impl Into<String>, doc_id: Identifier) -> Self {
        self.doc_type = Some(doc_type.into());
        self.doc_id = Some(doc_id);
        self
    }

    /// Validate against `registry` and mint an id. `occurred_at` / `actor_id` / `source_kind`
    /// are placeholders; [`crate::publish`] overwrites them from the transaction.
    pub fn build_with(self, registry: &SchemaRegistry) -> Result<Event> {
        let name = self
            .name
            .ok_or_else(|| Error::Invariant("event name is required".into()))?;
        let version = if self.version == 0 { 1 } else { self.version };
        let payload = self.payload.unwrap_or(Value::Object(Default::default()));
        registry.validate(&name, version, &payload)?;
        let kind = self.kind.unwrap_or_else(|| EventKind::Custom(name.clone()));
        Ok(Event {
            id: Identifier::generate(),
            kind,
            name,
            version,
            payload,
            occurred_at: DateTime::UNIX_EPOCH,
            actor_id: Identifier::from_uuid(uuid::Uuid::nil()),
            source_kind: String::new(),
            doc_type: self.doc_type,
            doc_id: self.doc_id,
        })
    }

    /// Validate against the process-global registry.
    pub fn build(self) -> Result<Event> {
        let registry = crate::schema::global()?;
        self.build_with(&registry)
    }
}
