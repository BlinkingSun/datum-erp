//! Controlled documents: masters, reconstructible revisions, content-addressed
//! blobs, and an approval state machine registered with the kernel.
//!
//! Writes go through [`datum_db::Tx`] only (CONTRACT §5a). Schema `documents`
//! (class `app`). Blob bytes live in [`FsBlobStore`]; the database holds the hash.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use datum_identity as _;

mod api;
mod blob;
mod domain;
mod error;
mod machine;
mod manifest;
mod store;

pub use api::{
    attach, create, effective_at, history, link, load, new_revision, set_legal_hold, transition,
    transition_context,
};
pub use blob::{BlobStore, FsBlobStore, verify_blob};
pub use domain::{
    AttachmentId, BlobHash, DOC_TYPE, DatePrecision, Document, DocumentId, EVENT_EFFECTIVE,
    EVENT_REVISION_CREATED, EVENT_SCHEMAS, EventSchemaDecl, LinkId, Manifest, PERMISSIONS,
    Revision, RevisionId, Status,
};
pub use error::{Error, Result};
pub use machine::document_machine;
pub use manifest::{DocumentsManifest, manifest};

/// Embedded migrator (`placeholder` + `0001_documents`).
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
    fn migrator_has_documents_migration() {
        assert!(MIGRATOR.migrations.len() >= 2);
        assert!(MIGRATOR.iter().any(|m| m.version == 1));
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn machine_is_total_for_both_profiles() {
        for profile in ["plain-shop", "regulated-device"] {
            let m = document_machine(profile).expect("machine");
            assert_eq!(m.doc_type, DOC_TYPE);
            let names: Vec<&str> = m.edges.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"approve"));
            assert!(names.contains(&"make_effective"));
            let approve = m.edges.iter().find(|e| e.name == "approve").unwrap();
            match profile {
                "regulated-device" => {
                    assert!(m.regulated);
                    assert!(matches!(
                        approve.signature,
                        datum_statemachine::SignatureDeclaration::Required(_)
                    ));
                }
                "plain-shop" => {
                    assert!(!m.regulated);
                    assert!(matches!(
                        approve.signature,
                        datum_statemachine::SignatureDeclaration::NotRequired { .. }
                    ));
                }
                _ => unreachable!(),
            }
        }
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
