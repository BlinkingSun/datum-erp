//! HTTP handlers. Mutations open `Tx::begin` only after a session is present.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
use wicket_core::{
    Identifier, ItemId, LocationId, LotId, RecordRef, SignatureError, SignatureId,
    SignatureMeaning, SignatureToken,
};
use wicket_db::{ReadPool, Tx, WriteContext, WritePool};
use wicket_identity::{PasswordProvider, Provider};
use wicket_ledger::CostMethod;
use wicket_mod_inventory::{BalanceQuery, DocumentKind, LineInput, ReceiveRequest, ReleaseRequest};
use wicket_mod_items::{Kind, NewItem, UpdateItem};
use wicket_mod_locations::{CreateLocation, LocationKind};
use wicket_mod_lots::{CreateLotBody, CreateSerialsBody, LotStatus, PackageLevel, SetStatusBody};
use wicket_mod_production_min::{
    CompleteRequest, CreateWorkOrder, FinishedLotTemplate, IssueMaterialRequest, StartRequest,
};

use crate::boot::AppState;
use crate::envelope::error_response;
use crate::error::{Error, Result};
use crate::extract::{self, check_version, require_if_match};
use crate::idempotency;
use crate::session::{self, write_context};
use crate::wire::{MoneyBody, QuantityBody, parse_uuid};

use axum::body::Bytes;

type H = HeaderMap;

fn rid(headers: &H) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("missing")
        .to_string()
}

fn parse_json<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|e| Error::validation(e.to_string(), None))
}

fn nonempty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn parse_limit(raw: Option<&str>) -> Result<Option<u32>> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let n: u32 = s
        .parse()
        .map_err(|_| Error::validation("limit must be an integer", Some("limit")))?;
    if !(1..=200).contains(&n) {
        return Err(Error::validation(
            "limit must be between 1 and 200",
            Some("limit"),
        ));
    }
    Ok(Some(n))
}

fn map_items_err(e: wicket_mod_items::Error) -> Error {
    match e {
        wicket_mod_items::Error::InvalidLimit => {
            Error::validation("limit must be between 1 and 200", Some("limit"))
        }
        wicket_mod_items::Error::StockMeasureImmutable
        | wicket_mod_items::Error::StandardCostRequired
        | wicket_mod_items::Error::InvalidTransition { .. }
        | wicket_mod_items::Error::Manifest(_) => Error::validation(e.to_string(), None),
        other => other.into(),
    }
}

fn map_locations_err(e: wicket_mod_locations::Error) -> Error {
    match e {
        wicket_mod_locations::Error::Validation(ref msg) => {
            let field = if msg == "limit" {
                Some("limit")
            } else if msg == "cursor" {
                Some("cursor")
            } else {
                None
            };
            Error::validation(e.to_string(), field)
        }
        wicket_mod_locations::Error::OnHand | wicket_mod_locations::Error::Protected => {
            Error::http("REFUSED", e.to_string(), None, StatusCode::CONFLICT)
        }
        wicket_mod_locations::Error::Immutable(_) | wicket_mod_locations::Error::Cycle => {
            Error::validation(e.to_string(), None)
        }
        other => other.into(),
    }
}

fn map_lots_err(e: wicket_mod_lots::Error) -> Error {
    match e {
        wicket_mod_lots::Error::InvalidLimit => {
            Error::validation("limit must be between 1 and 200", Some("limit"))
        }
        other => other.into(),
    }
}

fn map_production_err(e: wicket_mod_production_min::Error) -> Error {
    match e {
        wicket_mod_production_min::Error::InvalidLimit => {
            Error::validation("limit must be between 1 and 200", Some("limit"))
        }
        wicket_mod_production_min::Error::Manifest(_) => Error::validation(e.to_string(), None),
        other => other.into(),
    }
}

fn map_genealogy_err(e: wicket_mod_genealogy::Error) -> Error {
    match e {
        wicket_mod_genealogy::Error::Lots(wicket_mod_lots::Error::NotFound) => {
            Error::not_found("lot not found")
        }
        wicket_mod_genealogy::Error::NotFound => Error::not_found("not found"),
        other => other.into(),
    }
}

fn parse_cost_method(raw: &str) -> Result<CostMethod> {
    match raw {
        "FIFO" | "Fifo" | "fifo" => Ok(CostMethod::Fifo),
        "MOVING_AVG" | "MovingAvg" => Ok(CostMethod::MovingAvg),
        "STANDARD" | "Standard" => Ok(CostMethod::Standard),
        other => Err(Error::validation(
            format!("unknown cost_method {other}"),
            Some("cost_method"),
        )),
    }
}

fn json_status(status: u16, v: Value) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(v),
    )
        .into_response()
}

/// POST /api/v1/identity/login
pub async fn login(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match login_inner(&state, &headers, &request_id, &body).await {
        Ok(r) => r,
        Err(e) => error_response(e, &request_id),
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    username: String,
    password: String,
}

async fn login_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<Response> {
    let body: LoginBody = parse_json(raw)?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim());
    let write = state.write_pool();
    let mut ctx = session::system_ctx("identity.login");
    ctx.request_id = Some(request_id.to_string());
    ctx.config_version = Some(state.kernel().profile.spec_version.clone());
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some((status, stored)) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return login_cookies(headers, status, stored);
    }
    let sess = PasswordProvider
        .login(&mut tx, &body.username, &body.password, None, ip)
        .await?;
    let principal = wicket_identity::load_principal(state.pool(), sess.principal).await?;
    let csrf = Uuid::now_v7().to_string();
    let actor = wicket_core::Actor {
        id: sess.principal.0,
        kind: wicket_core::ActorKind::User,
    };
    let permissions = session::granted_permissions(&mut tx, actor).await?;
    session::store(
        &mut tx,
        sess.id,
        sess.principal,
        &principal.display_name,
        &csrf,
        sess.expires_at,
        &permissions,
    )
    .await?;
    let payload = json!({
        "session_id": sess.id.to_string(),
        "principal_id": sess.principal.as_uuid().to_string(),
        "display_name": principal.display_name,
        "csrf": csrf,
    });
    idempotency::remember(&mut tx, key, &hash, 200, &payload).await?;
    tx.commit().await?;
    login_cookies(headers, 200, payload)
}

fn login_cookies(headers: &H, status: u16, payload: Value) -> Result<Response> {
    let session_id = payload
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Config("login replay missing session_id".into()))?
        .to_string();
    let csrf = payload
        .get("csrf")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Config("login replay missing csrf".into()))?
        .to_string();
    let mut resp = (
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        Json(payload),
    )
        .into_response();
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|p| p.eq_ignore_ascii_case("https"));
    set_cookie(
        resp.headers_mut(),
        "wicket_session",
        &session_id,
        true,
        secure,
    );
    set_cookie(resp.headers_mut(), "wicket_csrf", &csrf, false, secure);
    Ok(resp)
}

fn set_cookie(headers: &mut H, name: &str, value: &str, http_only: bool, secure: bool) {
    let mut c = format!("{name}={value}; Path=/; SameSite=Lax");
    if http_only {
        c.push_str("; HttpOnly");
    }
    if secure {
        c.push_str("; Secure");
    }
    if let Ok(v) = HeaderValue::from_str(&c) {
        headers.append(axum::http::header::SET_COOKIE, v);
    }
}

/// POST /api/v1/identity/logout
pub async fn logout(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match logout_inner(&state, &headers, &request_id, &body).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn logout_inner(state: &AppState, headers: &H, request_id: &str, raw: &[u8]) -> Result<()> {
    let session = extract::require_mutation(state, headers, request_id, "identity.session").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "identity.logout",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if idempotency::replay(&mut tx, key, &hash).await?.is_some() {
        tx.commit().await?;
        return Ok(());
    }
    session::drop_session(&mut tx, session.id).await?;
    let empty = json!({});
    idempotency::remember(&mut tx, key, &hash, 204, &empty).await?;
    tx.commit().await?;
    Ok(())
}

/// GET /api/v1/openapi.json — unauthenticated.
pub async fn openapi(State(state): State<AppState>) -> Json<Value> {
    Json(crate::openapi::document(&state))
}

/// GET /api/v1/iq/manifest
pub async fn manifest(State(state): State<AppState>, headers: H) -> Response {
    let request_id = rid(&headers);
    if let Err(e) =
        extract::require_permission(&state, &headers, &request_id, "validation.manifest.read").await
    {
        return error_response(e, &request_id);
    }
    match crate::boot::live_manifest(state.pool()).await {
        Ok(m) => Json(json!({
            "profile_id": m.profile_id,
            "spec_version": m.spec_version,
            "content_hash": m.content_hash,
            "modules": m.modules,
            "signature_edges": m.signature_edges,
            "kernel_order": m.kernel_order,
        }))
        .into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

/// GET /api/v1/navigation
pub async fn navigation(State(state): State<AppState>, headers: H) -> Response {
    let request_id = rid(&headers);
    if let Err(e) =
        extract::require_permission(&state, &headers, &request_id, "identity.session").await
    {
        return error_response(e, &request_id);
    }
    Json(json!({
        "visible": state.kernel().profile.navigation.visible,
        "hidden": state.kernel().profile.navigation.hidden,
    }))
    .into_response()
}

/// GET /api/v1/audit — admin SELECT (permission-gated; kernel, not a module).
pub async fn audit_export(State(state): State<AppState>, headers: H) -> Response {
    let request_id = rid(&headers);
    if let Err(e) = extract::require_permission(&state, &headers, &request_id, "audit.export").await
    {
        return error_response(e, &request_id);
    }
    match wicket_audit::head(state.pool()).await {
        Ok(head) => Json(json!({
            "head": head.map(|h| json!({
                "seq": h.seq,
                "xid": h.xid,
                "row_count": h.row_count,
                "chain_algo": h.chain_algo,
            })),
        }))
        .into_response(),
        Err(e) => error_response(e.into(), &request_id),
    }
}

#[derive(Deserialize)]
pub struct ItemCreate {
    number: String,
    revision: String,
    description: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    stock_uom: i64,
    #[serde(default)]
    stock_scale: i16,
    #[serde(default)]
    residual_tolerance: Option<String>,
    #[serde(default)]
    cost_method: Option<String>,
    #[serde(default)]
    standard: Option<MoneyBody>,
    #[serde(default)]
    id: Option<String>,
}

/// POST /api/v1/items
pub async fn create_item(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match create_item_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn create_item_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: ItemCreate = parse_json(raw)?;
    if body.id.is_some() {
        return Err(Error::validation(
            "clients must not mint identifiers",
            Some("id"),
        ));
    }
    let session = extract::require_mutation(state, headers, request_id, "items.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let kind_s = body.kind.or(body.r#type).unwrap_or_else(|| "make".into());
    let kind = Kind::parse(&kind_s).map_err(|e| Error::validation(e.to_string(), Some("kind")))?;
    let residual = body
        .residual_tolerance
        .as_deref()
        .unwrap_or("0")
        .parse()
        .map_err(|_| Error::validation("residual_tolerance", Some("residual_tolerance")))?;
    let method = match body.cost_method.as_deref().unwrap_or("FIFO") {
        "FIFO" | "Fifo" | "fifo" => CostMethod::Fifo,
        "MOVING_AVG" | "MovingAvg" => CostMethod::MovingAvg,
        "STANDARD" | "Standard" => CostMethod::Standard,
        other => {
            return Err(Error::validation(
                format!("unknown cost_method {other}"),
                Some("cost_method"),
            ));
        }
    };
    let new = NewItem {
        number: body.number,
        revision: body.revision,
        description: body.description,
        kind,
        stock_uom: wicket_core::UnitId(body.stock_uom),
        stock_scale: body.stock_scale,
        residual_tolerance: residual,
        cost_method: method,
        standard: match body.standard {
            Some(m) => Some(m.to_money()?),
            None => None,
        },
    };
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "items.create",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let item = wicket_mod_items::create(&mut tx, state.kernel(), new).await?;
    let body = serde_json::to_value(wicket_mod_items::api::ItemBody::from(&item))?;
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// GET /api/v1/items/{id}
pub async fn get_item(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    if let Err(e) = extract::require_permission(&state, &headers, &request_id, "items.view").await {
        return error_response(e, &request_id);
    }
    let item_id = match parse_uuid(&id, "id", ItemId::from_uuid) {
        Ok(i) => i,
        Err(e) => return error_response(e, &request_id),
    };
    match wicket_mod_items::get(state.pool(), item_id).await {
        Ok(item) => Json(wicket_mod_items::api::ItemBody::from(&item)).into_response(),
        Err(e) => error_response(e.into(), &request_id),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ItemListQ {
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    number_prefix: Option<String>,
}

/// GET /api/v1/items
pub async fn list_items(
    State(state): State<AppState>,
    headers: H,
    Query(q): Query<ItemListQ>,
) -> Response {
    let request_id = rid(&headers);
    match list_items_inner(&state, &headers, &request_id, q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_items_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    q: ItemListQ,
) -> Result<Value> {
    let _session = extract::require_permission(state, headers, request_id, "items.view").await?;
    let limit = parse_limit(nonempty(&q.limit))?;
    let cursor = match nonempty(&q.cursor) {
        Some(c) => Some(parse_uuid(c, "cursor", ItemId::from_uuid)?),
        None => None,
    };
    let kind = match nonempty(&q.kind) {
        Some(s) => {
            Some(Kind::parse(s).map_err(|e| Error::validation(e.to_string(), Some("kind")))?)
        }
        None => None,
    };
    let status = match nonempty(&q.status) {
        Some(s) => Some(
            wicket_mod_items::Status::parse(s)
                .map_err(|e| Error::validation(e.to_string(), Some("status")))?,
        ),
        None => None,
    };
    let filter = wicket_mod_items::ListFilter {
        kind,
        status,
        number_prefix: nonempty(&q.number_prefix).map(str::to_string),
        cursor,
        limit,
    };
    let page = wicket_mod_items::list(state.pool(), filter)
        .await
        .map_err(map_items_err)?;
    let data: Vec<wicket_mod_items::api::ItemBody> = page
        .data
        .iter()
        .map(wicket_mod_items::api::ItemBody::from)
        .collect();
    Ok(json!({
        "data": data,
        "next_cursor": page.next_cursor,
        "has_more": page.has_more,
    }))
}

#[derive(Deserialize)]
struct ItemPatch {
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    stock_uom: Option<i64>,
    #[serde(default)]
    stock_scale: Option<i16>,
    #[serde(default)]
    residual_tolerance: Option<String>,
    #[serde(default)]
    cost_method: Option<String>,
    #[serde(default)]
    standard: Option<MoneyBody>,
}

/// PATCH /api/v1/items/{id}
pub async fn update_item(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = rid(&headers);
    match update_item_inner(&state, &headers, &request_id, &id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn update_item_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: ItemPatch = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "items.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let item_id = parse_uuid(id, "id", ItemId::from_uuid)?;
    let kind = match body.kind.or(body.r#type) {
        Some(s) => {
            Some(Kind::parse(&s).map_err(|e| Error::validation(e.to_string(), Some("kind")))?)
        }
        None => None,
    };
    let residual = match body.residual_tolerance.as_deref() {
        Some(s) => Some(
            s.parse()
                .map_err(|_| Error::validation("residual_tolerance", Some("residual_tolerance")))?,
        ),
        None => None,
    };
    let cost_method = match body.cost_method.as_deref() {
        Some(s) => Some(parse_cost_method(s)?),
        None => None,
    };
    let standard = match body.standard {
        Some(m) => Some(m.to_money()?),
        None => None,
    };
    let patch = UpdateItem {
        version: expected,
        revision: body.revision,
        description: body.description,
        kind,
        stock_uom: body.stock_uom.map(wicket_core::UnitId),
        stock_scale: body.stock_scale,
        residual_tolerance: residual,
        cost_method,
        standard,
    };
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "items.update",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let item = wicket_mod_items::update(&mut tx, item_id, patch)
        .await
        .map_err(map_items_err)?;
    let body = serde_json::to_value(wicket_mod_items::api::ItemBody::from(&item))?;
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[allow(clippy::result_large_err)]
fn blocking<T, F, Fut>(request_id: &str, f: F) -> std::result::Result<T, Response>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T>>,
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| Error::Config(format!("runtime: {e}")))?;
            rt.block_on(f())
        })();
        let _ = tx.send(result);
    });
    match rx.recv() {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(error_response(e, request_id)),
        Err(e) => Err(error_response(Error::Config(format!("{e}")), request_id)),
    }
}

async fn fresh_write(state: &AppState) -> Result<WritePool> {
    Ok(WritePool::new(
        wicket_db::connect(state.database_url()).await?,
    ))
}

/// POST /api/v1/items/{id}/release
pub async fn release_item(
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
        release_item_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn release_item_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let session = extract::require_mutation(state, headers, request_id, "items.release").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let item_id = parse_uuid(id, "id", ItemId::from_uuid)?;
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_mod_items::DOC_TYPE.into(),
        doc_id: Identifier::from_uuid(item_id.as_uuid()),
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc, "release");
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
    let current = wicket_mod_items::get(state.pool(), item_id).await?;
    check_version(current.version, expected)?;
    let item = wicket_mod_items::release(&mut tx, state.kernel(), &ctx, item_id).await?;
    let body = serde_json::to_value(wicket_mod_items::api::ItemBody::from(&item))?;
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

trait IntoActor {
    fn into_actor(self) -> wicket_core::Actor;
}

impl IntoActor for wicket_core::Identifier {
    fn into_actor(self) -> wicket_core::Actor {
        wicket_core::Actor {
            id: self,
            kind: wicket_core::ActorKind::User,
        }
    }
}

fn dummy_signature_token(
    actor: wicket_core::Actor,
    meaning: &str,
    record_id: Identifier,
    version: i64,
    signature: SignatureId,
) -> SignatureToken {
    SignatureToken {
        signature,
        signer: actor,
        meaning: SignatureMeaning(meaning.into()),
        record: RecordRef {
            table: "sm.instance".into(),
            id: record_id,
            version,
        },
        record_content_hash: [0; 32],
    }
}

/// Stamp `WriteContext.esign_id` from `X-Wicket-Signature` before `Tx::begin`
/// (D-2b-1). Never a nil UUID (D-2b-5 missing-token).
fn bind_esign_header(ctx: &mut WriteContext, headers: &H) {
    if let Some(raw) = headers
        .get("x-wicket-signature")
        .and_then(|v| v.to_str().ok())
        && let Ok(id) = Uuid::parse_str(raw)
        && !id.is_nil()
    {
        ctx.esign_id = Some(id.to_string());
    }
}

/// Pre-transition Consumed check (w2b-c1-server). Reads `consumed_at`
/// through the published esign seam — never `SELECT esign.*` here (R-2s-3).
async fn signature_row_consumed(tx: &mut Tx<'_>, id: SignatureId) -> Result<bool> {
    Ok(wicket_esign::signature_consumed_at(tx, id).await?.is_some())
}

fn consumed_conflict() -> Error {
    Error::http(
        "CONFLICT",
        SignatureError::Consumed.to_string(),
        None,
        StatusCode::CONFLICT,
    )
}

/// Bound-edge token: `X-Wicket-Signature` when present, else `None` so the
/// executor reports `Invalid("missing token")` (D-2b-5). A consumed row is
/// 409 before the state-machine edge runs.
async fn required_edge_token(
    state: &AppState,
    tx: &mut Tx<'_>,
    headers: &H,
    actor: wicket_core::Actor,
    meaning: &str,
    record_id: Identifier,
    version: i64,
) -> Result<Option<SignatureToken>> {
    let Some(raw) = headers
        .get("x-wicket-signature")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    let Ok(id) = Uuid::parse_str(raw) else {
        return Ok(None);
    };
    if id.is_nil() {
        return Ok(None);
    }
    let sid = SignatureId::from_uuid(id);
    if signature_row_consumed(tx, sid).await? {
        return Err(consumed_conflict());
    }
    if let Some(tok) = state.kernel().load_signature_token(tx, sid).await? {
        return Ok(Some(tok));
    }
    Ok(Some(dummy_signature_token(
        actor, meaning, record_id, version, sid,
    )))
}

/// Mint response is the crate D-2b-2 wire (including live-version supersession).
async fn manifestation_via_tx(tx: &mut Tx<'_>, id: SignatureId) -> Result<Value> {
    let body = wicket_esign::manifestation_in_tx(tx, id).await?;
    Ok(serde_json::to_value(body)?)
}

/// POST /api/v1/locations
pub async fn create_location(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match create_loc_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

#[derive(Deserialize)]
pub struct LocCreate {
    code: String,
    name: String,
    #[serde(default)]
    kind: Option<String>,
}

async fn create_loc_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: LocCreate = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "locations.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let kind = match body.kind.as_deref().unwrap_or("warehouse") {
        "warehouse" => LocationKind::Warehouse,
        "area" => LocationKind::Area,
        "bin" => LocationKind::Bin,
        "wip" => LocationKind::Wip,
        other => {
            return Err(Error::validation(
                format!("unknown kind {other}"),
                Some("kind"),
            ));
        }
    };
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "locations.create",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let site = wicket_mod_locations::default_site_id(&mut tx).await?;
    let loc = wicket_mod_locations::create(
        &mut tx,
        CreateLocation {
            code: body.code,
            name: body.name,
            site_id: site,
            parent_id: None,
            kind,
        },
    )
    .await?;
    let body = json!({
        "id": loc.id.to_string(),
        "code": loc.code,
        "name": loc.name,
        "kind": loc.kind.as_sql(),
        "version": loc.version,
    });
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// GET /api/v1/locations/{id}
pub async fn get_location(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match get_location_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn get_location_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "locations.view").await?;
    let loc_id = parse_uuid(id, "id", LocationId::from_uuid)?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "locations.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let loc = wicket_mod_locations::get(&mut tx, loc_id).await;
    tx.rollback().await?;
    let loc = loc?;
    Ok(json!({
        "id": loc.id.to_string(),
        "code": loc.code,
        "name": loc.name,
        "kind": loc.kind.as_sql(),
        "version": loc.version,
    }))
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQ {
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

/// GET /api/v1/locations
pub async fn list_locations(
    State(state): State<AppState>,
    headers: H,
    Query(q): Query<ListQ>,
) -> Response {
    let request_id = rid(&headers);
    match list_locations_inner(&state, &headers, &request_id, q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_locations_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    q: ListQ,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "locations.view").await?;
    let limit = parse_limit(nonempty(&q.limit))?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "locations.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let page = wicket_mod_locations::list_locations(&mut tx, limit, nonempty(&q.cursor)).await;
    tx.rollback().await?;
    serde_json::to_value(&page.map_err(map_locations_err)?).map_err(Error::from)
}

#[derive(Debug, Default, Deserialize)]
pub struct TreeQ {
    #[serde(default)]
    include_inactive: Option<String>,
}

fn parse_include_inactive(raw: Option<&str>) -> Result<bool> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(false),
        Some("true") | Some("1") => Ok(true),
        Some("false") | Some("0") => Ok(false),
        Some(other) => Err(Error::validation(
            format!("include_inactive must be true or false, got {other}"),
            Some("include_inactive"),
        )),
    }
}

/// GET /api/v1/locations/tree
pub async fn list_location_tree(
    State(state): State<AppState>,
    headers: H,
    Query(q): Query<TreeQ>,
) -> Response {
    let request_id = rid(&headers);
    match list_location_tree_inner(&state, &headers, &request_id, q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_location_tree_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    q: TreeQ,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "locations.view").await?;
    let include_inactive = parse_include_inactive(nonempty(&q.include_inactive))?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "locations.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let page = wicket_mod_locations::list_tree(&mut tx, include_inactive).await;
    tx.rollback().await?;
    serde_json::to_value(&page.map_err(map_locations_err)?).map_err(Error::from)
}

/// POST /api/v1/locations/{id}/deactivate
pub async fn deactivate_location(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = rid(&headers);
    match deactivate_location_inner(&state, &headers, &request_id, &id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn deactivate_location_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let session = extract::require_mutation(state, headers, request_id, "locations.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let loc_id = parse_uuid(id, "id", LocationId::from_uuid)?;
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "locations.deactivate",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let loc = wicket_mod_locations::deactivate_location(&mut tx, loc_id, expected)
        .await
        .map_err(map_locations_err)?;
    let body = serde_json::to_value(&loc)?;
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

/// POST /api/v1/lots
pub async fn create_lot(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match create_lot_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn create_lot_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: CreateLotBody = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "lots.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let ctx = write_context(
        &session,
        "lots.create",
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
    let lot = wicket_mod_lots::create_lot_http(&mut tx, state.kernel(), &ctx, body).await?;
    let body = serde_json::to_value(&lot)?;
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// GET /api/v1/lots/{id}
pub async fn get_lot(
    State(state): State<AppState>,
    headers: H,

    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match get_lot_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn get_lot_inner(state: &AppState, headers: &H, request_id: &str, id: &str) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "lots.view").await?;
    let lot_id = parse_uuid(id, "id", LotId::from_uuid)?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "lots.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let lot = wicket_mod_lots::get_lot(&mut tx, lot_id).await;
    tx.rollback().await?;
    serde_json::to_value(&lot?).map_err(Error::from)
}

/// POST /api/v1/lots/{id}/status
pub async fn set_lot_status(
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
        set_status_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn set_status_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: SetStatusBody = parse_json(raw)?;
    let perm = match body.status {
        LotStatus::Available => "lots.release",
        _ => "lots.edit",
    };
    let session = extract::require_mutation(state, headers, request_id, perm).await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let lot_id = parse_uuid(id, "id", LotId::from_uuid)?;
    let edge = match body.status {
        LotStatus::Available => "release",
        LotStatus::Hold => "hold",
        LotStatus::Rejected => "reject",
        LotStatus::Quarantine => {
            return Err(Error::validation(
                "cannot set quarantine via status",
                Some("status"),
            ));
        }
    };
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_mod_lots::DOC_TYPE.into(),
        doc_id: Identifier::from_uuid(lot_id.as_uuid()),
    };
    let mut ctx = state
        .kernel()
        .transition_context(session.principal.0.into_actor(), &doc, edge);
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
    let current = wicket_mod_lots::get_lot(&mut tx, lot_id).await?;
    check_version(current.version, expected)?;
    if edge == "release" {
        let _ = required_edge_token(
            state,
            &mut tx,
            headers,
            session.principal.0.into_actor(),
            "Lot released",
            Identifier::from_uuid(lot_id.as_uuid()),
            current.version,
        )
        .await?;
    }
    let lot = wicket_mod_lots::set_status_http(&mut tx, state.kernel(), &ctx, lot_id, body).await?;
    let body = serde_json::to_value(&lot)?;
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[derive(Deserialize)]
pub struct PackageCreate {
    level: String,
    contained: QuantityBody,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    label_ref: Option<String>,
}

/// POST /api/v1/lots/{id}/packages
pub async fn create_package(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = rid(&headers);
    match create_pkg_inner(&state, &headers, &request_id, &id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn create_pkg_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: PackageCreate = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "lots.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let lot_id = parse_uuid(id, "id", LotId::from_uuid)?;
    let level = PackageLevel::parse(&body.level)
        .map_err(|e| Error::validation(e.to_string(), Some("level")))?;
    let contained = body.contained.to_qty()?;
    let parent = match body.parent_id {
        Some(p) => Some(parse_uuid(&p, "parent_id", |u| {
            wicket_mod_lots::PackageId::from_uuid(u)
        })?),
        None => None,
    };
    let ctx = write_context(
        &session,
        "lots.edit",
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
    let pkg = wicket_mod_lots::create_package(
        &mut tx,
        lot_id,
        parent,
        level,
        contained,
        body.label_ref.as_deref(),
    )
    .await?;
    let body = json!({
        "id": pkg.id.as_uuid().to_string(),
        "lot_id": pkg.lot.to_string(),
        "parent_id": pkg.parent.map(|p| p.as_uuid().to_string()),
        "level": pkg.level.as_str(),
        "contained": QuantityBody::from_qty(&pkg.contained),
    });
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// GET /api/v1/lots/{id}/packages
pub async fn list_packages(
    State(state): State<AppState>,
    headers: H,

    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match list_pkg_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_pkg_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "lots.view").await?;
    let lot_id = parse_uuid(id, "id", LotId::from_uuid)?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "lots.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let tree = wicket_mod_lots::package_hierarchy(&mut tx, lot_id).await;
    tx.rollback().await?;
    let tree = tree?;
    let data: Vec<Value> = tree
        .iter()
        .map(|p| {
            json!({
                "id": p.id.as_uuid().to_string(),
                "parent_id": p.parent.map(|x| x.as_uuid().to_string()),
                "level": p.level.as_str(),
                "contained": QuantityBody::from_qty(&p.contained),
            })
        })
        .collect();
    Ok(json!({"data": data, "next_cursor": null, "has_more": false}))
}

/// GET /api/v1/lots/{id}/serials
pub async fn list_serials(
    State(state): State<AppState>,
    headers: H,

    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match list_serials_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_serials_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "lots.view").await?;
    let lot_id = parse_uuid(id, "id", LotId::from_uuid)?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "lots.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let page = wicket_mod_lots::list_serials(&mut tx, lot_id, Some(200), None).await;
    tx.rollback().await?;
    serde_json::to_value(&page?).map_err(Error::from)
}

/// GET /api/v1/lots
pub async fn list_lots(
    State(state): State<AppState>,
    headers: H,
    Query(q): Query<ListQ>,
) -> Response {
    let request_id = rid(&headers);
    match list_lots_inner(&state, &headers, &request_id, q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_lots_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    q: ListQ,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "lots.view").await?;
    let limit = parse_limit(nonempty(&q.limit))?.map(i64::from);
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "lots.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let page = wicket_mod_lots::list_lots(&mut tx, limit, nonempty(&q.cursor)).await;
    tx.rollback().await?;
    serde_json::to_value(&page.map_err(map_lots_err)?).map_err(Error::from)
}

/// POST /api/v1/lots/{id}/serials
pub async fn create_serials(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = rid(&headers);
    match create_serials_inner(&state, &headers, &request_id, &id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn create_serials_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: CreateSerialsBody = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "lots.edit").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let lot_id = parse_uuid(id, "id", LotId::from_uuid)?;
    let write = state.write_pool();
    let ctx = write_context(
        &session,
        "lots.edit",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let page = wicket_mod_lots::create_serials_http(&mut tx, state.kernel(), lot_id, body)
        .await
        .map_err(map_lots_err)?;
    let body = serde_json::to_value(&page)?;
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

#[derive(Deserialize)]
pub struct ReceiptBody {
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    lot_id: Option<String>,
    #[serde(default)]
    location_id: Option<String>,
    #[serde(default)]
    purchase_order: Option<String>,
    #[serde(default)]
    quantity: Option<QuantityBody>,
    #[serde(default)]
    entered: Option<QuantityBody>,
    #[serde(default)]
    unit_cost: Option<MoneyBody>,
    #[serde(default)]
    lines: Option<Vec<ReceiptLine>>,
    #[serde(default)]
    actor_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ReceiptLine {
    item_id: String,
    #[serde(default)]
    lot_id: Option<String>,
    #[serde(default)]
    package_id: Option<String>,
    #[serde(default)]
    quantity: Option<QuantityBody>,
    #[serde(default)]
    entered: Option<QuantityBody>,
    #[serde(default)]
    amount: Option<MoneyBody>,
}

/// POST /api/v1/inventory/receipts
pub async fn create_receipt(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    let state2 = state.clone();
    let headers2 = headers.clone();
    let rid2 = request_id.clone();
    let raw = body.to_vec();
    match blocking(&request_id, move || async move {
        receipt_inner(&state2, &headers2, &rid2, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn receipt_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: ReceiptBody = parse_json(raw)?;
    if body.actor_id.is_some() {
        return Err(Error::validation(
            "actor_id is not accepted",
            Some("actor_id"),
        ));
    }
    let session =
        extract::require_mutation(state, headers, request_id, "inventory.receive").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let to_location = match body.location_id {
        Some(s) => parse_uuid(&s, "location_id", LocationId::from_uuid)?,
        None => {
            return Err(Error::validation(
                "location_id required",
                Some("location_id"),
            ));
        }
    };
    let mut lines = Vec::new();
    if let Some(extra) = body.lines {
        for l in extra {
            lines.push(line_input(&l)?);
        }
    } else {
        let item = body
            .item_id
            .as_deref()
            .ok_or_else(|| Error::validation("item_id required", Some("item_id")))?;
        let entered = body
            .entered
            .as_ref()
            .or(body.quantity.as_ref())
            .ok_or_else(|| Error::validation("quantity required", Some("quantity")))?
            .to_qty()?;
        lines.push(LineInput {
            item: parse_uuid(item, "item_id", ItemId::from_uuid)?,
            entered,
            lot: match body.lot_id {
                Some(s) => Some(parse_uuid(&s, "lot_id", LotId::from_uuid)?),
                None => None,
            },
            serial: None,
            from_location: None,
            to_location: Some(to_location),
            package: None,
            amount: match body.unit_cost {
                Some(c) => {
                    let unit = c.to_money()?;
                    let qty = entered.amount;
                    Some(
                        wicket_core::Money::new(unit.amount() * qty, unit.currency())
                            .map_err(|e| Error::validation(e.to_string(), Some("unit_cost")))?,
                    )
                }
                None => None,
            },
            reason_code: None,
        });
    }
    let req = ReceiveRequest {
        to_location,
        reference: body.purchase_order,
        lines,
        expected: None,
        tolerance: None,
        idempotency_key: Some(key),
    };
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_mod_inventory::DOC_TYPE.into(),
        doc_id: Identifier::generate(),
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc, "receive");
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
    let posted = wicket_mod_inventory::receive(&mut tx, state.kernel(), &ctx, req).await?;
    let body = serde_json::to_value(wicket_mod_inventory::DocumentBody::from(&posted))?;
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

fn line_input(l: &ReceiptLine) -> Result<LineInput> {
    let entered = if let Some(q) = l.entered.as_ref().or(l.quantity.as_ref()) {
        q.to_qty()?
    } else if l.package_id.is_some() {
        wicket_core::AnyQuantity {
            amount: rust_decimal::Decimal::ZERO,
            unit: wicket_core::UnitId(1),
            dimension: wicket_core::DimensionKind::Count,
        }
    } else {
        return Err(Error::validation("quantity required", Some("quantity")));
    };
    Ok(LineInput {
        item: parse_uuid(&l.item_id, "item_id", ItemId::from_uuid)?,
        entered,
        lot: match &l.lot_id {
            Some(s) => Some(parse_uuid(s, "lot_id", LotId::from_uuid)?),
            None => None,
        },
        serial: None,
        from_location: None,
        to_location: None,
        package: match &l.package_id {
            Some(s) => Some(parse_uuid(s, "package_id", |u| {
                wicket_mod_lots::PackageId::from_uuid(u)
            })?),
            None => None,
        },
        amount: match &l.amount {
            Some(m) => Some(m.to_money()?),
            None => None,
        },
        reason_code: None,
    })
}

#[derive(Deserialize)]
pub struct ReleaseInvBody {
    lot_id: String,
    from_location_id: String,
    to_location_id: String,
    entered: QuantityBody,
    #[serde(default)]
    amount: Option<MoneyBody>,
}

/// POST /api/v1/inventory/releases
pub async fn release_stock(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    let state2 = state.clone();
    let headers2 = headers.clone();
    let rid2 = request_id.clone();
    let raw = body.to_vec();
    match blocking(&request_id, move || async move {
        release_stock_inner(&state2, &headers2, &rid2, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn release_stock_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: ReleaseInvBody = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "lots.release").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let lot = parse_uuid(&body.lot_id, "lot_id", LotId::from_uuid)?;
    let req = ReleaseRequest {
        lot,
        from_location: parse_uuid(
            &body.from_location_id,
            "from_location_id",
            LocationId::from_uuid,
        )?,
        to_location: parse_uuid(
            &body.to_location_id,
            "to_location_id",
            LocationId::from_uuid,
        )?,
        entered: body.entered.to_qty()?,
        amount: match body.amount {
            Some(m) => Some(m.to_money()?),
            None => None,
        },
        idempotency_key: Some(key),
    };
    // release_from_quarantine posts the movement then set_status (action lot.release).
    let doc = wicket_statemachine::DocRef {
        doc_type: "lot".into(),
        doc_id: Identifier::from_uuid(lot.as_uuid()),
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc, "release");
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
    let current = wicket_mod_lots::get_lot(&mut tx, lot).await?;
    check_version(current.version, expected)?;
    let _ = required_edge_token(
        state,
        &mut tx,
        headers,
        session.principal.0.into_actor(),
        "Lot released",
        Identifier::from_uuid(lot.as_uuid()),
        current.version,
    )
    .await?;
    let posted =
        wicket_mod_inventory::release_from_quarantine(&mut tx, state.kernel(), &ctx, req).await?;
    let body = serde_json::to_value(wicket_mod_inventory::DocumentBody::from(&posted))?;
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[derive(Deserialize)]
pub struct OnHandQ {
    item_id: String,
    #[serde(default)]
    location_id: Option<String>,
    #[serde(default)]
    lot_id: Option<String>,
}

/// GET /api/v1/inventory/on-hand
pub async fn on_hand(
    State(state): State<AppState>,
    headers: H,

    Query(q): Query<OnHandQ>,
) -> Response {
    let request_id = rid(&headers);
    match on_hand_inner(&state, &headers, &request_id, q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn on_hand_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    q: OnHandQ,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "inventory.view").await?;
    let item = parse_uuid(&q.item_id, "item_id", ItemId::from_uuid)?;
    let location = match q.location_id {
        Some(s) => Some(parse_uuid(&s, "location_id", LocationId::from_uuid)?),
        None => None,
    };
    let lot = match q.lot_id {
        Some(s) => Some(parse_uuid(&s, "lot_id", LotId::from_uuid)?),
        None => None,
    };
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "inventory.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let query = BalanceQuery {
        item,
        location,
        lot,
    };
    let qty = wicket_mod_inventory::on_hand(&mut tx, query).await;
    let avail = wicket_mod_inventory::available(&mut tx, query).await;
    tx.rollback().await?;
    Ok(json!({
        "on_hand": qty?.to_string(),
        "available": avail?.to_string(),
    }))
}

#[derive(Deserialize)]
pub struct WoCreate {
    item_id: String,
    quantity: QuantityBody,
    revision: String,
    #[serde(default)]
    id: Option<String>,
}

/// POST /api/v1/work-orders
pub async fn create_wo(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match create_wo_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn create_wo_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: WoCreate = parse_json(raw)?;
    if body.id.is_some() {
        return Err(Error::validation(
            "clients must not mint identifiers",
            Some("id"),
        ));
    }
    let session =
        extract::require_mutation(state, headers, request_id, "production.create").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let ctx = write_context(
        &session,
        "production.create",
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
    let wo = wicket_mod_production_min::create(
        &mut tx,
        state.kernel(),
        CreateWorkOrder {
            item: parse_uuid(&body.item_id, "item_id", ItemId::from_uuid)?,
            quantity_ordered: body.quantity.to_qty()?,
            revision: body.revision,
        },
    )
    .await?;
    let body = wo_json(&wo)?;
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

fn wo_json(wo: &wicket_mod_production_min::WorkOrder) -> Result<Value> {
    Ok(json!({
        "id": wo.id.to_string(),
        "number": wo.number,
        "item_id": wo.item.to_string(),
        "quantity": QuantityBody::from_qty(&wo.quantity_ordered),
        "status": wo.status.as_str(),
        "revision": wo.revision,
        "wip_location_id": wo.wip_location.map(|l| l.to_string()),
        "version": wo.version,
        "application_version": wo.application_version,
        "configuration_version": wo.configuration_version,
        "released_at": wo.released_at.map(|t| t.to_rfc3339()),
        "completed_at": wo.completed_at.map(|t| t.to_rfc3339()),
    }))
}

/// GET /api/v1/work-orders/{id}
pub async fn get_wo(State(state): State<AppState>, headers: H, Path(id): Path<String>) -> Response {
    let request_id = rid(&headers);
    match get_wo_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn get_wo_inner(state: &AppState, headers: &H, request_id: &str, id: &str) -> Result<Value> {
    let session =
        extract::require_permission(state, headers, request_id, "production.view").await?;
    let wo_id = parse_uuid(id, "id", Identifier::from_uuid)?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "production.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let wo = wicket_mod_production_min::load(&mut tx, wo_id).await;
    tx.rollback().await?;
    wo_json(&wo?)
}

#[derive(Debug, Default, Deserialize)]
pub struct WoListQ {
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// GET /api/v1/work-orders
pub async fn list_work_orders(
    State(state): State<AppState>,
    headers: H,
    Query(q): Query<WoListQ>,
) -> Response {
    let request_id = rid(&headers);
    match list_work_orders_inner(&state, &headers, &request_id, q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn list_work_orders_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    q: WoListQ,
) -> Result<Value> {
    let session =
        extract::require_permission(state, headers, request_id, "production.view").await?;
    let limit = parse_limit(nonempty(&q.limit))?;
    let cursor = match nonempty(&q.cursor) {
        Some(c) => Some(parse_uuid(c, "cursor", Identifier::from_uuid)?),
        None => None,
    };
    let status = match nonempty(&q.status) {
        Some(s) => Some(
            wicket_mod_production_min::Status::parse(s)
                .map_err(|e| Error::validation(e.to_string(), Some("status")))?,
        ),
        None => None,
    };
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "production.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let page = wicket_mod_production_min::list(
        &mut tx,
        wicket_mod_production_min::ListFilter {
            status,
            cursor,
            limit,
        },
    )
    .await;
    tx.rollback().await?;
    let page = page.map_err(map_production_err)?;
    let data: Result<Vec<Value>> = page.data.iter().map(wo_json).collect();
    Ok(json!({
        "data": data?,
        "next_cursor": page.next_cursor,
        "has_more": page.has_more,
    }))
}

/// POST .../release
pub async fn release_wo(
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
        release_wo_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn release_wo_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let session =
        extract::require_mutation(state, headers, request_id, "production.release").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let wo_id = parse_uuid(id, "id", Identifier::from_uuid)?;
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_mod_production_min::DOC_TYPE.into(),
        doc_id: wo_id,
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc, "release");
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
    let current = wicket_mod_production_min::load(&mut tx, wo_id).await?;
    check_version(current.version, expected)?;
    let wo = wicket_mod_production_min::release(&mut tx, state.kernel(), &ctx, wo_id).await?;
    let body = wo_json(&wo)?;
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[derive(Deserialize)]
pub struct IssueBody {
    from_location_id: String,
    lines: Vec<ReceiptLine>,
}

/// POST .../issue
pub async fn issue_wo(
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
        issue_wo_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn issue_wo_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: IssueBody = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "production.issue").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let wo_id = parse_uuid(id, "id", Identifier::from_uuid)?;
    let lines: Result<Vec<_>> = body.lines.iter().map(line_input).collect();
    let lines = lines?;
    let from_location = parse_uuid(
        &body.from_location_id,
        "from_location_id",
        LocationId::from_uuid,
    )?;
    let wo_doc = wicket_statemachine::DocRef {
        doc_type: wicket_mod_production_min::DOC_TYPE.into(),
        doc_id: wo_id,
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &wo_doc, "issue");
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
    let current = wicket_mod_production_min::load(&mut tx, wo_id).await?;
    check_version(current.version, expected)?;
    let wo = wicket_mod_production_min::start(
        &mut tx,
        state.kernel(),
        &ctx,
        StartRequest {
            work_order: wo_id,
            issue: Some(IssueMaterialRequest {
                work_order: wo_id,
                from_location,
                lines,
                idempotency_key: Some(key),
            }),
        },
    )
    .await?;
    let body = wo_json(&wo)?;
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[derive(Deserialize)]
pub struct CompleteBody {
    quantity: QuantityBody,
    #[serde(default)]
    scrap: Option<QuantityBody>,
    #[serde(default)]
    finished_lot_number: Option<String>,
    location_id: String,
    #[serde(default)]
    serial_from: Option<String>,
    #[serde(default)]
    serial_template: Option<String>,
}

/// POST .../complete
pub async fn complete_wo(
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
        complete_wo_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn complete_wo_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: CompleteBody = parse_json(raw)?;
    let session =
        extract::require_mutation(state, headers, request_id, "production.complete").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let wo_id = parse_uuid(id, "id", Identifier::from_uuid)?;
    let good = body.quantity.to_qty()?;
    let scrap = match body.scrap {
        Some(s) => s.to_qty()?,
        None => AnyQuantityZero::zero_like(&good),
    };
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_mod_production_min::DOC_TYPE.into(),
        doc_id: wo_id,
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc, "complete");
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
    let current = wicket_mod_production_min::load(&mut tx, wo_id).await?;
    check_version(current.version, expected)?;
    let completion = wicket_mod_production_min::complete(
        &mut tx,
        state.kernel(),
        &ctx,
        CompleteRequest {
            work_order: wo_id,
            to_location: parse_uuid(&body.location_id, "location_id", LocationId::from_uuid)?,
            good,
            scrap,
            finished_lot: FinishedLotTemplate {
                number: body.finished_lot_number,
                template: None,
                serial_template: body.serial_template.or(body.serial_from),
            },
        },
    )
    .await?;
    let wo = wicket_mod_production_min::load(&mut tx, wo_id).await?;
    let body = json!({
        "id": wo.id.to_string(),
        "number": wo.number,
        "status": wo.status.as_str(),
        "quantity": QuantityBody::from_qty(&wo.quantity_ordered),
        "finished_lot": {
            "id": completion.finished_lot.to_string(),
        },
        "group_id": completion.group_id.to_string(),
        "version": wo.version,
        "posted_at": wo.completed_at.map(|t| t.to_rfc3339()),
        "application_version": wo.application_version,
        "configuration_version": wo.configuration_version,
    });
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

struct AnyQuantityZero;
impl AnyQuantityZero {
    fn zero_like(q: &wicket_core::AnyQuantity) -> wicket_core::AnyQuantity {
        wicket_core::AnyQuantity {
            amount: Decimal::ZERO,
            unit: q.unit,
            dimension: q.dimension,
        }
    }
}

#[derive(Deserialize)]
pub struct TraceQ {
    #[serde(default)]
    from_lot_id: Option<String>,
    #[serde(default)]
    direction: Option<String>,
}

/// GET /api/v1/genealogy/trace
pub async fn genealogy_trace(
    State(state): State<AppState>,
    headers: H,

    Query(q): Query<TraceQ>,
) -> Response {
    let request_id = rid(&headers);
    match trace_inner(&state, &headers, &request_id, q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn trace_inner(state: &AppState, headers: &H, request_id: &str, q: TraceQ) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "genealogy.view").await?;
    let lot = q
        .from_lot_id
        .as_deref()
        .ok_or_else(|| Error::validation("from_lot_id required", Some("from_lot_id")))?;
    let lot_id = parse_uuid(lot, "from_lot_id", LotId::from_uuid)?;
    let direction = match q.direction.as_deref().unwrap_or("forward") {
        "forward" => wicket_mod_genealogy::Direction::Forward,
        "backward" => wicket_mod_genealogy::Direction::Backward,
        "both" => wicket_mod_genealogy::Direction::Both,
        other => {
            return Err(Error::validation(
                format!("bad direction {other}"),
                Some("direction"),
            ));
        }
    };
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "genealogy.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let outcome = wicket_mod_genealogy::trace(
        &mut tx,
        session.principal.0.into_actor(),
        wicket_mod_genealogy::TraceRequest {
            origin: wicket_mod_genealogy::TraceOrigin::Lot(lot_id),
            direction,
            depth: None,
            max_postings: None,
        },
    )
    .await;
    tx.rollback().await?;
    match outcome? {
        wicket_mod_genealogy::TraceOutcome::Inline(body) => Ok(serde_json::to_value(&body)?),
        wicket_mod_genealogy::TraceOutcome::Accepted { job_id, result_url } => Ok(json!({
            "job_id": job_id.0.to_string(),
            "result_url": result_url
        })),
    }
}

/// GET /api/v1/genealogy/impact/{lot}
pub async fn get_impact(
    State(state): State<AppState>,
    headers: H,
    Path(lot): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match get_impact_inner(&state, &headers, &request_id, &lot).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn get_impact_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    lot: &str,
) -> Result<Value> {
    let session = extract::require_permission(state, headers, request_id, "genealogy.view").await?;
    let lot_id = parse_uuid(lot, "lot", LotId::from_uuid)?;
    let write = crate::read::pool(state);
    let mut tx = crate::read::begin(
        &write,
        &session,
        "genealogy.view",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    )
    .await?;
    let impact = wicket_mod_genealogy::impact(&mut tx, lot_id).await;
    tx.rollback().await?;
    serde_json::to_value(&impact.map_err(map_genealogy_err)?).map_err(Error::from)
}

/// GET /api/v1/genealogy/jobs/{id}
pub async fn get_genealogy_job(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match get_genealogy_job_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e, &request_id),
    }
}

async fn get_genealogy_job_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
) -> Result<Value> {
    let _session =
        extract::require_permission(state, headers, request_id, "genealogy.view").await?;
    let job_id = parse_uuid(id, "id", |u| wicket_jobs::JobId(Identifier::from_uuid(u)))?;
    match wicket_mod_genealogy::job_status(state.pool(), job_id)
        .await
        .map_err(map_genealogy_err)?
    {
        Some(status) => serde_json::to_value(&status).map_err(Error::from),
        None => Err(Error::not_found("job not found")),
    }
}

/// POST /api/v1/calibration/certificates/{id}/approve — item 7 probe.
pub async fn approve_calibration(
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
        approve_cal_inner(&state2, &headers2, &rid2, &id, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn approve_cal_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let session =
        extract::require_mutation(state, headers, request_id, "calibration.approve").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let expected = require_if_match(headers)?;
    let _ = expected;
    let doc_id = parse_uuid(id, "id", Identifier::from_uuid)?;
    let doc = wicket_statemachine::DocRef {
        doc_type: "calibration.certificate".into(),
        doc_id,
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc, "approve");
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
    // Required edge: `X-Wicket-Signature` or `None` (D-2b-5 missing token).
    let token = required_edge_token(
        state,
        &mut tx,
        headers,
        session.principal.0.into_actor(),
        "Approved",
        doc_id,
        1,
    )
    .await?;
    state
        .kernel()
        .transition(&mut tx, &doc, "approve", token.as_ref(), &ctx)
        .await?;
    let body = json!({"id": doc_id.to_string(), "status": "approved"});
    idempotency::remember(&mut tx, key, &hash, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[derive(Deserialize)]
struct CountBody {
    location_id: String,
    #[serde(default)]
    reference: Option<String>,
    lines: Vec<CountLineBody>,
    #[serde(default)]
    tolerance: Option<String>,
}

#[derive(Deserialize)]
struct CountLineBody {
    item_id: String,
    #[serde(default)]
    lot_id: Option<String>,
    counted: QuantityBody,
    expected: QuantityBody,
}

/// POST /api/v1/inventory/counts
pub async fn create_count(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    let state2 = state.clone();
    let headers2 = headers.clone();
    let rid2 = request_id.clone();
    let raw = body.to_vec();
    match blocking(&request_id, move || async move {
        count_inner(&state2, &headers2, &rid2, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn count_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: CountBody = parse_json(raw)?;
    let session = extract::require_mutation(state, headers, request_id, "inventory.count").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let location = parse_uuid(&body.location_id, "location_id", LocationId::from_uuid)?;
    let mut lines = Vec::new();
    for l in body.lines {
        lines.push(wicket_mod_inventory::CountLine {
            item: parse_uuid(&l.item_id, "item_id", ItemId::from_uuid)?,
            lot: match l.lot_id {
                Some(s) => Some(parse_uuid(&s, "lot_id", LotId::from_uuid)?),
                None => None,
            },
            serial: None,
            counted: l.counted.to_qty()?,
            expected: l.expected.to_qty()?,
            amount: None,
        });
    }
    let tolerance: rust_decimal::Decimal = body
        .tolerance
        .as_deref()
        .unwrap_or("0")
        .parse()
        .map_err(|_| Error::validation("tolerance", Some("tolerance")))?;
    let req = wicket_mod_inventory::CountRequest {
        location,
        reference: body.reference,
        lines,
        tolerance,
        idempotency_key: Some(key),
    };
    let doc = wicket_statemachine::DocRef {
        doc_type: wicket_mod_inventory::DOC_TYPE.into(),
        doc_id: Identifier::generate(),
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc, "count");
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
    let posted = wicket_mod_inventory::cycle_count(&mut tx, state.kernel(), &ctx, req).await?;
    let body = serde_json::to_value(wicket_mod_inventory::DocumentBody::from(&posted))?;
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

#[derive(Deserialize)]
pub struct ReversalBody {
    document_id: String,
    reason: String,
}

/// POST /api/v1/inventory/reversals
///
/// One `Tx::begin` / one `WriteContext` (R-2s-7): `reverse_posted_issue` posts
/// the ledger `REVERSAL` inside the request transaction; the GUC is never rebound.
pub async fn create_reversal(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    let state2 = state.clone();
    let headers2 = headers.clone();
    let rid2 = request_id.clone();
    let raw = body.to_vec();
    match blocking(&request_id, move || async move {
        reverse_inner(&state2, &headers2, &rid2, &raw).await
    }) {
        Ok((st, v)) => json_status(st, v),
        Err(r) => r,
    }
}

async fn reverse_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let body: ReversalBody = parse_json(raw)?;
    if body.reason.trim().is_empty() {
        return Err(Error::validation("reason is required", Some("reason")));
    }
    let session = extract::require_mutation(state, headers, request_id, "inventory.adjust").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let issue_id = parse_uuid(&body.document_id, "document_id", Identifier::from_uuid)?;
    let doc_ref = wicket_statemachine::DocRef {
        doc_type: wicket_mod_inventory::DOC_TYPE.into(),
        doc_id: issue_id,
    };
    let mut ctx =
        state
            .kernel()
            .transition_context(session.principal.0.into_actor(), &doc_ref, "void");
    ctx.session_id = Some(session.id.to_string());
    ctx.request_id = Some(request_id.to_string());
    ctx.source_kind = "api".into();
    ctx.reason = Some(body.reason.clone());
    ctx.actor_display = Some(session.display_name.clone());
    let write = fresh_write(state).await?;
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let current = wicket_mod_inventory::load_document(&mut tx, issue_id).await?;
    if current.kind != DocumentKind::Issue {
        return Err(Error::conflict(
            "only issue documents can be reversed",
            Some("document_id"),
        ));
    }
    if current.posted_group_id.is_none() {
        return Err(Error::conflict(
            "document is not posted",
            Some("document_id"),
        ));
    }
    let reversal_group =
        wicket_mod_inventory::reverse_posted_issue(&mut tx, state.kernel(), &ctx, issue_id).await?;
    let posted = wicket_mod_inventory::load_document(&mut tx, issue_id).await?;
    let mut payload = serde_json::to_value(wicket_mod_inventory::DocumentBody::from(&posted))?;
    payload["reversal_group_id"] = json!(reversal_group.to_string());
    payload["reason"] = json!(body.reason);
    idempotency::remember(&mut tx, key, &hash, 201, &payload).await?;
    tx.commit().await?;
    Ok((201, payload))
}

/// Health.

#[derive(Deserialize)]
struct EsignMintBody {
    meaning: String,
    #[serde(default)]
    reason: Option<String>,
    record: EsignRecordBody,
    identification: EsignIdentBody,
    #[serde(default)]
    doc_type: Option<String>,
}

#[derive(Deserialize)]
struct EsignRecordBody {
    table: String,
    id: String,
    version: i64,
}

#[derive(Deserialize)]
struct EsignIdentBody {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    secret: Option<String>,
}

/// Permission keys to snapshot at mint, from the target record's Required
/// edges (`machine` × `meaning`). `permission_snapshot` is a set: two
/// Required edges that share a meaning on one machine both appear.
///
/// The static table is the fallback when the machine has no Required edge
/// for the meaning (plain-shop `NoSignatures`).
fn permission_for_meaning(
    profile: &wicket_module::Profile,
    machine: &str,
    meaning: &str,
) -> Vec<wicket_core::PermissionKey> {
    let keys = profile.required_edge_permission(machine, meaning);
    if !keys.is_empty() {
        return keys.into_iter().map(wicket_core::PermissionKey).collect();
    }
    vec![wicket_core::PermissionKey(
        match meaning {
            "Approved" => "calibration.approve",
            "Released" => "wo.release",
            "Lot released" => "lots.release",
            other => other,
        }
        .into(),
    )]
}

/// POST /api/v1/esign/challenges
pub async fn esign_challenge(State(state): State<AppState>, headers: H) -> Response {
    let request_id = rid(&headers);
    match esign_challenge_inner(&state, &headers, &request_id).await {
        Ok(v) => json_status(200, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn esign_challenge_inner(state: &AppState, headers: &H, request_id: &str) -> Result<Value> {
    let session = extract::require_mutation(state, headers, request_id, "identity.session").await?;
    let write = fresh_write(state).await?;
    let ctx = session::write_context(
        &session,
        "esign.challenge",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    let challenge = wicket_esign::challenge(
        &mut tx,
        session.principal,
        &state.kernel().profile.session_policy,
    )
    .await?;
    tx.commit().await?;
    Ok(serde_json::to_value(challenge)?)
}

/// POST /api/v1/esign/signatures
pub async fn esign_mint(State(state): State<AppState>, headers: H, body: Bytes) -> Response {
    let request_id = rid(&headers);
    match esign_mint_inner(&state, &headers, &request_id, &body).await {
        Ok((st, v)) => json_status(st, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn esign_mint_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    raw: &[u8],
) -> Result<(u16, Value)> {
    let session = extract::require_mutation(state, headers, request_id, "identity.session").await?;
    let key = idempotency::require_key(headers)?;
    let hash = idempotency::body_hash(raw);
    let body: EsignMintBody = parse_json(raw)?;
    let write = fresh_write(state).await?;
    let ctx = session::write_context(
        &session,
        "esign.mint",
        request_id,
        headers,
        &state.kernel().profile.spec_version,
    );
    let mut tx = Tx::begin(&write, &ctx).await?;
    if let Some(replay) = idempotency::replay(&mut tx, key, &hash).await? {
        tx.commit().await?;
        return Ok(replay);
    }
    let principal = wicket_identity::load_principal(state.pool(), session.principal).await?;
    let doc_id = parse_uuid(&body.record.id, "record.id", Identifier::from_uuid)?;
    let rec = RecordRef {
        table: body.record.table.clone(),
        id: doc_id,
        version: body.record.version,
    };
    let meaning = body.meaning.clone();
    let fallback_type = body
        .doc_type
        .clone()
        .unwrap_or_else(|| body.record.table.clone());
    let (doc_type, inst) = if body.record.table == "sm.instance" {
        match state.kernel().load_sm_instance(&mut tx, doc_id).await? {
            Some((live_type, state_name, version)) => (
                body.doc_type.unwrap_or(live_type.clone()),
                wicket_esign::InstanceTriple {
                    doc_type: live_type,
                    doc_id,
                    state: state_name,
                    version,
                },
            ),
            None => (
                fallback_type.clone(),
                wicket_esign::InstanceTriple {
                    doc_type: fallback_type.clone(),
                    doc_id,
                    state: String::new(),
                    version: body.record.version,
                },
            ),
        }
    } else {
        (
            fallback_type.clone(),
            wicket_esign::InstanceTriple {
                doc_type: fallback_type.clone(),
                doc_id,
                state: String::new(),
                version: body.record.version,
            },
        )
    };
    let mut components = Vec::new();
    if body
        .identification
        .code
        .as_ref()
        .is_some_and(|c| !c.is_empty())
    {
        components.push("code".into());
    }
    if body
        .identification
        .secret
        .as_ref()
        .is_some_and(|s| !s.is_empty())
    {
        components.push("secret".into());
    }
    let projection = state
        .kernel()
        .live_record(&mut tx, &doc_type, doc_id)
        .await?;
    // `required_edge_permission` returns a set (shared meaning → every
    // matching key). mint takes one PermissionKey; the first matching
    // Required-edge permission is snapshotted, plus meaning_policy's hint.
    let permission = permission_for_meaning(&state.kernel().profile, &inst.doc_type, &meaning)
        .into_iter()
        .next()
        .expect("permission_for_meaning never empty");
    let sig = wicket_esign::mint(
        &mut tx,
        &wicket_esign::MintRequest {
            components,
            code: body.identification.code.clone(),
            secret: body.identification.secret.clone().unwrap_or_default(),
            meaning: SignatureMeaning(meaning.clone()),
            reason: body.reason,
            record: rec,
            doc_type,
            projection,
            instance: inst,
            permission,
            signed_at_zone: state
                .kernel()
                .profile
                .seeded_permissions
                .display_timezone
                .clone(),
            policy: state.kernel().profile.session_policy.clone(),
            principal,
            login_session_id: Some(session.id),
            device_fingerprint: headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string),
            source_ip: headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(',').next().unwrap_or(s).trim().to_string()),
            boot_epoch: "1".into(),
            credential_kind: "signing_password".into(),
        },
    )
    .await?;
    let sig_id = sig.id;
    let body = manifestation_via_tx(&mut tx, sig_id).await?;
    idempotency::remember(&mut tx, key, &hash, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// GET /api/v1/esign/signatures/{id}
pub async fn esign_manifestation(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match esign_manifestation_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => json_status(200, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn esign_manifestation_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
) -> Result<Value> {
    let _session =
        extract::require_permission(state, headers, request_id, "identity.session").await?;
    let sig_id = parse_uuid(id, "id", wicket_core::SignatureId::from_uuid)?;
    let body = wicket_esign::manifestation(&ReadPool::new(state.pool().clone()), sig_id).await?;
    Ok(serde_json::to_value(body)?)
}

/// GET /api/v1/esign/signatures/{id}/bundle
pub async fn esign_bundle(
    State(state): State<AppState>,
    headers: H,
    Path(id): Path<String>,
) -> Response {
    let request_id = rid(&headers);
    match esign_bundle_inner(&state, &headers, &request_id, &id).await {
        Ok(v) => json_status(200, v),
        Err(e) => error_response(e, &request_id),
    }
}

async fn esign_bundle_inner(
    state: &AppState,
    headers: &H,
    request_id: &str,
    id: &str,
) -> Result<Value> {
    let _session =
        extract::require_permission(state, headers, request_id, "esign.bundle.read").await?;
    let sig_id = parse_uuid(id, "id", wicket_core::SignatureId::from_uuid)?;
    let body = wicket_esign::archival_bundle(&ReadPool::new(state.pool().clone()), sig_id).await?;
    Ok(serde_json::to_value(body)?)
}

pub async fn health() -> &'static str {
    crate::version()
}

pub mod customfields;
pub mod documents;
pub mod print;
