#![allow(dead_code)]

use std::path::PathBuf;

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{WriteContext, WritePool};
use datum_documents::{FsBlobStore, document_machine};
use datum_identity::seed_builtins;
use datum_print::{seed_templates, set_installation_profile};
use datum_statemachine::Engine;

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

pub async fn migrate(db: &datum_test::TestDb, profile: &str) {
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
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
            ("datum-statemachine", &datum_statemachine::MIGRATOR),
            ("datum-esign", &datum_esign::MIGRATOR),
            ("datum-documents", &datum_documents::MIGRATOR),
            ("datum-print", &datum_print::MIGRATOR),
        ],
    )
    .await
    .expect("app migrators");
    let write = WritePool::new(db.app_pool().clone());
    let mut tx = datum_db::Tx::begin(&write, &system_ctx("identity.seed"))
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
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
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

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

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
            .unwrap_or_else(|e| panic!("db: {e}")),
    )
}

pub fn blob_store(tag: &str) -> FsBlobStore {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "datum-print-blobs-{}-{}",
        tag,
        Identifier::generate()
    ));
    std::fs::create_dir_all(&root).expect("blob root");
    datum_print::set_test_blob_root(root.clone());
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
    let mut tx = datum_db::Tx::begin(write, &write_ctx("sm.persist"))
        .await
        .expect("begin");
    eng.persist(&mut tx).await.expect("persist");
    tx.commit().await.expect("commit");
}
