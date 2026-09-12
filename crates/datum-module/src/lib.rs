//! Composition root: module registry, profiles, and kernel wiring.
//!
//! Wires [`datum_core::PostingSink`] ([`datum_ledger::GroupBuilder`]) and
//! [`datum_core::SignatureGate`] (`NoSignatures` until `datum-esign`).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use datum_customfields as _;
use datum_documents as _;
use datum_esign as _;
use datum_print as _;
use datum_uom as _;

mod config;
mod error;
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
pub use kernel::{
    Kernel, bind_signature_gate, edges_from_registry, module_nodes, posting_sink,
    startup_fails_if_required_meets_no_signatures,
};
pub use manifest::{ModuleManifest, compiled_in, compiled_in_graph};
pub use order::{
    CONTRACT_KERNEL_EDGES, KERNEL_ORDER, MIGRATE_PREFIX, ModuleNode, attach_kernel_audit,
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

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
