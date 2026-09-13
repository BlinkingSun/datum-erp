//! Cross-crate snapshot reads through published documents / esign APIs.
//!
//! R-2s-3: render snapshots come from [`wicket_documents::revision_for_render`],
//! [`wicket_documents::attachments_for_render`], and
//! [`wicket_esign::manifestation_for_record`] on the sealed Tx. A record whose
//! table is `"documents.document"` still uses [`wicket_documents::history`] to
//! pick the latest revision id.

use wicket_core::{Identifier, RecordRef};
use wicket_db::Tx;
use wicket_documents::{
    DocumentId, RevisionId, attachments_for_render, history, revision_for_render,
};
use wicket_esign::{Manifestation, manifestation_for_record};

use crate::error::{Error, Result};

/// Snapshot columns matching `wicket_esign::manifestation` (D-2b-2), oldest first.
pub(crate) async fn manifestations_for_record(
    tx: &mut Tx<'_>,
    record: &RecordRef,
) -> Result<Vec<Manifestation>> {
    Ok(manifestation_for_record(tx, record).await?)
}

#[derive(Debug, Clone)]
pub(crate) struct DocumentRevisionView {
    pub title: String,
    pub number: String,
    pub label: String,
    pub effectivity: String,
    pub attachments: String,
    pub content_hash: [u8; 32],
}

pub(crate) async fn document_revision_view(
    tx: &mut Tx<'_>,
    record: &RecordRef,
) -> Result<DocumentRevisionView> {
    let rev_id = if record.table == "documents.revision" {
        RevisionId(Identifier::from_uuid(record.id.as_uuid()))
    } else if record.table == "documents.document" {
        let doc_id = DocumentId(Identifier::from_uuid(record.id.as_uuid()));
        let revs = history(tx, doc_id).await?;
        revs.last().map(|r| r.id).ok_or(Error::NotFound)?
    } else {
        return Err(Error::NotFound);
    };

    let rev = revision_for_render(tx, rev_id).await?;
    let names = attachments_for_render(tx, rev_id).await?;
    let attachments = if names.is_empty() {
        "(none)".to_owned()
    } else {
        names
            .into_iter()
            .map(|a| a.filename)
            .collect::<Vec<_>>()
            .join("; ")
    };
    let effectivity = rev
        .effective_from
        .map(|ts| ts.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default();
    Ok(DocumentRevisionView {
        title: rev.title,
        number: rev.number,
        label: rev.label,
        effectivity,
        attachments,
        content_hash: rev.content_hash,
    })
}
