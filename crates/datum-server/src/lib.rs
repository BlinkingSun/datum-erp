//! HTTP API library surface. The binary does not bind a port in tests.

use anyhow as _;
use axum::{Router, routing::get};
use clap as _;
use serde as _;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

/// Crate error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] datum_db::Error),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Embedded placeholder migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Health handler. Async so it satisfies axum's `Handler` bound.
async fn health() -> &'static str {
    version()
}

/// Router that does not listen. Used so axum / tower / tracing stay linked.
pub fn router() -> Router {
    let _ = core::any::type_name::<datum_core::Error>();
    let _ = core::any::type_name::<datum_db::Error>();
    let _ = core::any::type_name::<datum_audit::Error>();
    let _ = core::any::type_name::<datum_identity::Error>();
    let _ = core::any::type_name::<datum_numbering::Error>();
    let _ = core::any::type_name::<datum_uom::Error>();
    let _ = core::any::type_name::<datum_events::Error>();
    let _ = core::any::type_name::<datum_jobs::Error>();
    let _ = core::any::type_name::<datum_ledger::Error>();
    let _ = core::any::type_name::<datum_statemachine::Error>();
    let _ = core::any::type_name::<datum_esign::Error>();
    let _ = core::any::type_name::<datum_customfields::Error>();
    let _ = core::any::type_name::<datum_documents::Error>();
    let _ = core::any::type_name::<datum_print::Error>();
    let _ = core::any::type_name::<datum_module::Error>();
    let _ = serde_json::json!({ "version": version() });
    let _ = tracing::info_span!("router");
    let _ = core::any::type_name::<tokio::runtime::Runtime>();
    Router::new()
        .route("/health", get(health))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
}

/// Composition entry. Unimplemented.
pub fn serve() -> Result<()> {
    let _ = router();
    Err(Error::Unimplemented)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(!MIGRATOR.migrations.is_empty());
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
