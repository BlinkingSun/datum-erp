//! Composition root: module registry, profiles, and kernel wiring.
//!
//! Wires [`datum_core::PostingSink`] ([`datum_ledger::GroupBuilder`]) and
//! [`datum_esign::GateFactory`] (`NoSignatures` or the prepared esign gate).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

#[cfg(test)]
use chrono as _;
use datum_print as _;
#[cfg(test)]
use rust_decimal as _;

mod config;
mod documents;
mod error;
mod install_graph;
mod kernel;
mod manifest;
mod order;
mod profile;
mod registry;
mod semver;
mod toml;

pub use config::{
    ConfigurationManifest, KERNEL_DEFAULTS_ID, KernelDefaults, ManifestModule,
    export as export_manifest, load_kernel_defaults, verify,
};
pub use error::{Error, Result};
pub use install_graph::{
    GENEALOGY_MIGRATOR, INVENTORY_MIGRATOR, ITEMS_MIGRATOR, LOCATIONS_MIGRATOR, LOTS_MIGRATOR,
    ModuleInstallSpec, PRODUCTION_MIN_MIGRATOR, SERVER_MIGRATOR, WAVE_2S1_AUDIT_RELS,
    lots_release_is_required, manifest_machines_owned_by_register, migrate_slice_modules,
    migrate_wave_2s1_modules, slice_migrators, spec_by_id, wave_2s1_migrators, wave_2s1_order,
    wave_2s1_specs,
};
pub use kernel::{
    Kernel, KernelBuilder, ModuleJob, ModuleRoute, bind_signature_gate, edges_from_registry,
    module_nodes, posting_sink, startup_fails_if_required_meets_no_signatures,
};
pub use manifest::{
    ManifestJob, ManifestMachine, ManifestMachineEdge, ManifestRoute, ManifestSubscription,
    ModuleManifest, compiled_in, compiled_in_graph,
};
pub use order::{
    CONTRACT_KERNEL_EDGES, KERNEL_AUDIT_RELS, KERNEL_ORDER, MIGRATE_PREFIX, ModuleNode,
    SLICE_AUDIT_RELS, attach_kernel_audit, attach_slice_audit, install_kernel, install_slice,
    is_topological_sort, kernel_crates, kernel_migrators, migrate_prefix, migrate_suffix,
    run_migrations, topological_order,
};
pub use profile::{
    DELTA_ALLOWED, GateBinding, Profile, ProfileId, ProfileModule, SignatureEdge, delta_keys,
    profile_does_not_rewrite_edges,
};
pub use registry::{
    InstalledRow, disable, enable, install, list_installed, profile_permits_enable, uninstall,
    upgrade,
};
pub use semver::{Range, Version};

/// Configuration-manifest functions (`docs/03` §8).
pub mod manifest_export {
    pub use crate::config::{ConfigurationManifest, export, verify};
}

/// Embedded migrator (`placeholder` + `0001_module`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[cfg(test)]
mod _wave_2s1_dev_deps {
    use datum_mod_items as _;
    use datum_mod_locations as _;
    use datum_mod_lots as _;
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(MIGRATOR.migrations.len() >= 2);
        assert!(!kernel_migrators().is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn kernel_order_names_match_crates() {
        let crates = kernel_crates();
        assert_eq!(crates.len(), KERNEL_ORDER.len());
        for (name, listed) in crates.iter().zip(KERNEL_ORDER) {
            assert_eq!(name.0, *listed);
        }
    }

    #[test]
    fn slice_audit_rels_are_disjoint_from_kernel() {
        for rel in SLICE_AUDIT_RELS {
            assert!(
                !KERNEL_AUDIT_RELS.contains(rel),
                "{rel} belongs in SLICE_AUDIT_RELS only"
            );
        }
        assert!(!SLICE_AUDIT_RELS.is_empty());
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
