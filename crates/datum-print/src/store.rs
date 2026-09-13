//! SQL for schema `print` only.

use sqlx::{query, query_as};
use uuid::Uuid;

use datum_core::Identifier;
use datum_db::Tx;
use datum_documents::BlobHash;

use crate::domain::RenderLogRow;
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub(crate) struct TemplateRow {
    pub version: i32,
    pub body: String,
    pub semantic_version: String,
}

pub(crate) async fn latest_template(
    tx: &mut Tx<'_>,
    template_id: &str,
) -> Result<Option<TemplateRow>> {
    let row: Option<(i32, String, String)> = tx
        .fetch_optional(
            query_as(
                r#"SELECT version, body, semantic_version
                 FROM print.template
                 WHERE template_id = $1
                   AND (effective_until IS NULL OR effective_until > pg_catalog.now())
                 ORDER BY version DESC
                 LIMIT 1"#,
            )
            .bind(template_id),
        )
        .await?;
    Ok(row.map(|(version, body, semantic_version)| TemplateRow {
        version,
        body,
        semantic_version,
    }))
}

pub(crate) async fn insert_template_version(
    tx: &mut Tx<'_>,
    template_id: &str,
    version: i32,
    semantic_version: &str,
    body: &str,
    body_hash: &[u8; 32],
) -> Result<()> {
    tx.execute(
        query(
            r#"INSERT INTO print.template
               (template_id, version, semantic_version, body, body_hash)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(template_id)
        .bind(version)
        .bind(semantic_version)
        .bind(body)
        .bind(body_hash.as_slice()),
    )
    .await?;
    Ok(())
}

pub(crate) async fn set_install_profile(tx: &mut Tx<'_>, profile_id: &str) -> Result<()> {
    require_known_profile(profile_id)?;
    tx.execute(
        query(
            r#"INSERT INTO print.install (singleton, profile_id)
               VALUES ('x', $1)
               ON CONFLICT (singleton) DO UPDATE SET profile_id = EXCLUDED.profile_id"#,
        )
        .bind(profile_id),
    )
    .await?;
    Ok(())
}

pub(crate) async fn install_profile(tx: &mut Tx<'_>) -> Result<String> {
    let row: Option<(String,)> = tx
        .fetch_optional(query_as(
            "SELECT profile_id FROM print.install WHERE singleton = 'x'",
        ))
        .await?;
    Ok(row.map(|r| r.0).unwrap_or_else(|| "plain-shop".into()))
}

#[derive(Debug, Clone)]
pub(crate) struct RenderLogInsert {
    pub render_id: Identifier,
    pub record_table: String,
    pub record_id: Identifier,
    pub record_version: i64,
    pub record_content_hash: [u8; 32],
    pub template_id: String,
    pub template_version: i32,
    pub renderer_version: String,
    pub output_format: String,
    pub output_hash: [u8; 32],
    pub blob_hash: Option<BlobHash>,
}

pub(crate) async fn insert_render_log(tx: &mut Tx<'_>, row: &RenderLogInsert) -> Result<()> {
    tx.execute(
        query(
            r#"INSERT INTO print.render_log
               (render_id, record_table, record_id, record_version, record_content_hash,
                template_id, template_version, renderer_version, output_format, output_hash,
                blob_hash)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        )
        .bind(row.render_id.as_uuid())
        .bind(&row.record_table)
        .bind(row.record_id.as_uuid())
        .bind(row.record_version)
        .bind(row.record_content_hash.as_slice())
        .bind(&row.template_id)
        .bind(row.template_version)
        .bind(&row.renderer_version)
        .bind(&row.output_format)
        .bind(row.output_hash.as_slice())
        .bind(row.blob_hash.map(|h| h.0.to_vec())),
    )
    .await?;
    Ok(())
}

pub(crate) async fn find_archived_blob(
    tx: &mut Tx<'_>,
    record_table: &str,
    record_id: Identifier,
    record_version: i64,
    output_hash: &[u8; 32],
) -> Result<Option<BlobHash>> {
    let row: Option<(Vec<u8>,)> = tx
        .fetch_optional(
            query_as(
                r#"SELECT blob_hash FROM print.render_log
                 WHERE record_table = $1 AND record_id = $2 AND record_version = $3
                   AND output_hash = $4 AND blob_hash IS NOT NULL
                 ORDER BY created_at ASC
                 LIMIT 1"#,
            )
            .bind(record_table)
            .bind(record_id.as_uuid())
            .bind(record_version)
            .bind(output_hash.as_slice()),
        )
        .await?;
    Ok(row.and_then(|(bytes,)| {
        if bytes.len() == 32 {
            let mut h = [0u8; 32];
            h.copy_from_slice(&bytes);
            Some(BlobHash(h))
        } else {
            None
        }
    }))
}

pub(crate) async fn set_render_blob(
    tx: &mut Tx<'_>,
    render_id: Identifier,
    blob_hash: BlobHash,
) -> Result<()> {
    tx.execute(
        query(r#"UPDATE print.render_log SET blob_hash = $2 WHERE render_id = $1"#)
            .bind(render_id.as_uuid())
            .bind(blob_hash.0.as_slice()),
    )
    .await?;
    Ok(())
}

pub(crate) async fn latest_render_id(
    tx: &mut Tx<'_>,
    record_table: &str,
    record_id: Identifier,
    record_version: i64,
    output_hash: &[u8; 32],
) -> Result<Option<Identifier>> {
    let row: Option<(Uuid,)> = tx
        .fetch_optional(
            query_as(
                r#"SELECT render_id FROM print.render_log
                 WHERE record_table = $1 AND record_id = $2 AND record_version = $3
                   AND output_hash = $4
                 ORDER BY created_at DESC
                 LIMIT 1"#,
            )
            .bind(record_table)
            .bind(record_id.as_uuid())
            .bind(record_version)
            .bind(output_hash.as_slice()),
        )
        .await?;
    Ok(row.map(|(id,)| Identifier::from_uuid(id)))
}

type RenderLogDbRow = (
    Uuid,
    String,
    Uuid,
    i64,
    Vec<u8>,
    String,
    i32,
    String,
    String,
    Vec<u8>,
    Option<Vec<u8>>,
);

pub(crate) async fn list_render_log(
    tx: &mut Tx<'_>,
    record_table: &str,
    record_id: Identifier,
    record_version: i64,
) -> Result<Vec<RenderLogRow>> {
    let rows: Vec<RenderLogDbRow> = tx
        .fetch_all(
            query_as(
                r#"SELECT render_id, record_table, record_id, record_version,
                          record_content_hash, template_id, template_version,
                          renderer_version, output_format, output_hash, blob_hash
                   FROM print.render_log
                   WHERE record_table = $1 AND record_id = $2 AND record_version = $3
                   ORDER BY created_at ASC, render_id ASC"#,
            )
            .bind(record_table)
            .bind(record_id.as_uuid())
            .bind(record_version),
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut record_content_hash = [0u8; 32];
        if row.4.len() == 32 {
            record_content_hash.copy_from_slice(&row.4);
        }
        let mut output_hash = [0u8; 32];
        if row.9.len() == 32 {
            output_hash.copy_from_slice(&row.9);
        }
        let blob_hash = row.10.and_then(|b| {
            if b.len() == 32 {
                let mut h = [0u8; 32];
                h.copy_from_slice(&b);
                Some(BlobHash(h))
            } else {
                None
            }
        });
        out.push(RenderLogRow {
            render_id: Identifier::from_uuid(row.0),
            record_table: row.1,
            record_id: Identifier::from_uuid(row.2),
            record_version: row.3,
            record_content_hash,
            template_id: row.5,
            template_version: row.6,
            renderer_version: row.7,
            output_format: row.8,
            output_hash,
            blob_hash,
        });
    }
    Ok(out)
}

pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
    datum_audit::sha256::digest(bytes)
}

/// Latest effective version per `template_id` (no body). Ordered by id.
pub(crate) const LIST_TEMPLATES_SQL: &str = r#"
SELECT template_id, version, semantic_version, body_hash
  FROM (
    SELECT DISTINCT ON (template_id)
           template_id, version, semantic_version, body_hash
      FROM print.template
     WHERE effective_until IS NULL OR effective_until > pg_catalog.now()
     ORDER BY template_id ASC, version DESC
  ) latest
 ORDER BY template_id ASC
"#;

pub(crate) type TemplateListRow = (String, i32, String, Vec<u8>);

pub(crate) fn row_to_summary(row: TemplateListRow) -> crate::domain::TemplateSummary {
    crate::domain::TemplateSummary {
        template_id: row.0,
        version: row.1,
        semantic_version: row.2,
        body_hash: datum_audit::sha256::hex(&row.3),
    }
}

pub(crate) fn require_known_profile(profile: &str) -> Result<()> {
    if profile != "plain-shop" && profile != "regulated-device" {
        return Err(Error::UnknownProfile(profile.to_owned()));
    }
    Ok(())
}
