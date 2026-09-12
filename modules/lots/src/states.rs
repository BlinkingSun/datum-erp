//! Lot status machine. Edges are declared in `module.toml` and registered through
//! [`datum_module::KernelBuilder::apply_manifest`]. This module never posts; the
//! inventory module posts the quarantine/available movement.

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
