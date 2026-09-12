//! Lot status machine. Edges are declared in `module.toml` and registered through
//! [`datum_module::KernelBuilder::apply_manifest`]. This module never posts; the
//! inventory module posts the quarantine/available movement.

use datum_core::{Identifier, LotId, SerialId};
use datum_statemachine::DocRef;

use crate::domain::LotStatus;

/// Document type registered on the lot status machine.
pub const DOC_TYPE: &str = "lot";

/// States the machine enumerates (must match `module.toml`).
pub const STATES: &[LotStatus] = &[
    LotStatus::Quarantine,
    LotStatus::Available,
    LotStatus::Hold,
    LotStatus::Rejected,
];

/// Kernel document reference for a lot instance.
pub fn doc_ref_lot(id: LotId) -> DocRef {
    DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::from_uuid(id.as_uuid()),
    }
}

/// Kernel document reference for a serial instance (same machine, serial id).
pub fn doc_ref_serial(id: SerialId) -> DocRef {
    DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::from_uuid(id.as_uuid()),
    }
}

/// Manifest edge name for `from → to`, if the transition is legal.
pub fn edge_for_transition(from: LotStatus, to: LotStatus) -> Option<&'static str> {
    match (from, to) {
        (LotStatus::Quarantine, LotStatus::Available) => Some("release"),
        (LotStatus::Available, LotStatus::Hold) => Some("hold"),
        (LotStatus::Hold, LotStatus::Available) => Some("unhold"),
        (LotStatus::Quarantine, LotStatus::Rejected) => Some("reject_from_quarantine"),
        (LotStatus::Available, LotStatus::Rejected) => Some("reject"),
        (LotStatus::Hold, LotStatus::Rejected) => Some("reject_from_hold"),
        _ => None,
    }
}
