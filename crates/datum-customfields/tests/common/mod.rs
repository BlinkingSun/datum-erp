#![allow(dead_code)]

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{WriteContext, WritePool};

pub const PROFILES: [&str; 2] = ["plain-shop", "regulated-device"];

/// Short suffix for [`datum_test::db_case`] names (must be `&str`).
pub fn profile_suffix(profile: &str) -> &'static str {
    match profile {
        "plain-shop" => "plain",
        "regulated-device" => "regulated",
        _ => "other",
    }
}

pub fn write_ctx(action: &str, profile: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        action,
        "ui",
    );
    ctx.actor_display = Some("test".into());
    ctx.reason = Some("customfields integration test".into());
    ctx.config_version = Some(profile.into());
    ctx
}

/// D-2b-10 order through `datum-customfields`. Glue rider flips this to
/// `datum_module::order::install_upto(..., "datum-customfields")`.
/// TODO(2b-migorder-glue): replace this fallback with `install_upto`.
pub async fn migrate(db: &datum_test::TestDb) {
    migrate_prefix_privileged(db).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
            ("datum-uom", &datum_uom::MIGRATOR),
            ("datum-events", &datum_events::MIGRATOR),
            ("datum-jobs", &datum_jobs::MIGRATOR),
            ("datum-ledger", &datum_ledger::MIGRATOR),
            ("datum-statemachine", &datum_statemachine::MIGRATOR),
            ("datum-esign", &datum_esign::MIGRATOR),
            ("datum-customfields", &datum_customfields::MIGRATOR),
        ],
    )
    .await
    .expect("canonical through datum-customfields");
}

/// Predecessors of `datum-customfields` in D-2b-10 order, event trigger already up.
pub async fn migrate_predecessors(db: &datum_test::TestDb) {
    migrate_prefix_privileged(db).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
            ("datum-uom", &datum_uom::MIGRATOR),
            ("datum-events", &datum_events::MIGRATOR),
            ("datum-jobs", &datum_jobs::MIGRATOR),
            ("datum-ledger", &datum_ledger::MIGRATOR),
            ("datum-statemachine", &datum_statemachine::MIGRATOR),
            ("datum-esign", &datum_esign::MIGRATOR),
        ],
    )
    .await
    .expect("canonical predecessors of datum-customfields");
}

/// Apply this crate last: predecessors with the trigger down, then privileged, then customfields.
pub async fn migrate_as_last_crate(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
            ("datum-uom", &datum_uom::MIGRATOR),
            ("datum-events", &datum_events::MIGRATOR),
            ("datum-jobs", &datum_jobs::MIGRATOR),
            ("datum-ledger", &datum_ledger::MIGRATOR),
            ("datum-statemachine", &datum_statemachine::MIGRATOR),
            ("datum-esign", &datum_esign::MIGRATOR),
        ],
    )
    .await
    .expect("predecessors with trigger down");
    install_privileged(db).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[("datum-customfields", &datum_customfields::MIGRATOR)],
    )
    .await
    .expect("datum-customfields last with audit_attach up");
}

pub async fn migrate_prefix_privileged(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    install_privileged(db).await;
}

pub async fn install_privileged(db: &datum_test::TestDb) {
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

/// Open a case database named `{base}_{profile_suffix}`; skips when Postgres is absent.
pub async fn open_db(base: &str, profile: &str) -> Option<datum_test::TestDb> {
    if std::env::var("DATUM_REQUIRE_PG").ok().as_deref() == Some("1") {
        datum_test::require_postgres();
    } else if datum_test::postgres_available().is_err() {
        return None;
    }
    let name = format!("{}_{}", base, profile_suffix(profile));
    Some(
        datum_test::TestDb::case(&name)
            .await
            .unwrap_or_else(|e| panic!("test database: {e}")),
    )
}

pub fn pg_code(err: &datum_db::Error) -> String {
    match err {
        datum_db::Error::Sqlx(e) => e
            .as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("{e}")),
        other => other.to_string(),
    }
}
