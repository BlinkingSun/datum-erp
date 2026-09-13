//! Cross-crate snapshot reads through this lane's Tx.
//!
//! Parent `datum-esign` / `datum-documents` do not publish list-by-record or
//! revision-by-id / attachments-by-revision. This lane does not extend those
//! crates; it reads snapshot columns only (no live identity join).

use sqlx::query_as;
use uuid::Uuid;

use datum_core::{Identifier, RecordRef};
use datum_db::Tx;
use datum_documents::{DocumentId, RevisionId, load};
use datum_esign::{ManifestRecord, Manifestation, SignatureManifest};

use crate::error::{Error, Result};
use crate::store;

type ManifestRow = (
    Uuid,
    Uuid,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    Uuid,
    i64,
    String,
    Vec<u8>,
    String,
    Vec<String>,
    bool,
);

/// Snapshot columns matching `datum_esign::manifestation` (D-2b-2), oldest first.
pub(crate) async fn manifestations_for_record(
    tx: &mut Tx<'_>,
    record: &RecordRef,
) -> Result<Vec<Manifestation>> {
    let rows: Vec<ManifestRow> = tx
        .fetch_all(
            query_as(
                r#"SELECT
               signature_id, signer_id, signer_printed_name, meaning, reason,
               to_char(signed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
               signed_at_zone,
               (
                 to_char(timezone(signed_at_zone, signed_at), 'YYYY-MM-DD"T"HH24:MI:SS')
                 || CASE
                      WHEN timezone(signed_at_zone, signed_at)
                           >= timezone('UTC', signed_at)
                      THEN '+' ELSE '-'
                    END
                 || to_char(
                      (abs(extract(epoch from (
                         timezone(signed_at_zone, signed_at)
                         - timezone('UTC', signed_at)
                       )))::int / 3600),
                      'FM00'
                    )
                 || ':'
                 || to_char(
                      ((abs(extract(epoch from (
                         timezone(signed_at_zone, signed_at)
                         - timezone('UTC', signed_at)
                       )))::int % 3600) / 60),
                      'FM00'
                    )
               ) AS signed_at_local,
               record_table, record_id, record_version, doc_type,
               record_content_hash, credential_kind, components_used,
               EXISTS (
                 SELECT 1 FROM esign.supersession x
                  WHERE x.old_signature_id = esign.signature.signature_id
               ) AS superseded
          FROM esign.signature
         WHERE record_table = $1 AND record_id = $2 AND record_version = $3
         ORDER BY signed_at ASC, signature_id ASC"#,
            )
            .bind(&record.table)
            .bind(record.id.as_uuid())
            .bind(record.version),
        )
        .await?;
    Ok(rows.into_iter().map(row_to_manifestation).collect())
}

fn row_to_manifestation(row: ManifestRow) -> Manifestation {
    let mut hash = [0u8; 32];
    if row.12.len() == 32 {
        hash.copy_from_slice(&row.12);
    }
    Manifestation {
        signature: SignatureManifest {
            id: row.0.to_string(),
            signer_id: row.1.to_string(),
            printed_name: row.2,
            meaning: row.3,
            reason: row.4,
            signed_at: row.5,
            signed_at_zone: row.6,
            signed_at_local: row.7,
            record: ManifestRecord {
                table: row.8,
                doc_type: row.11,
                id: row.9.to_string(),
                version: row.10,
            },
            record_content_hash: datum_audit::sha256::hex(&hash),
            credential_kind: row.13,
            components_used: row.14,
            superseded: row.15,
        },
    }
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
        let revs = datum_documents::history(tx, doc_id).await?;
        revs.last().map(|r| r.id).ok_or(Error::NotFound)?
    } else {
        return Err(Error::NotFound);
    };

    let header: Option<(Uuid, String, serde_json::Value, Option<String>)> = tx
        .fetch_optional(
            query_as(
                r#"SELECT document_id, label, content_manifest,
                          CASE WHEN effective_from IS NULL THEN ''
                               ELSE to_char(effective_from AT TIME ZONE 'UTC',
                                            'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                          END
                   FROM documents.revision
                   WHERE revision_id = $1"#,
            )
            .bind(rev_id.as_uuid()),
        )
        .await?;
    let Some((document_id, label, content, effectivity)) = header else {
        return Err(Error::NotFound);
    };
    let effectivity = effectivity.unwrap_or_default();
    let doc = load(tx, DocumentId(Identifier::from_uuid(document_id))).await?;
    let names: Vec<(String,)> = tx
        .fetch_all(
            query_as(
                r#"SELECT filename
                   FROM documents.attachment
                   WHERE revision_id = $1
                   ORDER BY filename ASC, attachment_id ASC"#,
            )
            .bind(rev_id.as_uuid()),
        )
        .await?;
    let attachments = if names.is_empty() {
        "(none)".to_owned()
    } else {
        names
            .into_iter()
            .map(|n| n.0)
            .collect::<Vec<_>>()
            .join("; ")
    };
    let body = serde_json::to_string(&content)
        .map_err(|e| Error::Core(datum_core::Error::Invariant(e.to_string())))?;
    Ok(DocumentRevisionView {
        title: doc.title,
        number: doc.number,
        label,
        effectivity,
        attachments,
        content_hash: store::digest(body.as_bytes()),
    })
}
