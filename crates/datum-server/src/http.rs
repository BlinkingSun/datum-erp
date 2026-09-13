//! Router, middleware, and serve.

use std::net::SocketAddr;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::boot::AppState;
use crate::error::Result;
use crate::extract::{limits_mw, request_id_mw};
use crate::handlers;

/// Build the HTTP router. Does not bind a port.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/api/v1/openapi.json", get(handlers::openapi))
        .route("/api/v1/iq/manifest", get(handlers::manifest))
        .route("/api/v1/audit", get(handlers::audit_export))
        .route("/api/v1/navigation", get(handlers::navigation))
        .route("/api/v1/identity/login", post(handlers::login))
        .route("/api/v1/identity/logout", post(handlers::logout))
        .route("/api/v1/items", post(handlers::create_item))
        .route("/api/v1/items/{id}", get(handlers::get_item))
        .route("/api/v1/items/{id}/release", post(handlers::release_item))
        .route("/api/v1/locations", post(handlers::create_location))
        .route("/api/v1/locations/{id}", get(handlers::get_location))
        .route("/api/v1/lots", post(handlers::create_lot))
        .route("/api/v1/lots/{id}", get(handlers::get_lot))
        .route("/api/v1/lots/{id}/status", post(handlers::set_lot_status))
        .route(
            "/api/v1/lots/{id}/packages",
            get(handlers::list_packages).post(handlers::create_package),
        )
        .route("/api/v1/lots/{id}/serials", get(handlers::list_serials))
        .route("/api/v1/inventory/receipts", post(handlers::create_receipt))
        .route("/api/v1/inventory/releases", post(handlers::release_stock))
        .route("/api/v1/inventory/counts", post(handlers::create_count))
        .route(
            "/api/v1/inventory/reversals",
            post(handlers::create_reversal),
        )
        .route("/api/v1/inventory/on-hand", get(handlers::on_hand))
        .route("/api/v1/work-orders", post(handlers::create_wo))
        .route("/api/v1/work-orders/{id}", get(handlers::get_wo))
        .route(
            "/api/v1/work-orders/{id}/release",
            post(handlers::release_wo),
        )
        .route("/api/v1/work-orders/{id}/issue", post(handlers::issue_wo))
        .route(
            "/api/v1/work-orders/{id}/complete",
            post(handlers::complete_wo),
        )
        .route("/api/v1/production/work-orders", post(handlers::create_wo))
        .route("/api/v1/production/work-orders/{id}", get(handlers::get_wo))
        .route(
            "/api/v1/production/work-orders/{id}/release",
            post(handlers::release_wo),
        )
        .route(
            "/api/v1/production/work-orders/{id}/issue",
            post(handlers::issue_wo),
        )
        .route(
            "/api/v1/production/work-orders/{id}/complete",
            post(handlers::complete_wo),
        )
        .route("/api/v1/genealogy/trace", get(handlers::genealogy_trace))
        .route(
            "/api/v1/calibration/certificates/{id}/approve",
            post(handlers::approve_calibration),
        )
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(axum::middleware::from_fn(limits_mw))
        .layer(axum::middleware::from_fn(request_id_mw))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .with_state(state)
}

/// Bind and serve.
pub async fn serve(state: AppState) -> Result<()> {
    let bind: SocketAddr = state.bind();
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| crate::error::Error::Config(format!("bind {bind}: {e}")))?;
    tracing::info!(%bind, "datum listening");
    axum::serve(listener, router(state))
        .await
        .map_err(|e| crate::error::Error::Config(format!("serve: {e}")))?;
    Ok(())
}
