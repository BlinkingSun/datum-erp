//! Domain types for custom field definitions and values.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use datum_core::Identifier;

/// Stable definition identity (all versions share this id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DefinitionId(Identifier);

impl DefinitionId {
    /// Wrap an identifier.
    pub fn new(id: Identifier) -> Self {
        Self(id)
    }

    /// Generate a new id.
    pub fn generate() -> Self {
        Self(Identifier::generate())
    }

    /// Underlying identifier.
    pub fn as_identifier(self) -> Identifier {
        self.0
    }

    /// Underlying uuid.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0.as_uuid()
    }
}

/// Field key (manifest / API).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldKey(pub String);

/// Stored field type (no JSON blob).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldType {
    /// Short string.
    String,
    /// Long text.
    Text,
    /// Integer.
    Integer,
    /// Decimal with scale.
    Decimal,
    /// Boolean.
    Bool,
    /// Calendar date with precision (inv 12).
    Date,
    /// Enumerated string.
    Enum,
    /// Reference to another entity record.
    Reference,
}

impl FieldType {
    /// Parse manifest / SQL type name.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "string" => Some(Self::String),
            "text" => Some(Self::Text),
            "integer" => Some(Self::Integer),
            "decimal" => Some(Self::Decimal),
            "bool" => Some(Self::Bool),
            "date" => Some(Self::Date),
            "enum" => Some(Self::Enum),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }

    /// SQL / manifest name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Text => "text",
            Self::Integer => "integer",
            Self::Decimal => "decimal",
            Self::Bool => "bool",
            Self::Date => "date",
            Self::Enum => "enum",
            Self::Reference => "reference",
        }
    }
}

/// Definition lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DefinitionStatus {
    /// Active definitions accept writes.
    Active,
    /// Retired definitions are read-only.
    Retired,
}

impl DefinitionStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "retired" => Some(Self::Retired),
            _ => None,
        }
    }
}

/// Date precision (inv 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DatePrecision {
    /// Day precision.
    Day,
    /// Month precision (stored as first of month).
    Month,
    /// Year precision (stored as 1 January).
    Year,
}

impl DatePrecision {
    /// Wire / SQL name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "day" => Some(Self::Day),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }
}

/// Effectivity-versioned field definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    /// Stable id.
    pub id: DefinitionId,
    /// Version (increments on configuration change).
    pub version: i32,
    /// Entity name (`items.item`, …).
    pub entity: String,
    /// Field key within the entity.
    pub key: String,
    /// Stored type.
    pub field_type: FieldType,
    /// Human label.
    pub label: String,
    /// Validation rule string (`gs1-gtin`, `regex:…`, …).
    pub validation_rule: String,
    /// Required on the record.
    pub required: bool,
    /// Indexed hint (per-type btree exists on PK).
    pub indexed: bool,
    /// Owning module id (`mod-udi`, …).
    pub owner_module: String,
    /// Active or retired.
    pub status: DefinitionStatus,
}

/// Input for [`crate::define`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionSpec {
    /// Entity name.
    pub entity: String,
    /// Field key.
    pub key: String,
    /// Stored type.
    pub field_type: FieldType,
    /// Human label.
    pub label: String,
    /// Validation rule.
    pub validation_rule: String,
    /// Required flag.
    pub required: bool,
    /// Indexed flag.
    pub indexed: bool,
    /// Owning module.
    pub owner_module: String,
}

/// Typed value (never a JSON blob).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Value {
    /// Short string.
    String(String),
    /// Long text.
    Text(String),
    /// Integer.
    Integer(i64),
    /// Decimal with scale.
    Decimal {
        /// Numeric value.
        value: Decimal,
        /// Scale.
        scale: i16,
    },
    /// Boolean.
    Bool(bool),
    /// Date with precision.
    Date {
        /// Stored date.
        date: NaiveDate,
        /// Precision discriminator.
        precision: DatePrecision,
    },
    /// Enum member.
    Enum(String),
    /// Reference to another record.
    Reference {
        /// Target entity.
        entity: String,
        /// Target record id.
        id: Identifier,
    },
}

impl Value {
    /// Whether the value is empty for required-field checks.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::String(s) | Self::Text(s) | Self::Enum(s) => s.is_empty(),
            Self::Reference { entity, .. } => entity.is_empty(),
            _ => false,
        }
    }

    /// Expected type for this value.
    pub fn field_type(&self) -> FieldType {
        match self {
            Self::String(_) => FieldType::String,
            Self::Text(_) => FieldType::Text,
            Self::Integer(_) => FieldType::Integer,
            Self::Decimal { .. } => FieldType::Decimal,
            Self::Bool(_) => FieldType::Bool,
            Self::Date { .. } => FieldType::Date,
            Self::Enum(_) => FieldType::Enum,
            Self::Reference { .. } => FieldType::Reference,
        }
    }
}

/// Wire shape for a stored value (`docs/10`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueWire {
    /// Field key.
    pub key: String,
    /// Type name.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Typed value payload.
    pub value: serde_json::Value,
    /// Definition version at write time.
    pub definition_version: i32,
}
