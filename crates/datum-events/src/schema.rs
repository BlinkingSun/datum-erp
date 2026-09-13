//! Append-only payload contracts keyed by `(name, version)`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::Deserialize;
use serde_json::Value;

use crate::error::{Error, Result};

/// One field in a payload contract.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub struct Field {
    /// JSON object key.
    pub name: String,
    /// When true, [`Event::builder`](crate::Event::builder) refuses a payload that omits it.
    pub required: bool,
}

impl Field {
    /// Required payload field.
    pub fn required(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            required: true,
        }
    }

    /// Optional payload field. Adding one of these is the append-only evolution.
    pub fn optional(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            required: false,
        }
    }
}

/// Payload contract for one `(name, version)`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EventSchema {
    /// Event name (`inventory.lot_received`).
    pub name: String,
    /// Schema version. A new version is a new contract; fields never disappear inside one.
    pub version: i16,
    /// Fields of this version, in registration order.
    pub fields: Vec<Field>,
}

/// In-memory schema registry. Registration of the same `(name, version)` may only add fields.
#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    inner: BTreeMap<(String, i16), EventSchema>,
}

impl SchemaRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Kernel-known schemas: inventory v1 events, documents v1 events
    /// (`documents.revision_created`, `documents.effective`), and `test.ping.v1`.
    pub fn standard() -> Self {
        let mut reg = Self::new();
        for json in [
            include_str!("../fixtures/inventory.lot_received.v1.json"),
            include_str!("../fixtures/inventory.receipt_posted.v1.json"),
            include_str!("../fixtures/inventory.adjusted.v1.json"),
            include_str!("../fixtures/documents.revision_created.v1.json"),
            include_str!("../fixtures/documents.effective.v1.json"),
            include_str!("../fixtures/test.ping.v1.json"),
        ] {
            let _ = reg.register(schema_from_fixture(json));
        }
        reg
    }

    /// Frozen field names for `inventory.lot_received.v1`. The
    /// `schema_registry_rejects_removed_field` test fails if one of these is dropped.
    pub const INVENTORY_LOT_RECEIVED_V1_FIELDS: &'static [&'static str] =
        &["lot_id", "item_id", "qty"];

    /// Frozen field names for `inventory.receipt_posted.v1` (no `lot_id`).
    pub const INVENTORY_RECEIPT_POSTED_V1_FIELDS: &'static [&'static str] =
        &["item_id", "location_id", "qty", "uom", "posting_group_id"];

    /// Frozen field names for `inventory.adjusted.v1`.
    pub const INVENTORY_ADJUSTED_V1_FIELDS: &'static [&'static str] =
        &["item_id", "qty", "reason_code"];

    /// Frozen field names for `documents.revision_created.v1`.
    pub const DOCUMENTS_REVISION_CREATED_V1_FIELDS: &'static [&'static str] =
        &["document_id", "revision_id", "label"];

    /// Frozen field names for `documents.effective.v1`.
    pub const DOCUMENTS_EFFECTIVE_V1_FIELDS: &'static [&'static str] =
        &["document_id", "revision_id", "effective_from"];

    /// Register a contract. A second call for the same key must be a superset of the field names.
    pub fn register(&mut self, schema: EventSchema) -> Result<()> {
        let key = (schema.name.clone(), schema.version);
        if let Some(existing) = self.inner.get(&key) {
            let old: BTreeSet<&str> = existing.fields.iter().map(|f| f.name.as_str()).collect();
            let new: BTreeSet<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
            if let Some(field) = old.difference(&new).next() {
                return Err(Error::RemovedField {
                    name: schema.name,
                    version: schema.version,
                    field: (*field).to_string(),
                });
            }
        }
        self.inner.insert(key, schema);
        Ok(())
    }

    /// Look up a contract.
    pub fn get(&self, name: &str, version: i16) -> Option<&EventSchema> {
        self.inner.get(&(name.to_string(), version))
    }

    /// Every registered contract.
    pub fn iter(&self) -> impl Iterator<Item = &EventSchema> {
        self.inner.values()
    }

    /// Refuse a payload that is not an object, omits a required field, or
    /// includes a field the contract does not declare.
    pub fn validate(&self, name: &str, version: i16, payload: &Value) -> Result<()> {
        let schema = self
            .get(name, version)
            .ok_or_else(|| Error::UnknownSchema {
                name: name.to_string(),
                version,
            })?;
        let obj = payload.as_object().ok_or_else(|| Error::PayloadNotObject {
            name: name.to_string(),
            version,
        })?;
        for field in &schema.fields {
            if field.required && !obj.contains_key(&field.name) {
                return Err(Error::MissingField {
                    name: name.to_string(),
                    version,
                    field: field.name.clone(),
                });
            }
        }
        let allowed: BTreeSet<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
        for key in obj.keys() {
            if !allowed.contains(key.as_str()) {
                return Err(Error::UnexpectedField {
                    name: name.to_string(),
                    version,
                    field: key.clone(),
                });
            }
        }
        Ok(())
    }
}

fn schema_from_fixture(json: &str) -> EventSchema {
    match serde_json::from_str(json) {
        Ok(schema) => schema,
        Err(err) => panic!("standard event fixture is invalid: {err}"),
    }
}

static GLOBAL: LazyLock<RwLock<SchemaRegistry>> =
    LazyLock::new(|| RwLock::new(SchemaRegistry::standard()));

fn map_poison<T>(err: std::sync::PoisonError<T>) -> Error {
    let _ = err;
    Error::Invariant("schema registry lock poisoned".into())
}

/// Process-global registry used by [`crate::publish`] and [`Event::builder`](crate::Event::builder).
pub fn global() -> Result<RwLockReadGuard<'static, SchemaRegistry>> {
    GLOBAL.read().map_err(map_poison)
}

/// Mutable view of the process-global registry (composition root).
pub fn global_mut() -> Result<RwLockWriteGuard<'static, SchemaRegistry>> {
    GLOBAL.write().map_err(map_poison)
}

/// Register `schema` on the process-global registry.
pub fn register(schema: EventSchema) -> Result<()> {
    global_mut()?.register(schema)
}
