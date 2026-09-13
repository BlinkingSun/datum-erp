// w3b:documents

//! HTTP transport for controlled documents. Mutations go through
//! [`wicket_module::Kernel::create_document`],
//! [`wicket_module::Kernel::new_document_revision`], and
//! [`wicket_module::Kernel::transition`] — never `documents::transition` plus
//! the sync [`wicket_module::Kernel::signature_gate`].

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use wicket_core::{Identifier, SignatureError};
use wicket_db::Tx;
use wicket_documents::{Document, DocumentId, Manifest};

use crate::boot::AppState;
use crate::envelope::error_response;
use crate::error::{Error, Result};
use crate::extract::{self, check_version, require_if_match};
use crate::idempotency;
use crate::session::write_context;
use crate::wire::parse_uuid;

use super::{
    bind_esign_header, blocking, fresh_write, json_status, parse_json, required_edge_token, rid,
};

type H = HeaderMap;

/// Map this crate's errors: NotFound, Validation (`InvalidKind`), and
/// state-machine / signature refusals. Shared `error.rs` delegates here and
/// must not or this into the NotFound / Statemachine patterns.
pub fn envelope_arm(
    e: &wicket_documents::Error,
) -> (&'static str, StatusCode, Option<&'static str>, String) {
    match e {
        wicket_documents::Error::NotFound => {
            ("NOT_FOUND", StatusCode::NOT_FOUND, None, e.to_string())
        }
        wicket_documents::Error::InvalidKind(_) => (
            "VALIDATION",
            StatusCode::BAD_REQUEST,
            Some("kind"),
            e.to_string(),
        ),
        wicket_documents::Error::OverlappingEffectivity | wicket_documents::Error::LegalHold => {
            ("CONFLICT", StatusCode::CONFLICT, None, e.to_string())
        }
        wicket_documents::Error::UnknownProfile(_) => (
            "VALIDATION",
            StatusCode::BAD_REQUEST,
            Some("profile"),
            e.to_string(),
        ),
        wicket_documents::Error::Signature(sig) => map_signature(sig),
        wicket_documents::Error::StateMachine(sm) => map_statemachine(sm),
        _ => (
            "INTERNAL",
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            e.to_string(),
        ),
    }
}

fn map_signature(err: &SignatureError) -> (&'static str, StatusCode, Option<&'static str>, String) {
    match err {
        SignatureError::NoProvider => (
            "SIGNATURE_NO_PROVIDER",
            StatusCode::CONFLICT,
            None,
            "This transition requires a signature and no signature provider is bound.".into(),
        ),
        SignatureError::Consumed | SignatureError::HashMismatch => {
            ("CONFLICT", StatusCode::CONFLICT, None, err.to_string())
        }
        SignatureError::Invalid(msg) if msg == "missing token" => (
            "SIGNATURE_REQUIRED",
            StatusCode::UNAUTHORIZED,
            None,
            err.to_string(),
        ),
        SignatureError::SignerNotPermitted => (
            "SIGNATURE_REQUIRED",
            StatusCode::FORBIDDEN,
            None,
            err.to_string(),
        ),
        _ => (
            "SIGNATURE_REQUIRED",
            StatusCode::FORBIDDEN,
            None,
            err.to_string(),
        ),
    }
}

fn map_statemachine(
    err: &wicket_statemachine::Error,
) -> (&'static str, StatusCode, Option<&'static str>, String) {
    match err {
        wicket_statemachine::Error::Signature(sig) => map_signature(sig),
        wicket_statemachine::Error::PermissionDenied { .. } => {
            ("FORBIDDEN", StatusCode::FORBIDDEN, None, err.to_string())
        }
        wicket_statemachine::Error::ActionMismatch { .. } => (
            "VALIDATION",
            StatusCode::BAD_REQUEST,
            Some("action"),
            err.to_string(),
        ),
        wicket_statemachine::Error::InstanceNotFound { .. } => {
            ("NOT_FOUND", StatusCode::NOT_FOUND, None, err.to_string())
        }
        _ => (
            "INTERNAL",
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            err.to_string(),
        ),
    }
}

fn docs_from_kernel(err: wicket_module::Error) -> Error {
    match err {
        wicket_module::Error::Documents(e) => Error::Documents(e),
        other => Error::from(other),
    }
}

fn wrap_document_id(u: uuid::Uuid) -> DocumentId {
    DocumentId(Identifier::from_uuid(u))
}

fn actor(session: &crate::session::HttpSession) -> wicket_core::Actor {
    wicket_core::Actor {
        id: session.principal.0,
        kind: wicket_core::ActorKind::User,
    }
}

fn document_json(doc: &Document, version: i64) -> Value {
    json!({
        "id": doc.id.as_uuid().to_string(),
        "kind": doc.kind,
        "number": doc.number,
        "title": doc.title,
        "status": doc.status.as_str(),
        "retention_class": doc.retention_class,
        "legal_hold": doc.legal_hold,
        "version": version,
    })
}

async fn instance_version(state: &AppState, tx: &mut Tx<'_>, id: DocumentId) -> Result<i64> {
    let row = state
        .kernel()
        .load_sm_instance(tx, id.0)
        .await
        .map_err(docs_from_kernel)?;
    Ok(row.map(|(_, _, version)| version).unwrap_or(1))
}

async fn load_body(state: &AppState, tx: &mut Tx<'_>, id: DocumentId) -> Result<Value> {
    let doc = wicket_documents::load(tx, id).await?;
    let version = instance_version(state, tx, id).await?;
    Ok(document_json(&doc, version))
}

#[derive(Debug, Deserialize)]
struct DocumentCreate {
    kind: String,
    title: String,
    retention_class: String,
    #[serde(default)]
    id: Option<String>,
}

/// POST /api/v1/documents → [`wicket_module::Kernel::create_document`].
pub async fn create_document(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match create_document_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn create_document_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: DocumentCreate = parse_json(raw)?;
    if body.id.is_some() {
        return Err(Error::validation(
            "clients must not mint identifiers",
            Some("id"),
        ));
    }
    let session = extract::require_mutation(state, headers, request_id, "documents.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "documents.create",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let id = state
        .kernel()
        .create_document(&mut tx, &body.kind, &body.title, &body.retention_class)
        .await
        .map_err(docs_from_kernel)?;
    let payload = load_body(state, &mut tx, id).await?;
    idempotency::remember(&mut tx, key, &hash, 201, &payload).await?;
    tx.commit().await?;
    Ok((201, payload))
}

/// GET /api/v1/documents/{id} → [`wicket_documents::load`].
pub async fn get_document(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match get_document_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn get_document_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "documents.view").await?;
    let doc_id = parse_uuid(id, "id", wrap_document_id)?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "documents.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let body = load_body(state, &mut tx, doc_id).await;
    tx.rollback().await?;
    body
}

#[derive(Debug, Deserialize)]
struct RevisionCreate {
    label: String,
    #[serde(default)]
    content: Value,
    #[serde(default)]
    id: Option<String>,
}

/// POST /api/v1/documents/{id}/revisions →
/// [`wicket_module::Kernel::new_document_revision`] (event-emitting path).
pub async fn create_revision(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = rid(&headers);
    match create_revision_inner(&state, &headers, &request_id, &id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn create_revision_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: RevisionCreate = parse_json(raw)?;
    if body.id.is_some() {
        return Err(Error::validation(
            "clients must not mint identifiers",
            Some("id"),
        ));
    }
    let session = extract::require_mutation(state, headers, request_id, "documents.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let doc_id = parse_uuid(id, "id", wrap_document_id)?;
    let content = if body.content.is_null() {
        json!({})
    } else {
        body.content
    };
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "documents.edit",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let rev = state
        .kernel()
        .new_document_revision(&mut tx, doc_id, &body.label, Manifest::content(content))
        .await
        .map_err(docs_from_kernel)?;
    let payload = json!({
        "id": rev.as_uuid().to_string(),
        "document_id": doc_id.as_uuid().to_string(),
        "label": body.label,
    });
    idempotency::remember(&mut tx, key, &hash, 201, &payload).await?;
    tx.commit().await?;
    Ok((201, payload))
}

/// POST /api/v1/documents/{id}/submit → [`wicket_module::Kernel::transition`]
/// Draft→InReview. Not a Required edge; no signature token.
pub async fn submit_document(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = rid(&headers);
    let state2 = state.clone();
    let headers2 = headers.clone();
    let rid2 = request_id.clone();
    let raw = body.to_vec();
    match blocking(&request_id, move || async move {
        submit_document_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn submit_document_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let session = extract::require_mutation(state, headers, request_id, "documents.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let doc_id = parse_uuid(id, "id", wrap_document_id)?;
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_documents::DOC_TYPE.into(),
        doc_id: doc_id.0,
    };
    let mut ctx = state
        .kernel()
        .transition_context(actor(&session), &doc, "submit");
    ctx.session_id = Some(session.id.to_string());
    ctx.request_id = Some(request_id.to_string());
    ctx.source_kind = "api".into();
    ctx.reason = Some("api".into());
    ctx.actor_display = Some(session.display_name.clone());
    let write = fresh_write(state).await?;
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let version = instance_version(state, &mut tx, doc_id).await?;
    check_version(version, expected)?;
    state
        .kernel()
        .transition(&mut tx, &doc, "submit", None, &ctx)
        .await
        .map_err(docs_from_kernel)?;
    let payload = load_body(state, &mut tx, doc_id).await?;
    idempotency::remember(&mut tx, key, &hash, 200, &payload).await?;
    tx.commit().await?;
    Ok((200, payload))
}

/// POST /api/v1/documents/{id}/approve → [`wicket_module::Kernel::transition`]
/// with `bind_esign_header` + `required_edge_token` (lots / calibration).
pub async fn approve_document(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = rid(&headers);
    let state2 = state.clone();
    let headers2 = headers.clone();
    let rid2 = request_id.clone();
    let raw = body.to_vec();
    match blocking(&request_id, move || async move {
        approve_document_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn approve_document_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let session =
        extract::require_mutation(state, headers, request_id, "documents.approve").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let doc_id = parse_uuid(id, "id", wrap_document_id)?;
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_documents::DOC_TYPE.into(),
        doc_id: doc_id.0,
    };
    let mut ctx = state
        .kernel()
        .transition_context(actor(&session), &doc, "approve");
    ctx.session_id = Some(session.id.to_string());
    ctx.request_id = Some(request_id.to_string());
    ctx.source_kind = "api".into();
    ctx.reason = Some("api".into());
    ctx.actor_display = Some(session.display_name.clone());
    bind_esign_header(&mut ctx, headers);
    let write = fresh_write(state).await?;
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let version = instance_version(state, &mut tx, doc_id).await?;
    check_version(version, expected)?;
    let token = required_edge_token(
        state,
        &mut tx,
        headers,
        actor(&session),
        "Approved",
        doc_id.0,
        version,
    )
    .await?;
    state
        .kernel()
        .transition(&mut tx, &doc, "approve", token.as_ref(), &ctx)
        .await
        .map_err(docs_from_kernel)?;
    let payload = load_body(state, &mut tx, doc_id).await?;
    idempotency::remember(&mut tx, key, &hash, 200, &payload).await?;
    tx.commit().await?;
    Ok((200, payload))
}

#[cfg(test)]
mod tests {
    use super::envelope_arm;
    use axum::http::StatusCode;

    #[test]
    fn envelope_arm_maps_not_found() {
        let (code, status, field, _) = envelope_arm(&wicket_documents::Error::NotFound);
        assert_eq!(code, "NOT_FOUND");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(field, None);
    }

    #[test]
    fn envelope_arm_maps_invalid_kind() {
        let err = wicket_documents::Error::InvalidKind("nope".into());
        let (code, status, field, _) = envelope_arm(&err);
        assert_eq!(code, "VALIDATION");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(field, Some("kind"));
    }
}
