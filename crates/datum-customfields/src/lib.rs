//! Typed, validated, audited custom fields on any app-class entity.
//!
//! Values are stored per type in schema `customfields` — never as a JSON blob.
//! Writes go through [`datum_db::Tx`] only (CONTRACT §5a).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use datum_audit as _;
use datum_core::Identifier;
use datum_db::Tx;

mod domain;
mod error;
mod manifest;
mod store;
mod validate;

pub use domain::{
    DatePrecision, Definition, DefinitionId, DefinitionSpec, DefinitionStatus, FieldKey, FieldType,
    Value, ValueWire,
};
pub use error::{Error, Result};
pub use manifest::{ManifestCustomField, ManifestCustomFields};
pub use validate::{gtin_valid, validate_value as validate};

/// Embedded migrator (`placeholder` + `0001_customfields`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Define a new custom field or bump version on configuration change.
pub async fn define(tx: &mut Tx<'_>, spec: DefinitionSpec) -> Result<DefinitionId> {
    if let Some(existing) = store::load_active_by_entity_key(tx, &spec.entity, &spec.key).await? {
        if existing.field_type != spec.field_type {
            return Err(Error::TypeChangeRefused {
                entity: spec.entity.clone(),
                key: spec.key.clone(),
            });
        }
        let changed = existing.label != spec.label
            || existing.validation_rule != spec.validation_rule
            || existing.required != spec.required
            || existing.indexed != spec.indexed
            || existing.owner_module != spec.owner_module;
        if !changed {
            return Ok(existing.id);
        }
        let new_ver = existing.version + 1;
        store::close_previous_version(tx, existing.id, existing.version).await?;
        store::insert_definition_version(tx, existing.id, new_ver, &spec, DefinitionStatus::Active)
            .await?;
        return Ok(existing.id);
    }
    let id = DefinitionId::generate();
    store::insert_definition_version(tx, id, 1, &spec, DefinitionStatus::Active).await?;
    Ok(id)
}

/// Retire a definition (`owner` module or `customfields.define` action only).
pub async fn retire(tx: &mut Tx<'_>, id: DefinitionId) -> Result<()> {
    let row = tx
        .fetch_optional(
            sqlx::query_as::<_, (i32, String)>(
                "SELECT version, owner_module FROM customfields.definition
                 WHERE definition_id = $1 AND status = 'active' AND effective_to IS NULL
                 ORDER BY version DESC LIMIT 1",
            )
            .bind(id.as_uuid()),
        )
        .await?;
    let Some((version, owner)) = row else {
        return Err(Error::NotFound);
    };
    let action = tx.setting("datum.action").await?;
    if !store::caller_may_retire(&action, &owner) {
        return Err(Error::RetireForbidden { owner });
    }
    store::mark_retired(tx, id, version).await?;
    Ok(())
}

/// Set a typed value on a record.
pub async fn set(
    tx: &mut Tx<'_>,
    entity: &str,
    record_id: Identifier,
    key: &str,
    value: Value,
) -> Result<()> {
    let def = store::load_active_by_entity_key(tx, entity, key)
        .await?
        .ok_or(Error::NotFound)?;
    store::upsert_value(tx, &def, record_id, &value).await?;
    Ok(())
}

/// Read one value (includes values for retired definitions).
pub async fn get(
    tx: &mut Tx<'_>,
    entity: &str,
    record_id: Identifier,
    key: &str,
) -> Result<Option<Value>> {
    let def = store::load_latest_by_entity_key(tx, entity, key).await?;
    match def {
        None => Ok(None),
        Some(d) => store::load_value(tx, &d, record_id).await,
    }
}

/// All active definitions on an entity with values when present.
pub async fn list_for_record(
    tx: &mut Tx<'_>,
    entity: &str,
    record_id: Identifier,
) -> Result<Vec<(Definition, Value)>> {
    let defs = store::list_active_for_entity(tx, entity).await?;
    let mut out = Vec::new();
    for def in defs {
        if let Some(v) = store::load_value(tx, &def, record_id).await? {
            out.push((def, v));
        }
    }
    Ok(out)
}

/// Active definitions for an entity.
pub async fn definitions_for(tx: &mut Tx<'_>, entity: &str) -> Result<Vec<Definition>> {
    store::list_active_for_entity(tx, entity).await
}

/// Idempotent manifest registration.
pub async fn register_from_manifest(
    tx: &mut Tx<'_>,
    manifest: &ManifestCustomFields,
) -> Result<()> {
    for field in &manifest.fields {
        field.validate()?;
        let spec = field.to_spec();
        if let Some(existing) =
            store::load_active_by_entity_key(tx, &spec.entity, &spec.key).await?
        {
            if existing.field_type != spec.field_type {
                return Err(Error::TypeChangeRefused {
                    entity: spec.entity.clone(),
                    key: spec.key.clone(),
                });
            }
            if existing.label == spec.label
                && existing.validation_rule == spec.validation_rule
                && existing.required == spec.required
                && existing.indexed == spec.indexed
                && existing.owner_module == spec.owner_module
            {
                continue;
            }
        }
        define(tx, spec).await?;
    }
    Ok(())
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
    fn migrator_has_migrations() {
        assert!(MIGRATOR.migrations.len() >= 2);
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
