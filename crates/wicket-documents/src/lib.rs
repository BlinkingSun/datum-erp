//! Controlled documents: masters, reconstructible revisions, content-addressed
//! blobs, and an approval state machine registered with the kernel.
//!
//! Writes go through [`wicket_db::Tx`] only (CONTRACT §5a). Schema `documents`
//! (class `app`). Blob bytes live in [`FsBlobStore`]; the database holds the hash.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use wicket_identity as _;

mod api;
mod blob;
mod domain;
mod error;
mod machine;
mod manifest;
mod read;
mod store;

pub use api::{
    attach, create, discard_unreferenced_blob, effective_at, history, link, load, new_revision,
    set_legal_hold, transition, transition_context,
};
pub use blob::{BlobStore, FsBlobStore, hash_bytes, verify_blob};
pub use domain::{
    AttachmentId, BlobHash, DOC_TYPE, DatePrecision, Document, DocumentId, EVENT_EFFECTIVE,
    EVENT_REVISION_CREATED, EVENT_SCHEMAS, EventSchemaDecl, LinkId, Manifest, PERMISSIONS,
    Revision, RevisionId, Status,
};
pub use error::{Error, Result};
pub use machine::document_machine;
pub use manifest::{DocumentsManifest, manifest};
pub use read::{
    AttachmentForRender, RevisionForRender, attachments_for_render, attachments_for_render_on,
    revision_for_render, revision_for_render_on,
};

/// Embedded migrator (`placeholder` + `0001_documents`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio as _;
    use wicket_module as _;

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
        let _ = wicket_test::postgres_available();
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
            let make_effective = m.edges.iter().find(|e| e.name == "make_effective").unwrap();
            match profile {
                "regulated-device" => {
                    assert!(m.regulated);
                    assert!(matches!(
                        approve.signature,
                        wicket_statemachine::SignatureDeclaration::Required(_)
                    ));
                    assert!(
                        matches!(
                            make_effective.signature,
                            wicket_statemachine::SignatureDeclaration::Required(_)
                        ),
                        "regulated-device make_effective must be Required"
                    );
                }
                "plain-shop" => {
                    assert!(!m.regulated);
                    assert!(matches!(
                        approve.signature,
                        wicket_statemachine::SignatureDeclaration::NotRequired { .. }
                    ));
                    assert!(matches!(
                        make_effective.signature,
                        wicket_statemachine::SignatureDeclaration::NotRequired { .. }
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

    #[test]
    fn ranges_overlap_uses_explicit_unbounded_not_a_sentinel() {
        use chrono::{TimeZone, Utc};
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
        assert!(
            store::ranges_overlap(
                Some(t0),
                None,
                Some(t1),
                Some(t1 + chrono::Duration::days(1))
            ),
            "open until overlaps a later bounded window"
        );
        assert!(
            !store::ranges_overlap(None, None, Some(t0), Some(t1)),
            "both-NULL is not a window"
        );
        assert!(
            !store::ranges_overlap(Some(t0), Some(t1), Some(t1), None),
            "half-open: [t0, t1) does not overlap [t1, ∞)"
        );
        assert!(store::in_force(None, Some(t1), t0));
        assert!(!store::in_force(None, Some(t1), t1));
        assert!(!store::in_force(None, None, t0));
    }
}
