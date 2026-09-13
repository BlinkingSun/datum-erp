#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use datum_core::{
    Actor, ActorKind, Identifier, SignatureError, SignatureGate, SignatureId, SignatureMeaning,
    SignatureRequirement, SignatureToken,
};
use datum_db::{WriteContext, WritePool};
use datum_documents::{DOC_TYPE, FsBlobStore, document_machine};
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{PrincipalKind, create_principal, seed_builtins};
use datum_statemachine::{DocRef, Engine};

pub const PROFILES: [&str; 2] = ["plain-shop", "regulated-device"];

pub fn profile_suffix(profile: &str) -> &'static str {
    match profile {
        "plain-shop" => "plain",
        "regulated-device" => "regulated",
        _ => "other",
    }
}

pub async fn migrate(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
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
            ("datum-documents", &datum_documents::MIGRATOR),
        ],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate identity+numbering+sm+documents: {e:#}"));
    let write = WritePool::new(db.app_pool().clone());
    let mut tx = datum_db::Tx::begin(&write, &system_ctx("identity.seed"))
        .await
        .expect("seed begin");
    seed_builtins(&mut tx).await.expect("seed_builtins");
    tx.commit().await.expect("seed commit");
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
    ctx.reason = Some("documents-test".into());
    ctx
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
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
    ctx.reason = Some("documents integration test".into());
    ctx.config_version = Some(profile.into());
    ctx
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
            .unwrap_or_else(|e| panic!("test database: {e}")),
    )
}

pub fn blob_store(tag: &str) -> FsBlobStore {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "datum-doc-blobs-{}-{}",
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

pub async fn persist_engine(write: &WritePool, eng: &Engine, profile: &str) {
    let mut tx = datum_db::Tx::begin(write, &write_ctx("documents.boot", profile))
        .await
        .expect("begin persist");
    eng.persist(&mut tx).await.expect("persist");
    tx.commit().await.expect("commit persist");
}

/// Principal holding every documents.* permission.
pub async fn actor_with_docs(write: &WritePool, profile: &str) -> (Actor, WriteContext) {
    let slug = Identifier::generate().to_string();
    let short: String = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let mut tx = datum_db::Tx::begin(write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(
        &mut tx,
        PrincipalKind::User,
        &format!("u{short}"),
        "Operator",
    )
    .await
    .expect("principal");
    let roles = seed_bundles(
        &mut tx,
        &[RoleBundle {
            name: format!("r{short}"),
            permissions: vec![
                "documents.view".into(),
                "documents.edit".into(),
                "documents.approve".into(),
                "documents.release".into(),
            ],
        }],
    )
    .await
    .expect("role");
    assign_role(&mut tx, p.id, roles[0]).await.expect("assign");
    tx.commit().await.expect("commit actor");
    let actor = Actor {
        id: p.id.0,
        kind: ActorKind::User,
    };
    let mut ctx = WriteContext::new(actor, "pending", "ui");
    ctx.actor_display = Some("Operator".into());
    ctx.reason = Some("documents-test".into());
    ctx.config_version = Some(profile.into());
    (actor, ctx)
}

pub fn doc_ref(id: datum_documents::DocumentId) -> DocRef {
    DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: id.0,
    }
}

pub fn dummy_token(
    doc: datum_documents::DocumentId,
    meaning: &str,
    version: i64,
) -> SignatureToken {
    SignatureToken {
        signature: SignatureId::generate(),
        signer: Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        meaning: SignatureMeaning(meaning.into()),
        record: datum_core::RecordRef {
            table: "sm.instance".into(),
            id: doc.0,
            version,
        },
        record_content_hash: [0; 32],
    }
}

/// Gate that accepts every token (regulated happy-path tests).
pub struct AcceptingGate {
    pub calls: AtomicUsize,
}

impl AcceptingGate {
    pub fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl SignatureGate for AcceptingGate {
    fn verify(
        &self,
        _token: &SignatureToken,
        _required: &SignatureRequirement,
        _record: &datum_core::RecordRef,
    ) -> core::result::Result<(), SignatureError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub fn db_sqlstate(err: &datum_db::Error) -> String {
    match err {
        datum_db::Error::Refused(state) => state.as_str().to_owned(),
        datum_db::Error::Sqlx(e) => pg_code(e),
        other => other.to_string(),
    }
}
