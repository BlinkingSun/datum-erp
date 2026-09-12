//! Lot status machine. Edges are declared in `module.toml` and registered through
//! [`datum_module::KernelBuilder`]. This module never posts; the inventory module
//! posts the quarantine/available movement.

use datum_core::{PermissionKey, SignatureMeaning, SignatureRequirement};
use datum_module::Profile;
use datum_statemachine::{EdgeBuilder, Machine};

use crate::domain::LotStatus;
use crate::error::Result;

/// Document type registered on the lot status machine.
pub const DOC_TYPE: &str = "lot";

/// States the machine enumerates (must match `module.toml`).
pub const STATES: &[LotStatus] = &[
    LotStatus::Quarantine,
    LotStatus::Available,
    LotStatus::Hold,
    LotStatus::Rejected,
];

/// Build the lot machine. Under regulated-device, `release` is a Required edge.
pub fn lot_machine(profile: &Profile) -> Result<Machine> {
    let regulated = profile.id == datum_module::ProfileId::RegulatedDevice;
    let mut b = Machine::builder(DOC_TYPE).regulated(regulated);
    for s in STATES {
        b = b.state(s.as_str());
    }
    let release = EdgeBuilder::new("quarantine", "available", "release", "lots.status");
    b = if regulated {
        b.edge(release.required(SignatureRequirement {
            meaning: SignatureMeaning("Lot released".into()),
            permission: PermissionKey("lots.status".into()),
        }))
    } else {
        b.edge(
            release.not_required("lot status is a business record; inventory posts the movement"),
        )
    };
    for (from, to, name) in [
        ("available", "hold", "hold"),
        ("hold", "available", "unhold"),
        ("quarantine", "rejected", "reject_from_quarantine"),
        ("available", "rejected", "reject"),
        ("hold", "rejected", "reject_from_hold"),
    ] {
        b = b.edge(
            EdgeBuilder::new(from, to, name, "lots.status")
                .not_required("lot status is a business record; inventory posts the movement"),
        );
    }
    Ok(b.build()?)
}
