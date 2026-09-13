#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_docs
)]

mod common;

use datum_print::MIGRATOR;
use sqlx::{migrate::Migrator, query_scalar};

use common::{PROFILES, open_db};

fn print_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_print");
    migrator
}

#[tokio::test]
async fn migrate_down_then_up() {
    for profile in PROFILES {
        let Some(db) = open_db("print_mig", profile).await else {
            continue;
        };
        datum_db::migrate::run(
            db.migrate_pool(),
            &[
                ("datum-db", &datum_db::MIGRATOR),
                ("datum-audit", &datum_audit::MIGRATOR),
            ],
        )
        .await
        .expect("kernel");
        let boot = db.bootstrap_pool().await.expect("bootstrap");
        datum_audit::install_privileged(&boot)
            .await
            .expect("install_privileged");
        boot.close().await;
        let migrator = print_migrator();
        migrator.run(db.migrate_pool()).await.expect("up");
        migrator.undo(db.migrate_pool(), 0).await.expect("down");
        let gone: bool = query_scalar("SELECT to_regclass('print.render_log') IS NULL")
            .fetch_one(db.migrate_pool())
            .await
            .expect("probe");
        assert!(gone);
        let class_gone: bool = query_scalar(
            "SELECT NOT EXISTS (SELECT 1 FROM datum.schema_class WHERE nspname = 'print')",
        )
        .fetch_one(db.migrate_pool())
        .await
        .expect("probe class");
        assert!(class_gone);
        migrator.run(db.migrate_pool()).await.expect("up again");
        let exists: bool = query_scalar("SELECT to_regclass('print.template') IS NOT NULL")
            .fetch_one(db.migrate_pool())
            .await
            .expect("probe");
        assert!(exists);
        db.finish().await.expect("finish");
    }
}
