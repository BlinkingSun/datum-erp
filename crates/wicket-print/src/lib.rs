//! Deterministic record rendering and archival print (HTML/PDF).
//!
//! Writes go through [`wicket_db::Tx`] only. Schema `print` (class `app`).

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)
)]

mod api;
mod domain;
mod error;
mod reads;
mod render;
mod store;

pub use api::{
    archive, bump_template, list_templates, list_templates_on, log, manifestation_block, render,
    seed_templates, set_installation_profile,
};
pub use domain::{Format, RenderLogRow, Rendered, TemplateId, TemplateSummary};
pub use error::{Error, Result};

/// Embedded migrator (`placeholder` + `0001_print`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn migrator_has_print_migration() {
        assert!(MIGRATOR.migrations.len() >= 2);
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = wicket_test::postgres_available();
    }

    proptest! {
        #[test]
        fn renderer_version_is_non_empty(_x in 0u8..2) {
            prop_assert!(!render::renderer_version().is_empty());
        }
    }
}
