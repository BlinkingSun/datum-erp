//! Typed events. Subscriptions are declared in `module.toml` and registered
//! through `KernelBuilder::apply_manifest` / `events.subscribe`.

use datum_core::{Identifier, LotId};
use datum_events::{Event, EventSchema, Field};
use serde_json::json;

use crate::error::Result;

/// Work order released (number allocated, WIP location exists).
pub const WORK_ORDER_RELEASED: &str = "production.work_order_released";
/// Work order completed (genealogy subscribes).
pub const COMPLETED: &str = "production.completed";

/// Register payload contracts on the process-global schema registry.
pub fn register_schemas() -> Result<()> {
    datum_events::schema::register(EventSchema {
        name: WORK_ORDER_RELEASED.into(),
        version: 1,
        fields: vec![Field::required("work_order_id"), Field::required("number")],
    })?;
    datum_events::schema::register(EventSchema {
        name: COMPLETED.into(),
        version: 1,
        fields: vec![
            Field::required("work_order_id"),
            Field::required("finished_lot_id"),
            Field::required("posting_group_id"),
        ],
    })?;
    Ok(())
}

/// `production.work_order_released.v1`.
pub fn work_order_released(work_order: Identifier, number: &str) -> Result<Event> {
    Event::builder()
        .name(WORK_ORDER_RELEASED)
        .version(1)
        .document("production", work_order)
        .payload(json!({
            "work_order_id": work_order.to_string(),
            "number": number,
        }))
        .build()
        .map_err(Into::into)
}

/// `production.completed.v1`.
pub fn completed(
    work_order: Identifier,
    finished_lot: LotId,
    posting_group_id: Identifier,
) -> Result<Event> {
    Event::builder()
        .name(COMPLETED)
        .version(1)
        .document("production", work_order)
        .payload(json!({
            "work_order_id": work_order.to_string(),
            "finished_lot_id": finished_lot.to_string(),
            "posting_group_id": posting_group_id.to_string(),
        }))
        .build()
        .map_err(Into::into)
}
