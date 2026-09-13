//! Inventory document state machine. Edges are declared in `module.toml`
//! and registered through [`wicket_module::KernelBuilder`] with explicit
//! [`wicket_statemachine::SignatureDeclaration::NotRequired`] reasons.

use wicket_statemachine::{EdgeBuilder, Machine};

use crate::domain::{DOC_TYPE, NOT_REQUIRED_REASON};
use crate::error::Result;

/// Machine declared through `wicket-statemachine` with `NotRequired` on every edge.
pub fn document_machine() -> Result<Machine> {
    Ok(Machine::builder(DOC_TYPE)
        .regulated(false)
        .state("draft")
        .state("posted")
        .state("voided")
        .edge(
            EdgeBuilder::new("draft", "posted", "receive", "inventory.receive")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("draft", "posted", "issue", "inventory.issue")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("draft", "posted", "move", "inventory.move")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("draft", "posted", "adjust", "inventory.adjust")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("draft", "posted", "count", "inventory.count")
                .not_required(NOT_REQUIRED_REASON),
        )
        .edge(
            EdgeBuilder::new("posted", "voided", "void", "inventory.adjust")
                .not_required(NOT_REQUIRED_REASON),
        )
        .build()?)
}
