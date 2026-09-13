//! Electronic signatures: mint a row bound to a record version, claim it inside
//! the transition's transaction through a prepared [`datum_core::SignatureGate`].
//!
//! Composition-root wiring (`GateBinding` factory at startup, `prepare` before
//! `Engine::transition`, `datum.esign_id` on audit rows, profile TOML flip) is
//! the follow-up lane `2b1-glue`. This crate publishes what that lane needs.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod error;
mod gate;
mod hash;
mod mint;
mod projection;
mod read;
mod session;

pub use error::{Error, Result};
pub use gate::{BoundGate, GateFactory, LiveDoc, PreparedGate, prepare, supersede};
pub use mint::{MintRequest, Signature, log_refusal, mint};
pub use projection::{identity_projection, project, register_projection};
pub use read::{
    AnchorRef, ArchivalBundle, BundleVerification, ManifestRecord, Manifestation, SealRef,
    SignatureManifest, archival_bundle, manifestation, verify_bundle,
};
pub use session::{Challenge, SessionPolicy, SigningSession, challenge, close_session};

use datum_core::Identifier;
use serde::{Deserialize, Serialize};

/// Embedded migrator (`placeholder` + `0001_esign`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The `sm.instance` triple hashed with the business projection (D-2b-3).
///
/// Read by the caller (statemachine owns `sm`); this crate does not `SELECT sm.*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceTriple {
    /// Document type.
    pub doc_type: String,
    /// Document id.
    pub doc_id: Identifier,
    /// Current state.
    pub state: String,
    /// Instance version.
    pub version: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_statemachine as _;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(MIGRATOR.migrations.len() >= 2);
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn identity_projection_is_default() {
        let v = serde_json::json!({"a": 1});
        assert_eq!(identity_projection(&v), v);
        assert_eq!(project("unknown.doc", &v), v);
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
