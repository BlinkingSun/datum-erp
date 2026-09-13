//! Manifest export for `wicket-module` (machine, permissions, event schemas).

use crate::domain::{EVENT_SCHEMAS, EventSchemaDecl, PERMISSIONS};
use crate::error::Result;
use crate::machine::document_machine;

/// Kernel capability declaration consumed by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentsManifest {
    /// `documents.view|edit|approve|release`.
    pub permissions: &'static [&'static str],
    /// `documents.revision_created`, `documents.effective`.
    pub events: &'static [EventSchemaDecl],
    /// Installation profile the machine was built for.
    pub profile: String,
}

/// Manifest for `profile` (`plain-shop` or `regulated-device`).
pub fn manifest(profile: &str) -> Result<DocumentsManifest> {
    let _machine = document_machine(profile)?;
    Ok(DocumentsManifest {
        permissions: PERMISSIONS,
        events: EVENT_SCHEMAS,
        profile: profile.to_owned(),
    })
}
