//! Typed events `lots.lot_created.v1`, `lots.status_changed.v1`, `lots.serials_created.v1`.
//!
//! Subscriptions are declared in `module.toml` and registered through
//! `KernelBuilder::apply_manifest` / `events.subscribe`, never by a constant here.

use datum_core::{Identifier, ItemId, LotId, SerialId};
use datum_events::{Event, EventSchema, Field};
use serde_json::json;

use crate::domain::LotStatus;
use crate::error::Result;

/// Event name: lot created.
pub const LOT_CREATED: &str = "lots.lot_created";
/// Event name: status changed.
pub const STATUS_CHANGED: &str = "lots.status_changed";
/// Event name: serials created.
pub const SERIALS_CREATED: &str = "lots.serials_created";

/// Register payload contracts on the process-global schema registry.
pub fn register_schemas() -> Result<()> {
    datum_events::schema::register(EventSchema {
        name: LOT_CREATED.into(),
        version: 1,
        fields: vec![
            Field::required("lot_id"),
            Field::required("item_id"),
            Field::required("number"),
        ],
    })?;
    datum_events::schema::register(EventSchema {
        name: STATUS_CHANGED.into(),
        version: 1,
        fields: vec![
            Field::required("to"),
            Field::required("reason"),
            Field::optional("lot_id"),
            Field::optional("serial_id"),
            Field::optional("from"),
        ],
    })?;
    datum_events::schema::register(EventSchema {
        name: SERIALS_CREATED.into(),
        version: 1,
        fields: vec![
            Field::required("lot_id"),
            Field::required("count"),
            Field::required("serial_ids"),
        ],
    })?;
    Ok(())
}

/// `lots.lot_created.v1`.
pub fn lot_created(lot: LotId, item: ItemId, number: &str) -> Result<Event> {
    Event::builder()
        .name(LOT_CREATED)
        .version(1)
        .document("lot", Identifier::from_uuid(lot.as_uuid()))
        .payload(json!({
            "lot_id": lot.to_string(),
            "item_id": item.to_string(),
            "number": number,
        }))
        .build()
        .map_err(Into::into)
}

/// `lots.status_changed.v1`.
pub fn status_changed(
    lot: Option<LotId>,
    serial: Option<SerialId>,
    from: Option<LotStatus>,
    to: LotStatus,
    reason: &str,
) -> Result<Event> {
    let mut payload = serde_json::Map::new();
    if let Some(id) = lot {
        payload.insert("lot_id".into(), json!(id.to_string()));
    }
    if let Some(id) = serial {
        payload.insert("serial_id".into(), json!(id.to_string()));
    }
    if let Some(from) = from {
        payload.insert("from".into(), json!(from.as_str()));
    }
    payload.insert("to".into(), json!(to.as_str()));
    payload.insert("reason".into(), json!(reason));
    let doc_id = lot
        .map(|l| Identifier::from_uuid(l.as_uuid()))
        .or_else(|| serial.map(|s| Identifier::from_uuid(s.as_uuid())));
    let mut b = Event::builder()
        .name(STATUS_CHANGED)
        .version(1)
        .payload(serde_json::Value::Object(payload));
    if let Some(id) = doc_id {
        b = b.document("lot", id);
    }
    b.build().map_err(Into::into)
}

/// `lots.serials_created.v1`.
pub fn serials_created(lot: LotId, ids: &[SerialId]) -> Result<Event> {
    Event::builder()
        .name(SERIALS_CREATED)
        .version(1)
        .document("lot", Identifier::from_uuid(lot.as_uuid()))
        .payload(json!({
            "lot_id": lot.to_string(),
            "count": ids.len() as i64,
            "serial_ids": ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        }))
        .build()
        .map_err(Into::into)
}
