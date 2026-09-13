//! Documents composition: machine registration, event schemas, emission.

use serde_json::{Value, json};
use wicket_core::Identifier;
use wicket_db::Tx;
use wicket_documents::{
    DOC_TYPE, DocumentId, EVENT_EFFECTIVE, EVENT_REVISION_CREATED, EVENT_SCHEMAS, RevisionId,
    document_machine,
};
use wicket_events::{Event, EventSchema, Field};
use wicket_statemachine::Engine;

use crate::{Error, Result};

/// Register [`wicket_documents::document_machine`] for the live profile.
pub(crate) fn register_document_machine(engine: &mut Engine, profile: &str) -> Result<()> {
    Ok(engine.register_machine(document_machine(profile)?)?)
}

/// Seed documents payload contracts on `registry` and the process-global
/// registry (CONTRACT §4: the crate exports schemas; registration is here).
pub(crate) fn register_document_event_schemas(
    registry: &mut wicket_events::SchemaRegistry,
) -> Result<()> {
    for decl in EVENT_SCHEMAS {
        let schema = EventSchema {
            name: decl.name.to_owned(),
            version: decl.version,
            fields: decl.fields.iter().copied().map(Field::required).collect(),
        };
        registry.register(schema.clone())?;
        wicket_events::schema::register(schema)?;
    }
    Ok(())
}

pub(crate) async fn emit_revision_created(
    tx: &mut Tx<'_>,
    doc: DocumentId,
    rev: RevisionId,
    label: &str,
) -> Result<Identifier> {
    let event = Event::builder()
        .name(EVENT_REVISION_CREATED)
        .version(1)
        .payload(json!({
            "document_id": doc.as_uuid().to_string(),
            "revision_id": rev.as_uuid().to_string(),
            "label": label,
        }))
        .document(DOC_TYPE, doc.0)
        .build()?;
    Ok(wicket_events::publish(tx, event).await?)
}

pub(crate) async fn emit_effective(tx: &mut Tx<'_>, doc: DocumentId) -> Result<Identifier> {
    let revs = wicket_documents::history(tx, doc).await?;
    let rev = revs
        .last()
        .ok_or_else(|| Error::Manifest("make_effective with no revision".into()))?;
    let effective_from = rev
        .manifest
        .effective_from
        .map(|ts| Value::String(ts.to_rfc3339()))
        .unwrap_or(Value::Null);
    let event = Event::builder()
        .name(EVENT_EFFECTIVE)
        .version(1)
        .payload(json!({
            "document_id": doc.as_uuid().to_string(),
            "revision_id": rev.id.as_uuid().to_string(),
            "effective_from": effective_from,
        }))
        .document(DOC_TYPE, doc.0)
        .build()?;
    Ok(wicket_events::publish(tx, event).await?)
}
