// w3b:print

//! HTTP transport for `wicket-print`. Published crate APIs only (R-2s-3).

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use wicket_core::{Identifier, RecordRef};
use wicket_db::{ReadPool, Tx};
use wicket_documents::{BlobHash, BlobStore};
use wicket_print::{
    Format, TemplateId, archive as print_archive, list_templates_on, log as print_log,
    manifestation_block, render as print_render,
};

use super::{H, json_status, parse_json, rid};
use crate::boot::AppState;
use crate::envelope::{ListBody, error_response};
use crate::error::{Error, Result};
use crate::extract;
use crate::idempotency;
use crate::session;
use crate::wire::parse_uuid;

use axum::body::Bytes;

/// Map `wicket_print::Error` into the docs/10 envelope.
pub fn envelope_arm(
    e: &wicket_print::Error,
) -> (&'static str, axum::http::StatusCode, Option<&str>, String) {
    match e {
        wicket_print::Error::NotFound => (
            "NOT_FOUND",
            axum::http::StatusCode::NOT_FOUND,
            None,
            e.to_string(),
        ),
        wicket_print::Error::UnknownTemplate(_) => (
            "NOT_FOUND",
            axum::http::StatusCode::NOT_FOUND,
            Some("template_id"),
            e.to_string(),
        ),
        wicket_print::Error::UnsupportedFormat => (
            "VALIDATION",
            axum::http::StatusCode::BAD_REQUEST,
            Some("format"),
            e.to_string(),
        ),
        wicket_print::Error::UnknownProfile(_) => (
            "VALIDATION",
            axum::http::StatusCode::BAD_REQUEST,
            Some("profile"),
            e.to_string(),
        ),
        wicket_print::Error::Documents(wicket_documents::Error::NotFound) => (
            "NOT_FOUND",
            axum::http::StatusCode::NOT_FOUND,
            None,
            e.to_string(),
        ),
        // Print has no StateMachine variant; keep INTERNAL for the rest.
        _ => (
            "INTERNAL",
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            None,
            e.to_string(),
        ),
    }
}

/// GET /api/v1/print/templates
pub async fn list_templates(State(state): State<AppState>, headers: H) -> Response {
    let request_id = rid(&headers);
    match list_templates_inner(&state, &headers, &request_id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_templates_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
) -> Result<ListBody<wicket_print::TemplateSummary>> {
    let _session =
        extract::require_permission(state, headers, request_id, "print.templates").await?;
    let rows = list_templates_on(
        &ReadPool::new(state.pool().clone()),
        state.kernel().profile.id.as_str(),
    )
    .await?;
    Ok(ListBody {
        data: rows,
        next_cursor: None,
        has_more: false,
    })
}

/// POST /api/v1/print/render
pub async fn render(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match render_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

#[derive(Debug, Deserialize)]
struct RecordBody {
    table: String,
    id: String,
    version: i64,
}

#[derive(Debug, Deserialize)]
struct RenderBody {
    record: RecordBody,
    format: String,
    template_id: String,
}

#[derive(Debug, Deserialize)]
struct ArchiveBody {
    record: RecordBody,
    output_hash: String,
}

fn parse_record(body: &RecordBody) -> Result<RecordRef> {
    let id = parse_uuid(&body.id, "record.id", Identifier::from_uuid)?;
    if body.table.is_empty() {
        return Err(Error::validation(
            "record.table is required",
            Some("record.table"),
        ));
    }
    Ok(RecordRef {
        table: body.table.clone(),
        id,
        version: body.version,
    })
}

fn hex_hash(bytes: &[u8; 32]) -> String {
    wicket_audit::sha256::hex(bytes)
}

fn parse_hex32(s: &str, field: &str) -> Result<[u8; 32]> {
    if s.len() != 64 {
        return Err(Error::validation(
            "hash must be 64 lowercase hex characters",
            Some(field),
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let hi = hex_nibble(chunk[0], field)?;
        let lo = hex_nibble(chunk[1], field)?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(b: u8, field: &str) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(Error::validation(
            "hash must be 64 lowercase hex characters",
            Some(field),
        )),
    }
}

fn b64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < input.len() {
        let remaining = input.len() - i;
        let b0 = input[i];
        let b1 = if remaining > 1 { input[i + 1] } else { 0 };
        let b2 = if remaining > 2 { input[i + 2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if remaining > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if remaining > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

async fn render_inner(
    state: &AppState,
    headers: &HeaderMap,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: RenderBody = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "print.render").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let record = parse_record(&body.record)?;
    let format = Format::parse(&body.format)
        .ok_or_else(|| Error::validation("format must be html or pdf", Some("format")))?;
    if body.template_id == "item_label" {
        return Err(Error::validation(
            "item_label is ADR 0009 territory and is not mounted",
            Some("template_id"),
        ));
    }
    let template = TemplateId::new(&body.template_id);
    let ctx = session::write_context(
        &session,
        "print.render",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let write = state.write_pool();
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let rendered = print_render(&mut tx, record.clone(), format, template).await?;
    let manifestation = manifestation_block(&mut tx, record).await?;
    let payload = json!({
        "output_hash": hex_hash(&rendered.output_hash),
        "template_version": rendered.template_version,
        "renderer_version": rendered.renderer_version,
        "bytes_base64": b64_encode(&rendered.bytes),
        "manifestation": manifestation,
    });
    idempotency::remember(&mut tx, key, &hash, 200, &payload).await?;
    tx.commit().await?;
    Ok((200, payload))
}

/// POST /api/v1/print/archive
pub async fn archive(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match archive_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => {
            state.blobs().keep_puts();
            json_status(st, v)
        }
        Err(e) => {
            state.blobs().discard_uncommitted();
            error_response(e, &request_id)
        }
    }
}

async fn archive_inner(
    state: &AppState,
    headers: &HeaderMap,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: ArchiveBody = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "print.archive").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let record = parse_record(&body.record)?;
    let output_hash = parse_hex32(&body.output_hash, "output_hash")?;
    let ctx = session::write_context(
        &session,
        "print.archive",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let write = state.write_pool();
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let rows = print_log(&mut tx, record.clone()).await?;
    let row = rows
        .into_iter()
        .find(|r| r.output_hash == output_hash)
        .ok_or_else(|| Error::not_found("no render_log row for this output_hash"))?;
    let format = Format::parse(&row.output_format).ok_or(wicket_print::Error::UnsupportedFormat)?;
    let rendered = print_render(
        &mut tx,
        record.clone(),
        format,
        TemplateId::new(&row.template_id),
    )
    .await?;
    if rendered.output_hash != output_hash {
        tx.rollback().await?;
        return Err(Error::conflict(
            "re-render did not reproduce the requested output_hash",
            Some("output_hash"),
        ));
    }
    let blob: BlobHash = print_archive(&mut tx, &rendered, record, state.blobs()).await?;
    let payload = json!({ "blob_hash": blob.to_hex() });
    idempotency::remember(&mut tx, key, &hash, 200, &payload).await?;
    tx.commit().await?;
    Ok((200, payload))
}
