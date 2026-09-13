//! Item release state machine (`draft → released → obsolete`).

use wicket_statemachine::{EdgeBuilder, Machine};

use crate::domain::{DOC_TYPE, NOT_REQUIRED_REASON};
use crate::error::Result;

/// Machine declared through `wicket-statemachine` with `NotRequired` on every edge.
pub fn item_machine() -> Result<Machine> {
    Ok(Machine::builder(DOC_TYPE)
        .regulated(false)
        .state("draft")
        .state("released")
        .state("obsolete")
        .edge(
            EdgeBuilder::new("draft", "released", "release", "items.release")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("released", "obsolete", "obsolete", "items.release")
                .not_required(NOT_REQUIRED_REASON),
        )
        .build()?)
}
