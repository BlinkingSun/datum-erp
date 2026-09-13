//! Reversible migration tests.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use sqlx::{migrate::Migrator, query_scalar};
use wicket_documents::MIGRATOR;

use common::open_db;

fn documents_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_documents");
    migrator
}

#[tokio::test]
async fn migrate_down_then_up() {
    for profile in common::PROFILES {
        let Some(db) = open_db("doc_mig", profile).await else {
            continue;
        };
        wicket_db::migrate::run(
            db.migrate_pool(),
            &[
                ("wicket-db", &wicket_db::MIGRATOR),
                ("wicket-audit", &wicket_audit::MIGRATOR),
            ],
        )
        .await
        .expect("kernel");
        let boot = db.bootstrap_pool().await.expect("bootstrap");
        wicket_audit::install_privileged(&boot)
            .await
            .expect("install_privileged");
        boot.close().await;
        let migrator = documents_migrator();
        migrator.run(db.migrate_pool()).await.expect("up");
        migrator.undo(db.migrate_pool(), 0).await.expect("down");
        let gone: bool = query_scalar("SELECT to_regclass('documents.document') IS NULL")
            .fetch_one(db.migrate_pool())
            .await
            .expect("probe");
        assert!(gone);
        migrator.run(db.migrate_pool()).await.expect("up again");
        db.finish().await.expect("finish");
    }
}
