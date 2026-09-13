//! Module manifest `[[custom-fields]]` entries.

use crate::domain::{DefinitionSpec, FieldType};
use crate::error::{Error, Result};
use crate::validate::parse_rule_at_define;

/// One `[[custom-fields]]` row from a module manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestCustomField {
    /// Target entity (`items.item`).
    pub entity: String,
    /// Field key.
    pub key: String,
    /// Type name.
    pub field_type: FieldType,
    /// Label.
    pub label: String,
    /// Validation rule.
    pub validate: String,
    /// Audited (must be true for app-class entities).
    pub audit: bool,
    /// Required on record.
    pub required: bool,
    /// Indexed hint.
    pub indexed: bool,
    /// Owning module id.
    pub owner: String,
}

/// Batch of manifest custom fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestCustomFields {
    /// Declared fields.
    pub fields: Vec<ManifestCustomField>,
}

impl ManifestCustomField {
    /// Convert to a definition spec for define/register.
    pub fn to_spec(&self) -> DefinitionSpec {
        DefinitionSpec {
            entity: self.entity.clone(),
            key: self.key.clone(),
            field_type: self.field_type,
            label: self.label.clone(),
            validation_rule: self.validate.clone(),
            required: self.required,
            indexed: self.indexed,
            owner_module: self.owner.clone(),
        }
    }

    /// Manifest validation.
    pub fn validate(&self) -> Result<()> {
        if !self.audit {
            return Err(Error::AuditRefused);
        }
        parse_rule_at_define(&self.validate, self.field_type)?;
        Ok(())
    }
}
