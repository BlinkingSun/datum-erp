//! Typed events this module emits.

use serde_json::json;
use wicket_core::{Identifier, ItemId};
use wicket_events::{Event, EventSchema, Field, SchemaRegistry};

use crate::domain::Item;
use crate::error::Result;

/// `items.item_released.v1`
pub const ITEM_RELEASED: &str = "items.item_released";
/// `items.item_obsoleted.v1`
pub const ITEM_OBSOLETED: &str = "items.item_obsoleted";

/// Register payload contracts on the process-global registry (append-only).
pub fn register_event_schemas() -> Result<()> {
    let mut extra = SchemaRegistry::new();
    extra.register(EventSchema {
        name: ITEM_RELEASED.into(),
        version: 1,
        fields: vec![
            Field::required("item_id"),
            Field::required("number"),
            Field::optional("revision"),
        ],
    })?;
    extra.register(EventSchema {
        name: ITEM_OBSOLETED.into(),
        version: 1,
        fields: vec![
            Field::required("item_id"),
            Field::required("number"),
            Field::optional("revision"),
        ],
    })?;
    for schema in extra.iter() {
        wicket_events::schema::register(schema.clone())?;
    }
    Ok(())
}

/// Build `items.item_released.v1` for `item`.
pub fn item_released(item: &Item) -> Result<Event> {
    event(ITEM_RELEASED, item)
}

/// Build `items.item_obsoleted.v1` for `item`.
pub fn item_obsoleted(item: &Item) -> Result<Event> {
    event(ITEM_OBSOLETED, item)
}

fn event(name: &str, item: &Item) -> Result<Event> {
    register_event_schemas()?;
    Ok(Event::builder()
        .name(name)
        .version(1)
        .payload(json!({
            "item_id": item.id.to_string(),
            "number": item.number,
            "revision": item.revision,
        }))
        .document("items", Identifier::from_uuid(item.id.as_uuid()))
        .build()?)
}

/// Payload `item_id` as an [`ItemId`] (tests).
pub fn payload_item_id(event: &Event) -> Option<ItemId> {
    event
        .payload
        .get("item_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .map(ItemId::from_uuid)
}
