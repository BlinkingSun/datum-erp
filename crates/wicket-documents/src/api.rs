//! Public write/read API on the sealed [`wicket_db::Tx`].

use chrono::{DateTime, Utc};
use wicket_core::{Identifier, NoPostings, PostingSink, SignatureGate, SignatureToken};
use wicket_db::{Tx, WriteContext};
use wicket_numbering::{ResetPolicy, define, next_number};
use wicket_statemachine::{DocRef, Engine, current_state, instance_exists, with_action};

use crate::blob::BlobStore;
use crate::domain::{
    AttachmentId, DOC_TYPE, DatePrecision, Document, DocumentId, LinkId, Manifest, Revision,
    RevisionId, Status,
};
use crate::error::{Error, Result};
use crate::store;

/// Allocate a gap-free number late, insert the master in `Draft`, and spawn the machine.
pub async fn create(
    tx: &mut Tx<'_>,
    engine: &Engine,
    kind: &str,
    title: &str,
    retention_class: &str,
) -> Result<DocumentId> {
    if kind.is_empty()
        || !kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::InvalidKind(kind.to_owned()));
    }
    engine.persist(tx).await?;
    let format = format!("{kind}-{{0000}}");
    let seq = define(tx, kind, format, ResetPolicy::Never).await?;
    let number = next_number(tx, seq).await?;
    let id = DocumentId::generate();
    store::insert_document(tx, id, kind, &number, title, retention_class).await?;
    let doc = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: id.0,
    };
    engine.spawn(tx, &doc, Status::Draft.as_str()).await?;
    if !instance_exists(tx, &doc).await? {
        return Err(Error::Core(wicket_core::Error::Invariant(
            "spawn did not persist a machine instance".into(),
        )));
    }
    Ok(id)
}

/// Insert a revision. Auto-chains `supersedes_revision_id` to the latest row.
/// Effectivity on `manifest` is refused when it overlaps an existing revision.
pub async fn new_revision(
    tx: &mut Tx<'_>,
    doc: DocumentId,
    label: &str,
    manifest: Manifest,
) -> Result<RevisionId> {
    let parent = store::load_document(tx, doc).await?;
    let mut manifest = manifest;
    if manifest.effective_from.is_some() && manifest.from_precision.is_none() {
        manifest.from_precision = Some(DatePrecision::Day);
    }
    if manifest.effective_until.is_some() && manifest.until_precision.is_none() {
        manifest.until_precision = Some(DatePrecision::Day);
    }
    let existing = store::list_revisions(tx, doc).await?;
    for prev in &existing {
        if store::ranges_overlap(
            manifest.effective_from,
            manifest.effective_until,
            prev.manifest.effective_from,
            prev.manifest.effective_until,
        ) {
            return Err(Error::OverlappingEffectivity);
        }
    }
    let supersedes = store::latest_revision_id(tx, doc).await?;
    let id = RevisionId::generate();
    store::insert_revision(
        tx,
        id,
        doc,
        label,
        supersedes,
        &manifest,
        &parent.retention_class,
    )
    .await?;
    Ok(id)
}

/// Store bytes in the blob store and insert an attachment row. Same bytes reuse one blob.
///
/// The `documents.blob` row is inserted before bytes hit disk. If `put` fails after
/// creating a new object, that object is removed. After a transaction rollback the
/// composition root calls [`crate::BlobStore::discard_uncommitted`] so a rolled-back
/// row cannot leave an orphan file.
pub async fn attach(
    tx: &mut Tx<'_>,
    store: &dyn BlobStore,
    rev: RevisionId,
    bytes: &[u8],
    filename: &str,
    media_type: &str,
) -> Result<AttachmentId> {
    let _ = store::load_revision(tx, rev).await?;
    let hash = crate::blob::hash_bytes(bytes);
    let already_on_disk = store.exists(hash);
    store::insert_blob(tx, hash, bytes.len() as i64).await?;
    let id = AttachmentId::generate();
    store::insert_attachment(
        tx,
        id.as_uuid(),
        rev,
        hash,
        filename,
        media_type,
        bytes.len() as i64,
    )
    .await?;
    if let Err(e) = store.put(bytes) {
        if !already_on_disk {
            store.discard_hash(hash);
        }
        return Err(e);
    }
    Ok(id)
}

/// Link a revision to an entity record.
pub async fn link(
    tx: &mut Tx<'_>,
    rev: RevisionId,
    entity: &str,
    record_id: Identifier,
    kind: &str,
) -> Result<LinkId> {
    let _ = store::load_revision(tx, rev).await?;
    let id = LinkId::generate();
    store::insert_link(tx, id.as_uuid(), rev, entity, record_id.as_uuid(), kind).await?;
    Ok(id)
}

/// Drive the registered machine. Live status is [`current_state`] after the
/// engine returns; the `status` column is the insert-time snapshot (immutable).
///
/// `tx` must have been begun with [`with_action`] for `(document, edge)`.
/// `approve` / `make_effective` honour the profile's signature declaration.
pub async fn transition(
    tx: &mut Tx<'_>,
    engine: &Engine,
    gate: &dyn SignatureGate,
    doc: DocumentId,
    edge: &str,
    ctx: &WriteContext,
    signature: Option<&SignatureToken>,
) -> Result<Document> {
    let current = store::load_document(tx, doc).await?;
    if current.legal_hold && (edge == "obsolete" || edge == "supersede") {
        return Err(Error::LegalHold);
    }
    let doc_ref = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: doc.0,
    };
    let sink: Box<dyn PostingSink> = Box::new(NoPostings);
    let instance = engine
        .transition(tx, sink, &doc_ref, edge, signature, gate, ctx)
        .await?;
    let live = current_state(tx, &doc_ref).await?;
    match live {
        Some(state) if state.0 == instance.state.0 => {}
        other => {
            return Err(Error::Core(wicket_core::Error::Invariant(format!(
                "machine state {:?} after transition to {}",
                other.map(|s| s.0),
                instance.state.0
            ))));
        }
    }
    store::load_document(tx, doc).await
}

/// Bind `WriteContext.action` to `"document.<edge>"` for [`transition`].
pub fn transition_context(mut ctx: WriteContext, doc: DocumentId, edge: &str) -> WriteContext {
    let doc_ref = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: doc.0,
    };
    ctx = with_action(ctx, &doc_ref, edge);
    ctx
}

/// Rebuild the version chain from `revision` rows (oldest first).
pub async fn history(tx: &mut Tx<'_>, doc: DocumentId) -> Result<Vec<Revision>> {
    let _ = store::load_document(tx, doc).await?;
    let rows = store::list_revisions(tx, doc).await?;
    Ok(rebuild_chain(&rows))
}

/// Revision whose `[effective_from, effective_until)` contains `ts`.
pub async fn effective_at(
    tx: &mut Tx<'_>,
    doc: DocumentId,
    ts: DateTime<Utc>,
) -> Result<Option<Revision>> {
    let rows = store::list_revisions(tx, doc).await?;
    Ok(rows
        .into_iter()
        .find(|r| store::in_force(r.manifest.effective_from, r.manifest.effective_until, ts)))
}

/// Set or clear legal hold. `true` refuses subsequent Obsolete transitions.
pub async fn set_legal_hold(tx: &mut Tx<'_>, doc: DocumentId, hold: bool) -> Result<()> {
    store::set_legal_hold_row(tx, doc, hold).await
}

/// Load the master. `status` is the live machine state (`current_state`).
pub async fn load(tx: &mut Tx<'_>, id: DocumentId) -> Result<Document> {
    store::load_document(tx, id).await
}

/// Remove on-disk bytes for `hash` when no `documents.blob` row is visible.
/// Composition root calls this (or [`BlobStore::discard_uncommitted`]) after rollback.
pub async fn discard_unreferenced_blob(
    tx: &mut Tx<'_>,
    store: &dyn BlobStore,
    hash: crate::domain::BlobHash,
) -> Result<()> {
    if !store::blob_row_exists(tx, hash).await? {
        store.discard_hash(hash);
    }
    Ok(())
}

pub(crate) fn rebuild_chain(rows: &[Revision]) -> Vec<Revision> {
    if rows.is_empty() {
        return Vec::new();
    }
    let mut by_id = std::collections::HashMap::new();
    let mut children: std::collections::HashMap<Option<RevisionId>, Vec<RevisionId>> =
        std::collections::HashMap::new();
    for r in rows {
        by_id.insert(r.id, r);
        children.entry(r.supersedes).or_default().push(r.id);
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut stack: Vec<RevisionId> = children.get(&None).cloned().unwrap_or_default();
    while let Some(id) = stack.pop() {
        if let Some(row) = by_id.get(&id) {
            out.push((*row).clone());
            if let Some(next) = children.get(&Some(id)) {
                for child in next.iter().rev() {
                    stack.push(*child);
                }
            }
        }
    }
    if out.len() != rows.len() {
        return rows.to_vec();
    }
    out
}
