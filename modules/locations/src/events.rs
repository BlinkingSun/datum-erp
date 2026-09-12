//! Event contracts published by this module.

use datum_events::{EventSchema, Field, SchemaRegistry};

/// `locations.location_deactivated` v1.
pub const LOCATION_DEACTIVATED: &str = "locations.location_deactivated";

/// Register payload schemas declared by this module.
pub fn register_schemas(registry: &mut SchemaRegistry) -> crate::Result<()> {
    registry
        .register(EventSchema {
            name: LOCATION_DEACTIVATED.into(),
            version: 1,
            fields: vec![Field::required("location_id")],
        })
        .map_err(Into::into)
}
