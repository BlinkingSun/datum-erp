//! HTTP API library. The binary binds a port; tests drive [`http::router`].

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)
)]

use anyhow as _;
use datum_customfields as _;
use datum_documents as _;
use datum_esign as _;
use datum_events as _;
use datum_jobs as _;
use datum_numbering as _;
use datum_print as _;
use datum_uom as _;

mod boot;
mod cli;
mod config;
mod envelope;
mod error;
mod extract;
mod handlers;
mod http;
mod idempotency;
mod openapi;
mod read;
mod session;
mod wire;

pub use boot::{App, AppState, build_kernel, migrate_slice_modules, run_iq, startup_guard_release};
pub use config::{Config, bootstrap_against_app, rewrite_database, with_os_userinfo};
pub use envelope::{ErrorBody, ListBody};
pub use error::{Error, Result};
pub use http::{router, serve};
pub use openapi::{document as openapi_document, mounted_operations, registered_operations};

/// Embedded migrator (`placeholder` + `0001_server`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// CLI entry used by the `datum` binary.
pub async fn run_cli() -> Result<()> {
    cli::run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(MIGRATOR.migrations.len() >= 2);
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
        let _ = crate::idempotency::body_hash(b"{}");
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
