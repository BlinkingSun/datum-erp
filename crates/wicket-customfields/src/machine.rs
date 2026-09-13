//! Definition lifecycle machine: `active → retired` (terminal).
//!
//! The composition root registers [`definition_machine`] on the kernel engine
//! the same way it registers [`wicket_documents::document_machine`]. The catalog
//! is identical in both installation profiles: retiring a field is
//! configuration, not a quality approval, so `retire` is `NotRequired`.

use wicket_statemachine::{DocRef, EdgeBuilder, Engine, Machine, with_action};

use crate::domain::{DOC_TYPE, DefinitionId, DefinitionStatus};
use crate::error::Result;

/// Edge name for Active → Retired.
pub const RETIRE_EDGE: &str = "retire";

/// RBAC key the executor checks on [`RETIRE_EDGE`].
pub const RETIRE_PERMISSION: &str = "customfields.retire";

/// Build the definition lifecycle machine.
///
/// `profile` is accepted so the composition root can call this the same way as
/// `document_machine`. The persisted catalog does not vary by profile.
pub fn definition_machine(profile: &str) -> Result<Machine> {
    match profile {
        "plain-shop" | "regulated-device" => {}
        other => return Err(crate::error::Error::UnknownProfile(other.to_owned())),
    }
    Ok(Machine::builder(DOC_TYPE)
        .regulated(false)
        .state(DefinitionStatus::Active.as_str())
        .state(DefinitionStatus::Retired.as_str())
        .edge(
            EdgeBuilder::new(
                DefinitionStatus::Active.as_str(),
                DefinitionStatus::Retired.as_str(),
                RETIRE_EDGE,
                RETIRE_PERMISSION,
            )
            .not_required(
                "retiring a custom field definition is configuration, not a quality approval",
            ),
        )
        .build()?)
}

/// Kernel document reference for a definition instance.
pub fn doc_ref(id: DefinitionId) -> DocRef {
    DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: id.as_identifier(),
    }
}

/// Bind `WriteContext.action` to `"customfields.definition.retire"` for [`crate::retire`].
pub fn retire_context(ctx: wicket_db::WriteContext, id: DefinitionId) -> wicket_db::WriteContext {
    with_action(ctx, &doc_ref(id), RETIRE_EDGE)
}

/// Frozen engine carrying only this crate's machine. Used by [`crate::define`]
/// to spawn; composition and tests register the same declaration on their engine.
pub(crate) fn frozen_engine(profile: &str) -> Result<Engine> {
    let mut eng = Engine::new();
    eng.register_machine(definition_machine(profile)?)?;
    eng.freeze()?;
    Ok(eng)
}
