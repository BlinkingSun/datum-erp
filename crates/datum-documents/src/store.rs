//! SQL store (schema `documents` only).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{query, query_as};
use uuid::Uuid;

use datum_core::Identifier;
use datum_db::Tx;

use crate::domain::{
    BlobHash, DatePrecision, Document, DocumentId, Manifest, Revision, RevisionId, Status,
};
use crate::error::{Error, Result};

#[derive(sqlx::FromRow)]
struct DocRow {
    document_id: Uuid,
    kind: String,
    number: String,
    title: String,
    status: String,
    retention_class: String,
    legal_hold: bool,
}

#[derive(sqlx::FromRow)]
struct RevRow {
    revision_id: Uuid,
    document_id: Uuid,
    label: String,
    supersedes_revision_id: Option<Uuid>,
    content_manifest: Value,
    status: String,
    retention_class: String,
    effective_from: Option<DateTime<Utc>>,
    effective_until: Option<DateTime<Utc>>,
    effective_from_precision: Option<String>,
    effective_until_precision: Option<String>,
}

fn row_to_doc(r: DocRow) -> Result<Document> {
    let status = Status::parse(&r.status).ok_or_else(|| {
        Error::Core(datum_core::Error::Invariant(format!(
            "bad document status {}",
            r.status
        )))
    })?;
    Ok(Document {
        id: DocumentId(Identifier::from_uuid(r.document_id)),
        kind: r.kind,
        number: r.number,
        title: r.title,
        status,
        retention_class: r.retention_class,
        legal_hold: r.legal_hold,
    })
}

fn row_to_rev(r: RevRow) -> Result<Revision> {
    let status = Status::parse(&r.status).ok_or_else(|| {
        Error::Core(datum_core::Error::Invariant(format!(
            "bad revision status {}",
            r.status
        )))
    })?;
    let from_precision = r
        .effective_from_precision
        .as_deref()
        .map(|s| {
            DatePrecision::parse(s).ok_or_else(|| {
                Error::Core(datum_core::Error::Invariant(format!("bad precision {s}")))
            })
        })
        .transpose()?;
    let until_precision = r
        .effective_until_precision
        .as_deref()
        .map(|s| {
            DatePrecision::parse(s).ok_or_else(|| {
                Error::Core(datum_core::Error::Invariant(format!("bad precision {s}")))
            })
        })
        .transpose()?;
    Ok(Revision {
        id: RevisionId(Identifier::from_uuid(r.revision_id)),
        document_id: DocumentId(Identifier::from_uuid(r.document_id)),
        label: r.label,
        supersedes: r
            .supersedes_revision_id
            .map(|u| RevisionId(Identifier::from_uuid(u))),
        manifest: Manifest {
            content: r.content_manifest,
            effective_from: r.effective_from,
            effective_until: r.effective_until,
            from_precision,
            until_precision,
        },
        status,
        retention_class: r.retention_class,
    })
}

pub(crate) async fn insert_document(
    tx: &mut Tx<'_>,
    id: DocumentId,
    kind: &str,
    number: &str,
    title: &str,
    retention_class: &str,
) -> Result<()> {
    tx.execute(
        query(
            "INSERT INTO documents.document
                 (document_id, kind, number, title, status, retention_class, legal_hold)
             VALUES ($1, $2, $3, $4, 'Draft', $5, false)",
        )
        .bind(id.as_uuid())
        .bind(kind)
        .bind(number)
        .bind(title)
        .bind(retention_class),
    )
    .await?;
    Ok(())
}

pub(crate) async fn load_document(tx: &mut Tx<'_>, id: DocumentId) -> Result<Document> {
    let row = tx
        .fetch_optional(
            query_as::<_, DocRow>(
                "SELECT document_id, kind, number, title, status, retention_class, legal_hold
                 FROM documents.document WHERE document_id = $1",
            )
            .bind(id.as_uuid()),
        )
        .await?;
    row.map(row_to_doc).transpose()?.ok_or(Error::NotFound)
}

pub(crate) async fn set_document_status(
    tx: &mut Tx<'_>,
    id: DocumentId,
    status: &str,
) -> Result<()> {
    let n = tx
        .execute(
            query("UPDATE documents.document SET status = $2 WHERE document_id = $1")
                .bind(id.as_uuid())
                .bind(status),
        )
        .await?;
    if n.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

pub(crate) async fn set_legal_hold_row(tx: &mut Tx<'_>, id: DocumentId, hold: bool) -> Result<()> {
    let n = tx
        .execute(
            query("UPDATE documents.document SET legal_hold = $2 WHERE document_id = $1")
                .bind(id.as_uuid())
                .bind(hold),
        )
        .await?;
    if n.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

pub(crate) async fn insert_revision(
    tx: &mut Tx<'_>,
    id: RevisionId,
    doc: DocumentId,
    label: &str,
    supersedes: Option<RevisionId>,
    manifest: &Manifest,
    retention_class: &str,
) -> Result<()> {
    let from_prec = manifest.from_precision.map(DatePrecision::as_str);
    let until_prec = manifest.until_precision.map(DatePrecision::as_str);
    tx.execute(
        query(
            "INSERT INTO documents.revision
                 (revision_id, document_id, label, supersedes_revision_id, content_manifest,
                  status, retention_class, effective_from, effective_until,
                  effective_from_precision, effective_until_precision)
             VALUES ($1, $2, $3, $4, $5, 'Draft', $6, $7, $8, $9, $10)",
        )
        .bind(id.as_uuid())
        .bind(doc.as_uuid())
        .bind(label)
        .bind(supersedes.map(RevisionId::as_uuid))
        .bind(&manifest.content)
        .bind(retention_class)
        .bind(manifest.effective_from)
        .bind(manifest.effective_until)
        .bind(from_prec)
        .bind(until_prec),
    )
    .await?;
    Ok(())
}

pub(crate) async fn load_revision(tx: &mut Tx<'_>, id: RevisionId) -> Result<Revision> {
    let row = tx
        .fetch_optional(
            query_as::<_, RevRow>(
                "SELECT revision_id, document_id, label, supersedes_revision_id, content_manifest,
                        status, retention_class, effective_from, effective_until,
                        effective_from_precision, effective_until_precision
                 FROM documents.revision WHERE revision_id = $1",
            )
            .bind(id.as_uuid()),
        )
        .await?;
    row.map(row_to_rev).transpose()?.ok_or(Error::NotFound)
}

pub(crate) async fn list_revisions(tx: &mut Tx<'_>, doc: DocumentId) -> Result<Vec<Revision>> {
    let rows = tx
        .fetch_all(
            query_as::<_, RevRow>(
                "SELECT revision_id, document_id, label, supersedes_revision_id, content_manifest,
                        status, retention_class, effective_from, effective_until,
                        effective_from_precision, effective_until_precision
                 FROM documents.revision
                 WHERE document_id = $1
                 ORDER BY created_at ASC, revision_id ASC",
            )
            .bind(doc.as_uuid()),
        )
        .await?;
    rows.into_iter().map(row_to_rev).collect()
}

pub(crate) async fn latest_revision_id(
    tx: &mut Tx<'_>,
    doc: DocumentId,
) -> Result<Option<RevisionId>> {
    let row: Option<(Uuid,)> = tx
        .fetch_optional(
            query_as(
                "SELECT revision_id FROM documents.revision
                 WHERE document_id = $1
                 ORDER BY created_at DESC, revision_id DESC
                 LIMIT 1",
            )
            .bind(doc.as_uuid()),
        )
        .await?;
    Ok(row.map(|r| RevisionId(Identifier::from_uuid(r.0))))
}

pub(crate) async fn set_revision_status(
    tx: &mut Tx<'_>,
    id: RevisionId,
    status: &str,
) -> Result<()> {
    let n = tx
        .execute(
            query("UPDATE documents.revision SET status = $2 WHERE revision_id = $1")
                .bind(id.as_uuid())
                .bind(status),
        )
        .await?;
    if n.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

pub(crate) async fn insert_blob(tx: &mut Tx<'_>, hash: BlobHash, byte_size: i64) -> Result<()> {
    tx.execute(
        query(
            "INSERT INTO documents.blob (hash, byte_size)
             VALUES ($1, $2)
             ON CONFLICT (hash) DO NOTHING",
        )
        .bind(hash.as_bytes().as_slice())
        .bind(byte_size),
    )
    .await?;
    Ok(())
}

pub(crate) async fn insert_attachment(
    tx: &mut Tx<'_>,
    id: Uuid,
    revision_id: RevisionId,
    hash: BlobHash,
    filename: &str,
    media_type: &str,
    byte_size: i64,
) -> Result<()> {
    tx.execute(
        query(
            "INSERT INTO documents.attachment
                 (attachment_id, revision_id, blob_hash, filename, media_type, byte_size)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(revision_id.as_uuid())
        .bind(hash.as_bytes().as_slice())
        .bind(filename)
        .bind(media_type)
        .bind(byte_size),
    )
    .await?;
    Ok(())
}

pub(crate) async fn insert_link(
    tx: &mut Tx<'_>,
    id: Uuid,
    revision_id: RevisionId,
    entity: &str,
    record_id: Uuid,
    kind: &str,
) -> Result<()> {
    tx.execute(
        query(
            "INSERT INTO documents.link
                 (link_id, revision_id, entity, record_id, kind)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(revision_id.as_uuid())
        .bind(entity)
        .bind(record_id)
        .bind(kind),
    )
    .await?;
    Ok(())
}

pub(crate) fn ranges_overlap(
    a_from: Option<DateTime<Utc>>,
    a_until: Option<DateTime<Utc>>,
    b_from: Option<DateTime<Utc>>,
    b_until: Option<DateTime<Utc>>,
) -> bool {
    let (Some(a0), Some(b0)) = (a_from, b_from) else {
        return false;
    };
    let open =
        DateTime::<Utc>::from_timestamp(4_102_444_800, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let a1 = a_until.unwrap_or(open);
    let b1 = b_until.unwrap_or(open);
    a0 < b1 && b0 < a1
}
