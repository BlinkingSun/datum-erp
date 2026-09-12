//! Transactional outbox: typed events, at-least-once asynchronous delivery.
//!
//! A module publishes a typed [`Event`] inside the same [`datum_db::Tx`] as its
//! business change. The row in `app.event` becomes visible only if that change
//! commits. [`Dispatcher`] delivers to in-process subscribers at least once,
//! outside the originating transaction, as a named service principal
//! (`WriteContext.source_kind = "job"`).
//!
//! # Idempotency
//!
//! Handlers **must be idempotent**. Delivery is at-least-once: a handler crash
//! after side effects but before the `transient.delivery` row is committed
//! causes a re-delivery. The test `delivery_is_at_least_once` proves the
//! re-delivery (side-effect count 2, `attempts = 2`); idempotency is what makes
//! that acceptable.
//!
//! `app.event` is append-only (schema class `app`, D-W1-2): fields may be added
//! to a payload contract, never removed or repurposed within a version. There
//! is no DELETE path on `app.event`. `transient.delivery` is working state and
//! is not audited.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod dispatch;
pub mod error;
pub mod event;
pub mod publish;
pub mod schema;
mod sql;
pub mod subscribe;

pub use dispatch::{DEFAULT_MAX_ATTEMPTS, Dispatcher};
pub use error::{Error, Result};
pub use event::{Event, EventBuilder, EventKind};
pub use publish::publish;
pub use schema::{EventSchema, Field, SchemaRegistry};
pub use subscribe::{EventHandler, HandlerFuture, Registry, enable_subscription};

/// Embedded migrator (`placeholder` + `0001_events`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[cfg(test)]
mod tests {
    use super::*;
    use datum_audit as _;
    use proptest::prelude::*;
    use serde_json::json;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(MIGRATOR.migrations.len() >= 2);
        assert!(MIGRATOR.iter().any(|m| m.version == 1));
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn builder_refuses_unknown_schema() {
        let registry = SchemaRegistry::new();
        let err = Event::builder()
            .name("no.such")
            .version(1)
            .payload(json!({}))
            .build_with(&registry)
            .unwrap_err();
        assert!(matches!(err, Error::UnknownSchema { .. }));
    }

    #[test]
    fn builder_refuses_missing_required_field() {
        let registry = SchemaRegistry::standard();
        let err = Event::builder()
            .name("inventory.lot_received")
            .version(1)
            .payload(json!({"lot_id": "x"}))
            .build_with(&registry)
            .unwrap_err();
        assert!(matches!(err, Error::MissingField { ref field, .. } if field == "item_id"));
    }

    #[test]
    fn schema_registry_rejects_removed_field() {
        let mut registry = SchemaRegistry::new();
        registry
            .register(EventSchema {
                name: "demo.thing".into(),
                version: 1,
                fields: vec![Field::required("a"), Field::required("b")],
            })
            .unwrap();
        let err = registry
            .register(EventSchema {
                name: "demo.thing".into(),
                version: 1,
                fields: vec![Field::required("a")],
            })
            .unwrap_err();
        assert!(matches!(err, Error::RemovedField { ref field, .. } if field == "b"));

        let standard = SchemaRegistry::standard();
        let lot = standard.get("inventory.lot_received", 1).expect("seeded");
        for frozen in SchemaRegistry::INVENTORY_LOT_RECEIVED_V1_FIELDS {
            assert!(
                lot.fields.iter().any(|f| f.name == *frozen),
                "inventory.lot_received.v1 dropped field {frozen}; payload contracts are append-only"
            );
        }
    }

    #[test]
    fn standard_registry_has_inventory_receipt_posted_v1() {
        let registry = SchemaRegistry::standard();
        let schema = registry
            .get("inventory.receipt_posted", 1)
            .expect("inventory.receipt_posted.v1 is seeded");
        assert_eq!(schema.version, 1);
        let names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names.as_slice(),
            SchemaRegistry::INVENTORY_RECEIPT_POSTED_V1_FIELDS
        );
        assert!(schema.fields.iter().all(|f| f.required));
        assert!(
            !names.contains(&"lot_id"),
            "lot-less receipt_posted.v1 must not declare lot_id"
        );

        let lot = registry
            .get("inventory.lot_received", 1)
            .expect("inventory.lot_received.v1 stays seeded");
        assert!(
            lot.fields.iter().any(|f| f.name == "lot_id" && f.required),
            "lot_id stays required on inventory.lot_received.v1"
        );
    }

    #[test]
    fn receipt_posted_rejects_lot_id() {
        let registry = SchemaRegistry::standard();
        let err = Event::builder()
            .name("inventory.receipt_posted")
            .version(1)
            .payload(json!({
                "item_id": "item",
                "location_id": "loc",
                "qty": "1",
                "uom": 1,
                "posting_group_id": "pg",
                "lot_id": "must-not-be-present",
            }))
            .build_with(&registry)
            .unwrap_err();
        assert!(matches!(
            err,
            Error::UnexpectedField { ref field, .. } if field == "lot_id"
        ));
    }

    #[test]
    fn adjusted_round_trip() {
        let registry = SchemaRegistry::standard();
        let payload = json!({
            "item_id": "item",
            "qty": "1.5",
            "reason_code": "CYCLE_COUNT",
        });
        let event = Event::builder()
            .name("inventory.adjusted")
            .version(1)
            .payload(payload.clone())
            .build_with(&registry)
            .expect("adjusted v1 accepts the module payload");
        assert_eq!(event.name, "inventory.adjusted");
        assert_eq!(event.version, 1);
        assert_eq!(event.payload, payload);
        registry
            .validate("inventory.adjusted", 1, &event.payload)
            .expect("re-validate");
        let encoded = serde_json::to_value(&event).expect("serialize");
        let decoded: Event = serde_json::from_value(encoded).expect("deserialize");
        assert_eq!(decoded.name, event.name);
        assert_eq!(decoded.version, event.version);
        assert_eq!(decoded.payload, event.payload);
        registry
            .validate(&decoded.name, decoded.version, &decoded.payload)
            .expect("round-trip still matches schema");
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
