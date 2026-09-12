//! Work-order state machine. Edges are declared in `module.toml` and registered
//! through [`datum_module::KernelBuilder`] with explicit
//! [`datum_statemachine::SignatureDeclaration::NotRequired`] reasons.

use datum_core::Identifier;
use datum_statemachine::{DocRef, EdgeBuilder, Machine};

use crate::domain::{DOC_TYPE, NOT_REQUIRED_REASON};
use crate::error::Result;

/// Machine declared through `datum-statemachine` with `NotRequired` on every edge.
pub fn work_order_machine() -> Result<Machine> {
    Ok(Machine::builder(DOC_TYPE)
        .regulated(false)
        .state("draft")
        .state("released")
        .state("in_process")
        .state("completed")
        .state("cancelled")
        .edge(
            EdgeBuilder::new("draft", "released", "release", "production.release")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("released", "in_process", "issue", "production.issue")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("in_process", "completed", "complete", "production.complete")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("draft", "cancelled", "cancel_draft", "production.release")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("released", "cancelled", "cancel", "production.release")
                .not_required(NOT_REQUIRED_REASON),
        )
        .build()?)
}

/// Document reference for a work order.
pub fn doc_ref(id: Identifier) -> DocRef {
    DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: id,
    }
}
