//! Typed events. Subscriptions are declared in `module.toml` and registered
//! through `KernelBuilder::apply_manifest` / `events.subscribe`.

use datum_core::{Identifier, ItemId, LocationId, LotId};
use datum_events::{Event, EventSchema, Field};
use rust_decimal::Decimal;
use serde_json::json;

use crate::error::Result;

/// Lot-controlled receipt (genealogy subscribes). `lot_id` is required.
pub const LOT_RECEIVED: &str = "inventory.lot_received";
/// Lot-less receipt (R-2s-4). No `lot_id`.
pub const RECEIPT_POSTED: &str = "inventory.receipt_posted";
/// Issue posted.
pub const ISSUED: &str = "inventory.issued";
/// Adjustment posted.
pub const ADJUSTED: &str = "inventory.adjusted";

/// Register `inventory.issued.v1`. Kernel-owned inventory contracts live in
/// [`datum_events::SchemaRegistry::standard`] (R-2s-4).
pub fn register_schemas() -> Result<()> {
    datum_events::schema::register(EventSchema {
        name: ISSUED.into(),
        version: 1,
        fields: vec![
            Field::required("item_id"),
            Field::required("qty"),
            Field::optional("lot_id"),
            Field::optional("work_order_id"),
        ],
    })?;
    Ok(())
}

/// `inventory.lot_received.v1` (`lot_id` required by the kernel standard registry).
pub fn lot_received(lot: LotId, item: ItemId, qty: Decimal, document: Identifier) -> Result<Event> {
    Event::builder()
        .name(LOT_RECEIVED)
        .version(1)
        .document("inventory", document)
        .payload(json!({
            "lot_id": lot.to_string(),
            "item_id": item.to_string(),
            "qty": qty.to_string(),
        }))
        .build()
        .map_err(Into::into)
}

/// `inventory.receipt_posted.v1` for a lot-less receipt (R-2s-4).
pub fn receipt_posted(
    item: ItemId,
    location: LocationId,
    qty: Decimal,
    uom: i64,
    posting_group_id: Identifier,
    document: Identifier,
) -> Result<Event> {
    Event::builder()
        .name(RECEIPT_POSTED)
        .version(1)
        .document("inventory", document)
        .payload(json!({
            "item_id": item.to_string(),
            "location_id": location.to_string(),
            "qty": qty.to_string(),
            "uom": uom,
            "posting_group_id": posting_group_id.to_string(),
        }))
        .build_with(&datum_events::SchemaRegistry::standard())
        .map_err(Into::into)
}

/// `inventory.issued.v1`.
pub fn issued(
    item: ItemId,
    qty: Decimal,
    lot: Option<LotId>,
    work_order: Option<Identifier>,
    document: Identifier,
) -> Result<Event> {
    let mut payload = serde_json::Map::new();
    payload.insert("item_id".into(), json!(item.to_string()));
    payload.insert("qty".into(), json!(qty.to_string()));
    if let Some(lot) = lot {
        payload.insert("lot_id".into(), json!(lot.to_string()));
    }
    if let Some(wo) = work_order {
        payload.insert("work_order_id".into(), json!(wo.to_string()));
    }
    Event::builder()
        .name(ISSUED)
        .version(1)
        .document("inventory", document)
        .payload(serde_json::Value::Object(payload))
        .build()
        .map_err(Into::into)
}

/// `inventory.adjusted.v1`.
pub fn adjusted(
    item: ItemId,
    qty: Decimal,
    reason_code: &str,
    document: Identifier,
) -> Result<Event> {
    Event::builder()
        .name(ADJUSTED)
        .version(1)
        .document("inventory", document)
        .payload(json!({
            "item_id": item.to_string(),
            "qty": qty.to_string(),
            "reason_code": reason_code,
        }))
        .build_with(&datum_events::SchemaRegistry::standard())
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::{Identifier, ItemId, LocationId};
    use datum_events::SchemaRegistry;

    #[test]
    fn receipt_posted_builds_from_standard_registry_without_module_register() {
        let reg = SchemaRegistry::standard();
        reg.get(RECEIPT_POSTED, 1)
            .expect("kernel standard seeds receipt_posted.v1");
        let event = receipt_posted(
            ItemId::generate(),
            LocationId::generate(),
            Decimal::ONE,
            1,
            Identifier::generate(),
            Identifier::generate(),
        )
        .expect("receipt_posted helper");
        reg.validate(RECEIPT_POSTED, 1, &event.payload)
            .expect("payload matches standard contract");
    }

    #[test]
    fn adjusted_builds_from_standard_registry_without_module_register() {
        let reg = SchemaRegistry::standard();
        reg.get(ADJUSTED, 1)
            .expect("kernel standard seeds adjusted.v1");
        let event = adjusted(
            ItemId::generate(),
            Decimal::new(-3, 0),
            "SCRAP",
            Identifier::generate(),
        )
        .expect("adjusted helper");
        reg.validate(ADJUSTED, 1, &event.payload)
            .expect("payload matches standard contract");
    }
}
