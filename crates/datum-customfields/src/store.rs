//! SQL store (schema `customfields` only).

use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use datum_core::Identifier;
use datum_db::Tx;
use datum_statemachine::current_state;

use crate::domain::{
    DatePrecision, Definition, DefinitionId, DefinitionSpec, DefinitionStatus, FieldType, Value,
};
use crate::error::{Error, Result};
use crate::machine::doc_ref;
use crate::validate::{parse_rule_at_define, validate_value};

#[derive(sqlx::FromRow)]
struct DefRow {
    definition_id: Uuid,
    version: i32,
    entity: String,
    key: String,
    field_type: String,
    label: String,
    validation_rule: String,
    required: bool,
    indexed: bool,
    owner_module: String,
    status: String,
}

fn row_to_def(r: DefRow) -> Result<Definition> {
    let field_type = FieldType::parse(&r.field_type)
        .ok_or_else(|| Error::Core(datum_core::Error::Invariant("bad field_type in db".into())))?;
    let status = DefinitionStatus::parse(&r.status)
        .ok_or_else(|| Error::Core(datum_core::Error::Invariant("bad status in db".into())))?;
    Ok(Definition {
        id: DefinitionId::new(Identifier::from_uuid(r.definition_id)),
        version: r.version,
        entity: r.entity,
        key: r.key,
        field_type,
        label: r.label,
        validation_rule: r.validation_rule,
        required: r.required,
        indexed: r.indexed,
        owner_module: r.owner_module,
        status,
    })
}

pub(crate) async fn load_latest_by_entity_key(
    tx: &mut Tx<'_>,
    entity: &str,
    key: &str,
) -> Result<Option<Definition>> {
    let row = tx
        .fetch_optional(
            sqlx::query_as::<_, DefRow>(
                "SELECT definition_id, version, entity, key, field_type, label, validation_rule,
                        required, indexed, owner_module, status
                 FROM customfields.definition
                 WHERE entity = $1 AND key = $2
                 ORDER BY version DESC
                 LIMIT 1",
            )
            .bind(entity)
            .bind(key),
        )
        .await?;
    let mut def = row.map(row_to_def).transpose()?;
    if let Some(d) = def.as_mut() {
        overlay_live_status(tx, d).await?;
    }
    Ok(def)
}

pub(crate) async fn load_latest_by_id(
    tx: &mut Tx<'_>,
    id: DefinitionId,
) -> Result<Option<Definition>> {
    let row = tx
        .fetch_optional(
            sqlx::query_as::<_, DefRow>(
                "SELECT definition_id, version, entity, key, field_type, label, validation_rule,
                        required, indexed, owner_module, status
                 FROM customfields.definition
                 WHERE definition_id = $1
                 ORDER BY version DESC
                 LIMIT 1",
            )
            .bind(id.as_uuid()),
        )
        .await?;
    let mut def = row.map(row_to_def).transpose()?;
    if let Some(d) = def.as_mut() {
        overlay_live_status(tx, d).await?;
    }
    Ok(def)
}

pub(crate) async fn load_active_by_entity_key(
    tx: &mut Tx<'_>,
    entity: &str,
    key: &str,
) -> Result<Option<Definition>> {
    let row = tx
        .fetch_optional(
            sqlx::query_as::<_, DefRow>(
                "SELECT definition_id, version, entity, key, field_type, label, validation_rule,
                        required, indexed, owner_module, status
                 FROM customfields.definition
                 WHERE entity = $1 AND key = $2 AND status = 'active' AND effective_to IS NULL
                 ORDER BY version DESC
                 LIMIT 1",
            )
            .bind(entity)
            .bind(key),
        )
        .await?;
    let mut def = row.map(row_to_def).transpose()?;
    if let Some(d) = def.as_mut() {
        overlay_live_status(tx, d).await?;
        if d.status != DefinitionStatus::Active {
            return Ok(None);
        }
    }
    Ok(def)
}

pub(crate) async fn list_active_for_entity(
    tx: &mut Tx<'_>,
    entity: &str,
) -> Result<Vec<Definition>> {
    let rows = tx
        .fetch_all(
            sqlx::query_as::<_, DefRow>(
                "SELECT DISTINCT ON (entity, key)
                        definition_id, version, entity, key, field_type, label, validation_rule,
                        required, indexed, owner_module, status
                 FROM customfields.definition
                 WHERE entity = $1 AND status = 'active' AND effective_to IS NULL
                 ORDER BY entity, key, version DESC",
            )
            .bind(entity),
        )
        .await?;
    let mut out = Vec::new();
    for r in rows {
        let mut def = row_to_def(r)?;
        overlay_live_status(tx, &mut def).await?;
        if def.status == DefinitionStatus::Active {
            out.push(def);
        }
    }
    Ok(out)
}

/// Live status is the machine. The column is the insert-time snapshot.
async fn overlay_live_status(tx: &mut Tx<'_>, def: &mut Definition) -> Result<()> {
    let live = current_state(tx, &doc_ref(def.id)).await?;
    if let Some(state) = live {
        def.status = DefinitionStatus::parse(&state.0).ok_or_else(|| {
            Error::Core(datum_core::Error::Invariant(format!(
                "bad machine state {}",
                state.0
            )))
        })?;
    }
    Ok(())
}

pub(crate) async fn insert_definition_version(
    tx: &mut Tx<'_>,
    id: DefinitionId,
    version: i32,
    spec: &DefinitionSpec,
    status: DefinitionStatus,
) -> Result<()> {
    parse_rule_at_define(&spec.validation_rule, spec.field_type)?;
    tx.execute(
        sqlx::query(
            "INSERT INTO customfields.definition
             (definition_id, version, entity, key, field_type, label, validation_rule,
              required, indexed, owner_module, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(id.as_uuid())
        .bind(version)
        .bind(&spec.entity)
        .bind(&spec.key)
        .bind(spec.field_type.as_str())
        .bind(&spec.label)
        .bind(&spec.validation_rule)
        .bind(spec.required)
        .bind(spec.indexed)
        .bind(&spec.owner_module)
        .bind(status.as_str()),
    )
    .await?;
    Ok(())
}

pub(crate) async fn close_previous_version(
    tx: &mut Tx<'_>,
    id: DefinitionId,
    version: i32,
) -> Result<()> {
    tx.execute(
        sqlx::query(
            "UPDATE customfields.definition
             SET effective_to = pg_catalog.now()
             WHERE definition_id = $1 AND version = $2 AND effective_to IS NULL",
        )
        .bind(id.as_uuid())
        .bind(version),
    )
    .await?;
    Ok(())
}

pub(crate) async fn upsert_value(
    tx: &mut Tx<'_>,
    def: &Definition,
    record_id: Identifier,
    value: &Value,
) -> Result<()> {
    if def.status == DefinitionStatus::Retired {
        return Err(Error::Retired);
    }
    validate_value(def, value)?;
    let ver = def.version;
    let rid = record_id.as_uuid();
    let did = def.id.as_uuid();
    match value {
        Value::String(v) => {
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_string (definition_id, record_id, definition_version, value)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version, value = EXCLUDED.value",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(v),
            )
            .await?;
        }
        Value::Text(v) => {
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_text (definition_id, record_id, definition_version, value)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version, value = EXCLUDED.value",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(v),
            )
            .await?;
        }
        Value::Integer(v) => {
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_integer (definition_id, record_id, definition_version, value)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version, value = EXCLUDED.value",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(v),
            )
            .await?;
        }
        Value::Decimal { value: dec, scale } => {
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_decimal (definition_id, record_id, definition_version, value, scale)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version, value = EXCLUDED.value,
                         scale = EXCLUDED.scale",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(dec)
                .bind(scale),
            )
            .await?;
        }
        Value::Bool(v) => {
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_bool (definition_id, record_id, definition_version, value)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version, value = EXCLUDED.value",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(v),
            )
            .await?;
        }
        Value::Date { date, precision } => {
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_date (definition_id, record_id, definition_version, value, precision)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version, value = EXCLUDED.value,
                         precision = EXCLUDED.precision",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(date)
                .bind(precision.as_str()),
            )
            .await?;
        }
        Value::Enum(v) => {
            if let Some(opts) = def.validation_rule.strip_prefix("enum:") {
                let allowed: Vec<&str> = opts
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                if !allowed.is_empty() && !allowed.contains(&v.as_str()) {
                    return Err(Error::ValidationFailed {
                        rule: def.validation_rule.clone(),
                    });
                }
            }
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_enum (definition_id, record_id, definition_version, value)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version, value = EXCLUDED.value",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(v),
            )
            .await?;
        }
        Value::Reference { entity, id } => {
            tx.execute(
                sqlx::query(
                    "INSERT INTO customfields.value_reference (definition_id, record_id, definition_version, ref_entity, ref_id)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (definition_id, record_id) DO UPDATE
                     SET definition_version = EXCLUDED.definition_version,
                         ref_entity = EXCLUDED.ref_entity, ref_id = EXCLUDED.ref_id",
                )
                .bind(did)
                .bind(rid)
                .bind(ver)
                .bind(entity)
                .bind(id.as_uuid()),
            )
            .await?;
        }
    }
    Ok(())
}

pub(crate) async fn load_value(
    tx: &mut Tx<'_>,
    def: &Definition,
    record_id: Identifier,
) -> Result<Option<Value>> {
    let rid = record_id.as_uuid();
    let did = def.id.as_uuid();
    match def.field_type {
        FieldType::String => {
            let row: Option<(String,)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT value FROM customfields.value_string WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.map(|(v,)| Value::String(v)))
        }
        FieldType::Text => {
            let row: Option<(String,)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT value FROM customfields.value_text WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.map(|(v,)| Value::Text(v)))
        }
        FieldType::Integer => {
            let row: Option<(i64,)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT value FROM customfields.value_integer WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.map(|(v,)| Value::Integer(v)))
        }
        FieldType::Decimal => {
            let row: Option<(Decimal, i16)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT value, scale FROM customfields.value_decimal
                         WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.map(|(value, scale)| Value::Decimal { value, scale }))
        }
        FieldType::Bool => {
            let row: Option<(bool,)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT value FROM customfields.value_bool WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.map(|(v,)| Value::Bool(v)))
        }
        FieldType::Date => {
            let row: Option<(NaiveDate, String)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT value, precision FROM customfields.value_date
                         WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.and_then(|(date, prec_s)| {
                DatePrecision::parse(&prec_s).map(|precision| Value::Date { date, precision })
            }))
        }
        FieldType::Enum => {
            let row: Option<(String,)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT value FROM customfields.value_enum WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.map(|(v,)| Value::Enum(v)))
        }
        FieldType::Reference => {
            let row: Option<(String, Uuid)> = tx
                .fetch_optional(
                    sqlx::query_as(
                        "SELECT ref_entity, ref_id FROM customfields.value_reference
                         WHERE definition_id = $1 AND record_id = $2",
                    )
                    .bind(did)
                    .bind(rid),
                )
                .await?;
            Ok(row.map(|(entity, id)| Value::Reference {
                entity,
                id: Identifier::from_uuid(id),
            }))
        }
    }
}

pub(crate) fn caller_may_retire(action: &str, owner: &str) -> bool {
    if action == "customfields.define" || action.starts_with("customfields.") {
        return true;
    }
    let module = action.split('.').next().unwrap_or("");
    module == owner
}
