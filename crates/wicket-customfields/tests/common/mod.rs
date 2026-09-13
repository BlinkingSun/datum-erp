#![allow(dead_code)]

use wicket_core::{Actor, ActorKind, Identifier};
use wicket_customfields::{DefinitionId, definition_machine, retire_context};
use wicket_db::{Tx, WriteContext, WritePool};
use wicket_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use wicket_statemachine::Engine;

pub const PROFILES: [&str; 2] = ["plain-shop", "regulated-device"];

/// Short suffix for [`wicket_test::db_case`] names (must be `&str`).
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
            id: Identifier::from_uuid(wicket_identity::SYSTEM_ID),
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

/// D-2b-10 order through `wicket-customfields`. Glue rider flips this to
/// `wicket_module::order::install_upto(..., "wicket-customfields")`.
/// TODO(2b-migorder-glue): replace this fallback with `install_upto`.
pub async fn migrate(db: &wicket_test::TestDb) {
    migrate_prefix_privileged(db).await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
            ("wicket-uom", &wicket_uom::MIGRATOR),
            ("wicket-events", &wicket_events::MIGRATOR),
            ("wicket-jobs", &wicket_jobs::MIGRATOR),
            ("wicket-ledger", &wicket_ledger::MIGRATOR),
            ("wicket-statemachine", &wicket_statemachine::MIGRATOR),
            ("wicket-esign", &wicket_esign::MIGRATOR),
            ("wicket-customfields", &wicket_customfields::MIGRATOR),
        ],
    )
    .await
    .expect("canonical through wicket-customfields");
}

/// Predecessors of `wicket-customfields` in D-2b-10 order, event trigger already up.
pub async fn migrate_predecessors(db: &wicket_test::TestDb) {
    migrate_prefix_privileged(db).await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
            ("wicket-uom", &wicket_uom::MIGRATOR),
            ("wicket-events", &wicket_events::MIGRATOR),
            ("wicket-jobs", &wicket_jobs::MIGRATOR),
            ("wicket-ledger", &wicket_ledger::MIGRATOR),
            ("wicket-statemachine", &wicket_statemachine::MIGRATOR),
            ("wicket-esign", &wicket_esign::MIGRATOR),
        ],
    )
    .await
    .expect("canonical predecessors of wicket-customfields");
}

/// Apply this crate last: predecessors with the trigger down, then privileged, then customfields.
pub async fn migrate_as_last_crate(db: &wicket_test::TestDb) {
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
            ("wicket-uom", &wicket_uom::MIGRATOR),
            ("wicket-events", &wicket_events::MIGRATOR),
            ("wicket-jobs", &wicket_jobs::MIGRATOR),
            ("wicket-ledger", &wicket_ledger::MIGRATOR),
            ("wicket-statemachine", &wicket_statemachine::MIGRATOR),
            ("wicket-esign", &wicket_esign::MIGRATOR),
        ],
    )
    .await
    .expect("predecessors with trigger down");
    install_privileged(db).await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[("wicket-customfields", &wicket_customfields::MIGRATOR)],
    )
    .await
    .expect("wicket-customfields last with audit_attach up");
}

pub async fn migrate_prefix_privileged(db: &wicket_test::TestDb) {
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    install_privileged(db).await;
}

pub async fn install_privileged(db: &wicket_test::TestDb) {
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    wicket_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
}

pub fn write_pool(db: &wicket_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

/// Engine with [`definition_machine`] registered and frozen (composition-root stand-in).
pub fn frozen_engine(profile: &str) -> Engine {
    let mut eng = Engine::new();
    eng.register_machine(definition_machine(profile).expect("machine"))
        .expect("register");
    eng.freeze().expect("freeze");
    eng
}

pub async fn persist_engine(write: &WritePool, eng: &Engine, profile: &str) {
    let mut tx = Tx::begin(write, &write_ctx("customfields.boot", profile))
        .await
        .expect("begin persist");
    eng.persist(&mut tx).await.expect("persist");
    tx.commit().await.expect("commit persist");
}

/// Grant `customfields.retire` to the built-in system principal.
pub async fn grant_retire_permission(write: &WritePool, profile: &str) {
    let mut tx = Tx::begin(write, &write_ctx("identity.rbac", profile))
        .await
        .expect("begin rbac");
    let roles = seed_bundles(
        &mut tx,
        &[RoleBundle {
            name: format!("cf_retire_{}", Identifier::generate()),
            permissions: vec!["customfields.retire".into()],
        }],
    )
    .await
    .expect("seed bundle");
    assign_role(
        &mut tx,
        wicket_identity::UserId(Identifier::from_uuid(wicket_identity::SYSTEM_ID)),
        roles[0],
    )
    .await
    .expect("assign");
    tx.commit().await.expect("commit rbac");
}

pub fn retire_write_ctx(profile: &str, id: DefinitionId) -> WriteContext {
    retire_context(write_ctx("pending", profile), id)
}

/// Open a case database named `{base}_{profile_suffix}`; skips when Postgres is absent.
pub async fn open_db(base: &str, profile: &str) -> Option<wicket_test::TestDb> {
    if std::env::var("WICKET_REQUIRE_PG").ok().as_deref() == Some("1") {
        wicket_test::require_postgres();
    } else if wicket_test::postgres_available().is_err() {
        return None;
    }
    let name = format!("{}_{}", base, profile_suffix(profile));
    Some(
        wicket_test::TestDb::case(&name)
            .await
            .unwrap_or_else(|e| panic!("test database: {e}")),
    )
}

pub fn pg_code(err: &wicket_db::Error) -> String {
    match err {
        wicket_db::Error::Sqlx(e) => e
            .as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("{e}")),
        other => other.to_string(),
    }
}
