//! Published render reads for `documents.revision` / `documents.attachment`.
//!
//! `datum-print` calls these instead of selecting `documents.*` itself (R-2s-3).
//! Offered on the sealed [`Tx`] and on [`ReadPool`]. Direct SELECT of this
//! crate's tables (invoker-rights; no SECURITY DEFINER).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::query_as;
use uuid::Uuid;

use datum_core::Identifier;
use datum_db::{ReadPool, Tx};
use datum_statemachine::{DocRef, current_state, current_state_on};

use crate::domain::{AttachmentId, BlobHash, DOC_TYPE, DocumentId, RevisionId, Status};
use crate::error::{Error, Result};

/// Snapshot a renderer needs for one revision.
///
/// Live document status is the machine (`current_state`). Effectivity is the
/// half-open `[effective_from, effective_until)` stored on the revision.
/// `content_hash` is SHA-256 of the compact JSON of [`Self::content_manifest`]
/// (the revision has no `documents.blob` row of its own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionForRender {
    /// Revision id.
    pub revision_id: RevisionId,
    /// Parent document.
    pub document_id: DocumentId,
    /// Gap-free document number.
    pub number: String,
    /// Document title.
    pub title: String,
    /// Human revision label (`A`, `B`, `1`).
    pub label: String,
    /// Live document status (machine overlay; column is the insert-time snapshot).
    pub status: Status,
    /// Inclusive start of effectivity.
    pub effective_from: Option<DateTime<Utc>>,
    /// Exclusive end of effectivity (`None` = open).
    pub effective_until: Option<DateTime<Utc>>,
    /// Opaque content list stored on the revision.
    pub content_manifest: Value,
    /// SHA-256 of the compact JSON of [`Self::content_manifest`].
    pub content_hash: [u8; 32],
}

/// Attachment row a renderer lists: filename, media type, blob hash (bytes handle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentForRender {
    /// Attachment id.
    pub id: AttachmentId,
    /// Original filename.
    pub filename: String,
    /// Media type (`application/pdf`, …).
    pub media_type: String,
    /// Declared byte size.
    pub byte_size: i64,
    /// Content-addressed blob primary key. Bytes live in [`crate::BlobStore`].
    pub blob_hash: BlobHash,
}

type RevRenderRow = (
    Uuid,
    String,
    String,
    String,
    Uuid,
    String,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Value,
);

const REVISION_FOR_RENDER_SQL: &str = r#"SELECT d.document_id, d.number, d.title, d.status,
              r.revision_id, r.label, r.effective_from, r.effective_until,
              r.content_manifest
         FROM documents.revision r
         JOIN documents.document d ON d.document_id = r.document_id
        WHERE r.revision_id = $1"#;

const ATTACHMENTS_FOR_RENDER_SQL: &str = r#"SELECT attachment_id, filename, media_type, blob_hash, byte_size
         FROM documents.attachment
        WHERE revision_id = $1
        ORDER BY filename ASC, attachment_id ASC"#;

const REVISION_EXISTS_SQL: &str = "SELECT true FROM documents.revision WHERE revision_id = $1";

/// Load the render snapshot for `revision_id` on the sealed [`Tx`].
pub async fn revision_for_render(
    tx: &mut Tx<'_>,
    revision_id: RevisionId,
) -> Result<RevisionForRender> {
    let row: Option<RevRenderRow> = tx
        .fetch_optional(query_as(REVISION_FOR_RENDER_SQL).bind(revision_id.as_uuid()))
        .await?;
    let Some(row) = row else {
        return Err(Error::NotFound);
    };
    let live = current_state(
        tx,
        &DocRef {
            doc_type: DOC_TYPE.into(),
            doc_id: Identifier::from_uuid(row.0),
        },
    )
    .await?;
    row_to_revision(row, live.map(|s| s.0))
}

/// [`revision_for_render`] through a [`ReadPool`] (no actor bound on the connection).
pub async fn revision_for_render_on(
    pool: &ReadPool,
    revision_id: RevisionId,
) -> Result<RevisionForRender> {
    let row: Option<RevRenderRow> = pool
        .fetch_optional(query_as(REVISION_FOR_RENDER_SQL).bind(revision_id.as_uuid()))
        .await?;
    let Some(row) = row else {
        return Err(Error::NotFound);
    };
    let live = current_state_on(
        pool,
        &DocRef {
            doc_type: DOC_TYPE.into(),
            doc_id: Identifier::from_uuid(row.0),
        },
    )
    .await?;
    row_to_revision(row, live.map(|s| s.0))
}

/// Attachments of `revision_id`, ordered by filename then id.
///
/// Missing revision is [`Error::NotFound`]. A revision with no attachments
/// returns an empty vec.
pub async fn attachments_for_render(
    tx: &mut Tx<'_>,
    revision_id: RevisionId,
) -> Result<Vec<AttachmentForRender>> {
    let exists: Option<(bool,)> = tx
        .fetch_optional(query_as(REVISION_EXISTS_SQL).bind(revision_id.as_uuid()))
        .await?;
    if exists.is_none() {
        return Err(Error::NotFound);
    }
    let rows: Vec<(Uuid, String, String, Vec<u8>, i64)> = tx
        .fetch_all(query_as(ATTACHMENTS_FOR_RENDER_SQL).bind(revision_id.as_uuid()))
        .await?;
    rows.into_iter().map(row_to_attachment).collect()
}

/// [`attachments_for_render`] through a [`ReadPool`].
pub async fn attachments_for_render_on(
    pool: &ReadPool,
    revision_id: RevisionId,
) -> Result<Vec<AttachmentForRender>> {
    let exists: Option<(bool,)> = pool
        .fetch_optional(query_as(REVISION_EXISTS_SQL).bind(revision_id.as_uuid()))
        .await?;
    if exists.is_none() {
        return Err(Error::NotFound);
    }
    let rows: Vec<(Uuid, String, String, Vec<u8>, i64)> = pool
        .fetch_all(query_as(ATTACHMENTS_FOR_RENDER_SQL).bind(revision_id.as_uuid()))
        .await?;
    rows.into_iter().map(row_to_attachment).collect()
}

fn row_to_revision(row: RevRenderRow, live_state: Option<String>) -> Result<RevisionForRender> {
    let snapshot = parse_status(&row.3)?;
    let status = match live_state {
        Some(state) => parse_status(&state)?,
        None => snapshot,
    };
    let content_hash = hash_manifest(&row.8)?;
    Ok(RevisionForRender {
        document_id: DocumentId(Identifier::from_uuid(row.0)),
        number: row.1,
        title: row.2,
        revision_id: RevisionId(Identifier::from_uuid(row.4)),
        label: row.5,
        status,
        effective_from: row.6,
        effective_until: row.7,
        content_manifest: row.8,
        content_hash,
    })
}

fn row_to_attachment(row: (Uuid, String, String, Vec<u8>, i64)) -> Result<AttachmentForRender> {
    let blob_hash = BlobHash::from_slice(&row.3).ok_or_else(|| {
        Error::Core(datum_core::Error::Invariant(format!(
            "attachment {} blob hash is not 32 bytes",
            row.0
        )))
    })?;
    Ok(AttachmentForRender {
        id: AttachmentId(Identifier::from_uuid(row.0)),
        filename: row.1,
        media_type: row.2,
        blob_hash,
        byte_size: row.4,
    })
}

fn parse_status(s: &str) -> Result<Status> {
    Status::parse(s).ok_or_else(|| {
        Error::Core(datum_core::Error::Invariant(format!(
            "bad document status {s}"
        )))
    })
}

fn hash_manifest(content: &Value) -> Result<[u8; 32]> {
    let body = serde_json::to_vec(content)
        .map_err(|e| Error::Core(datum_core::Error::Invariant(e.to_string())))?;
    Ok(datum_audit::sha256::digest(&body))
}
