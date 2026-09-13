#![allow(dead_code)]

use std::path::PathBuf;

use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::{ReadPool, WriteContext, WritePool};
use wicket_documents::{FsBlobStore, document_machine};
use wicket_identity::seed_builtins;
use wicket_print::{seed_templates, set_installation_profile};
use wicket_statemachine::Engine;

pub const PROFILES: [&str; 2] = ["plain-shop", "regulated-device"];

/// Production composition root stamps `config_version` from `profile.spec_version`.
pub const SPEC_VERSION: &str = "1.0.0";

pub fn profile_suffix(profile: &str) -> &'static str {
    match profile {
        "plain-shop" => "plain",
        "regulated-device" => "regulated",
        _ => "other",
    }
}

pub async fn migrate(db: &wicket_test::TestDb, profile: &str) {
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
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
            ("wicket-statemachine", &wicket_statemachine::MIGRATOR),
            ("wicket-esign", &wicket_esign::MIGRATOR),
            ("wicket-documents", &wicket_documents::MIGRATOR),
            ("wicket-print", &wicket_print::MIGRATOR),
        ],
    )
    .await
    .expect("app migrators");
    let write = WritePool::new(db.app_pool().clone());
    let mut tx = wicket_db::Tx::begin(&write, &system_ctx("identity.seed"))
        .await
        .expect("seed");
    seed_builtins(&mut tx).await.expect("builtins");
    seed_templates(&mut tx).await.expect("print templates");
    set_installation_profile(&mut tx, profile)
        .await
        .expect("print install profile");
    tx.commit().await.expect("commit");
}

pub fn system_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(wicket_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        action,
        "api",
    );
    ctx.actor_display = Some("system".into());
    ctx.config_version = Some(SPEC_VERSION.into());
    ctx
}

pub fn write_ctx(action: &str) -> WriteContext {
    system_ctx(action)
}

pub fn write_pool(db: &wicket_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn read_pool(db: &wicket_test::TestDb) -> ReadPool {
    ReadPool::new(db.app_pool().clone())
}

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
            .unwrap_or_else(|e| panic!("db: {e}")),
    )
}

/// Unique filesystem blob store for this test. No process-global root.
pub fn blob_store(tag: &str) -> FsBlobStore {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "wicket-print-blobs-{}-{}",
        tag,
        Identifier::generate()
    ));
    std::fs::create_dir_all(&root).expect("blob root");
    FsBlobStore::new(root)
}

pub fn frozen_engine(profile: &str) -> Engine {
    let mut eng = Engine::new();
    eng.register_machine(document_machine(profile).expect("machine"))
        .expect("register");
    eng.freeze().expect("freeze");
    eng
}

pub async fn persist_engine(write: &WritePool, eng: &Engine) {
    let mut tx = wicket_db::Tx::begin(write, &write_ctx("sm.persist"))
        .await
        .expect("begin");
    eng.persist(&mut tx).await.expect("persist");
    tx.commit().await.expect("commit");
}
