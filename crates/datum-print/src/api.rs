//! Public API on the sealed [`datum_db::Tx`].

use datum_core::{Identifier, RecordRef};
use datum_db::Tx;
use datum_documents::{BlobHash, BlobStore};
use datum_esign::Manifestation;

use crate::domain::{Format, RenderLogRow, Rendered, TemplateId};
use crate::error::{Error, Result};
use crate::reads;
use crate::render as render_util;
use crate::store;

/// Load manifestation blocks for a record (esign snapshot columns, no identity join).
pub async fn manifestation_block(tx: &mut Tx<'_>, record: RecordRef) -> Result<Vec<Manifestation>> {
    reads::manifestations_for_record(tx, &record).await
}

/// Deterministic render for a record version.
pub async fn render(
    tx: &mut Tx<'_>,
    record: RecordRef,
    format: Format,
    template: TemplateId,
) -> Result<Rendered> {
    let regulated = render_util::regulated_from_tx(tx).await?;
    let tpl = store::latest_template(tx, &template.0)
        .await?
        .ok_or_else(|| Error::UnknownTemplate(template.0.clone()))?;
    let view = render_util::resolve_record(tx, &record, &template).await?;
    let manifestations = manifestation_block(tx, record.clone()).await?;
    let app_version = tx.setting("datum.app_version").await?;
    let config_version = tx.setting("datum.config_version").await?;
    let footer = render_util::footer_stamp(
        &tpl.semantic_version,
        tpl.version,
        &render_util::renderer_version(),
        &app_version,
        &config_version,
    );
    let html = render_util::build_html(&tpl.body, &view, &manifestations, &footer, regulated);
    let bytes = match format {
        Format::Html => html.into_bytes(),
        Format::Pdf => render_util::html_to_pdf(&html)?,
    };
    let output_hash = store::digest(&bytes);
    let renderer_version = render_util::renderer_version();
    let render_id = Identifier::generate();
    store::insert_render_log(
        tx,
        &store::RenderLogInsert {
            render_id,
            record_table: record.table.clone(),
            record_id: record.id,
            record_version: record.version,
            record_content_hash: view.content_hash,
            template_id: template.0.clone(),
            template_version: tpl.version,
            renderer_version: renderer_version.clone(),
            output_format: format.as_str().to_owned(),
            output_hash,
            blob_hash: None,
        },
    )
    .await?;
    Ok(Rendered {
        bytes,
        output_hash,
        template_version: tpl.version,
        renderer_version,
    })
}

/// Archive a rendition to the content-addressed blob store (immutable).
///
/// The caller supplies the [`BlobStore`] (same pattern as [`datum_documents::attach`]).
/// Tests pass a unique [`datum_documents::FsBlobStore`] rooted in a temp directory;
/// production passes `FsBlobStore::from_env()`. There is no process-global blob root.
pub async fn archive(
    tx: &mut Tx<'_>,
    rendered: &Rendered,
    record: RecordRef,
    blobs: &dyn BlobStore,
) -> Result<BlobHash> {
    if let Some(existing) = store::find_archived_blob(
        tx,
        &record.table,
        record.id,
        record.version,
        &rendered.output_hash,
    )
    .await?
    {
        return Ok(existing);
    }
    let hash = blobs.put(&rendered.bytes)?;
    if let Some(render_id) = store::latest_render_id(
        tx,
        &record.table,
        record.id,
        record.version,
        &rendered.output_hash,
    )
    .await?
    {
        store::set_render_blob(tx, render_id, hash).await?;
    }
    Ok(hash)
}

/// Render log rows for a record version.
pub async fn log(tx: &mut Tx<'_>, record: RecordRef) -> Result<Vec<RenderLogRow>> {
    store::list_render_log(tx, &record.table, record.id, record.version).await
}

/// Seed built-in templates (idempotent per version). Tests and first boot call this once.
pub async fn seed_templates(tx: &mut Tx<'_>) -> Result<()> {
    seed_one(
        tx,
        TemplateId::DOCUMENT_REVISION,
        "1.0.0",
        1,
        include_str!("../templates/document_revision.html"),
    )
    .await?;
    seed_one(
        tx,
        TemplateId::GENERIC_RECORD,
        "1.0.0",
        1,
        include_str!("../templates/generic_record.html"),
    )
    .await?;
    seed_one(
        tx,
        TemplateId::WORK_ORDER_TRAVELER,
        "1.0.0",
        1,
        include_str!("../templates/work_order_traveler.html"),
    )
    .await?;
    Ok(())
}

/// Stamp the installation profile id used for 11.50(b) UNSIGNED / signature-block gating.
///
/// `datum.config_version` is the profile *spec* version and must not be used as the
/// profile id. The composition root calls this at boot with `profile.id`.
pub async fn set_installation_profile(tx: &mut Tx<'_>, profile_id: &str) -> Result<()> {
    store::set_install_profile(tx, profile_id).await
}

async fn seed_one(tx: &mut Tx<'_>, id: &str, semver: &str, version: i32, body: &str) -> Result<()> {
    let exists: Option<(i32,)> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT version FROM print.template WHERE template_id = $1 AND version = $2",
            )
            .bind(id)
            .bind(version),
        )
        .await?;
    if exists.is_some() {
        return Ok(());
    }
    let hash = store::digest(body.as_bytes());
    store::insert_template_version(tx, id, version, semver, body, &hash).await?;
    Ok(())
}

/// Insert a new template version (bumps content hash); used by tests and template overrides later.
pub async fn bump_template(
    tx: &mut Tx<'_>,
    template_id: &str,
    version: i32,
    semantic_version: &str,
    body: &str,
) -> Result<()> {
    let hash = store::digest(body.as_bytes());
    store::insert_template_version(tx, template_id, version, semantic_version, body, &hash).await?;
    Ok(())
}
