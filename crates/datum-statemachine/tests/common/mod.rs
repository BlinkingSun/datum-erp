//! Shared helpers for commit-mode statemachine tests.

#![allow(dead_code, unused_imports)]

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use datum_core::{
    Actor, ActorKind, Boundary, GroupKind, Identifier, ItemId, LocationId, NoSignatures,
    PermissionKey, PostingError, PostingGroupHeader, PostingHandle, PostingIntent, PostingSink,
    QuantityPosting, SignatureError, SignatureGate, SignatureId, SignatureMeaning,
    SignatureRequirement, SignatureToken, UnitId,
};
use datum_core::{AnyQuantity, DimensionKind};
use datum_db::{WriteContext, WritePool};
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{PrincipalKind, create_principal, seed_builtins};
use datum_statemachine::{
    DocRef, EdgeBuilder, Engine, Machine, SignatureDeclaration, action_for, with_action,
};
use rust_decimal::Decimal;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, query as sql_query, query_scalar as sql_query_scalar};

pub fn rewrite_database(url: &str, database: &str) -> String {
    let Some((prefix, rest)) = url.rsplit_once('/') else {
        panic!("url has no database path: {url}");
    };
    let qs = rest
        .split_once('?')
        .map(|(_, q)| format!("?{q}"))
        .unwrap_or_default();
    format!("{prefix}/{database}{qs}")
}

fn url_has_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    rest.split(['/', '?']).next().unwrap_or("").contains('@')
}

pub async fn bootstrap_pool(database: &str) -> PgPool {
    let url = std::env::var("DATUM_BOOTSTRAP_URL").expect("DATUM_BOOTSTRAP_URL");
    let rewritten = rewrite_database(&url, database);
    let mut opts: PgConnectOptions = rewritten.parse().expect("bootstrap url");
    if !url_has_userinfo(&url)
        && let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("LOGNAME"))
    {
        opts = opts.username(&user);
    }
    PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opts)
        .await
        .expect("bootstrap pool")
}

/// Migrate db + audit, install event triggers, then identity + statemachine.
/// Privilege and schema statements live in migrations run as `datum_migrate`;
/// this crate does not issue them from Rust.
pub async fn migrate_and_install(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    let boot = bootstrap_pool(db.database()).await;
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-statemachine", &datum_statemachine::MIGRATOR),
        ],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate identity+sm: {e:#}"));
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
    ctx.reason = Some("statemachine-test".into());
    ctx
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub async fn count_audit(pool: &PgPool, table: &str, action: &str) -> i64 {
    sql_query_scalar("SELECT count(*) FROM audit.event WHERE table_name = $1 AND action = $2")
        .bind(table)
        .bind(action)
        .fetch_one(pool)
        .await
        .expect("audit count")
}

pub async fn instance_state(pool: &PgPool, doc: &DocRef) -> Option<String> {
    sql_query_scalar("SELECT state FROM sm.instance WHERE doc_type = $1 AND doc_id = $2")
        .bind(&doc.doc_type)
        .bind(doc.doc_id.as_uuid())
        .fetch_optional(pool)
        .await
        .expect("state")
}

/// Principal + role holding `permission`, and a WriteContext for `doc`/`edge`.
pub async fn actor_with_perm(
    write: &WritePool,
    permission: &str,
    doc: &DocRef,
    edge: &str,
) -> (Actor, WriteContext) {
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
            permissions: vec![permission.to_owned()],
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
    ctx.reason = Some("statemachine-test".into());
    let ctx = with_action(ctx, doc, edge);
    (actor, ctx)
}

pub fn draft_release_machine(required: bool) -> Machine {
    let mut edge = EdgeBuilder::new("Draft", "Released", "release", "wo.release");
    edge = if required {
        edge.required(SignatureRequirement {
            meaning: SignatureMeaning("Released".into()),
            permission: PermissionKey("wo.release".into()),
        })
    } else {
        edge.not_required("plain-shop; no signature on release")
    };
    Machine::builder("wo")
        .regulated(required)
        .state("Draft")
        .state("Released")
        .edge(edge)
        .build()
        .expect("machine")
}

pub fn dummy_intent() -> PostingIntent {
    PostingIntent::Quantity(QuantityPosting {
        item: ItemId::from_uuid(uuid::Uuid::nil()),
        quantity: AnyQuantity {
            amount: Decimal::from(1),
            unit: UnitId(1),
            dimension: DimensionKind::Count,
        },
        location: LocationId::from_uuid(uuid::Uuid::nil()),
        boundary: Some(Boundary::Adjustment),
        lot: None,
        serial: None,
        entered: None,
    })
}

pub struct CollectingSink {
    header: PostingGroupHeader,
    next: u32,
    pub contribs: Arc<Mutex<Vec<u32>>>,
    pub finalized: Arc<AtomicBool>,
}

impl CollectingSink {
    pub fn new() -> Self {
        Self {
            header: PostingGroupHeader {
                source_kind: "sm-test".into(),
                source_id: None,
                work_order_id: None,
                reason_code: Some("test".into()),
                reverses_group_id: None,
            },
            next: 0,
            contribs: Arc::new(Mutex::new(Vec::new())),
            finalized: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn into_box(self) -> Box<dyn PostingSink> {
        Box::new(self)
    }
}

impl PostingSink for CollectingSink {
    fn kind(&self) -> GroupKind {
        GroupKind::Adjustment
    }
    fn header(&self) -> &PostingGroupHeader {
        &self.header
    }
    fn contribute(
        &mut self,
        _intent: PostingIntent,
    ) -> core::result::Result<PostingHandle, PostingError> {
        let h = PostingHandle(self.next);
        self.next += 1;
        self.contribs.lock().expect("contribs").push(h.0);
        Ok(h)
    }
    fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError> {
        self.finalized.store(true, Ordering::SeqCst);
        Ok(())
    }
}

pub struct CountingGate {
    pub calls: AtomicUsize,
}

impl CountingGate {
    pub fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl SignatureGate for CountingGate {
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

pub fn dummy_token(doc: &DocRef, version: i64) -> SignatureToken {
    SignatureToken {
        signature: SignatureId::from_uuid(uuid::Uuid::nil()),
        signer: Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        meaning: SignatureMeaning("Released".into()),
        record: datum_core::RecordRef {
            table: "sm.instance".into(),
            id: doc.doc_id,
            version,
        },
        record_content_hash: [0; 32],
    }
}

pub fn _use_no_signatures() -> NoSignatures {
    NoSignatures
}

pub async fn persist_spawn(
    eng: &Engine,
    write: &WritePool,
    ctx: &WriteContext,
    doc: &DocRef,
) -> datum_statemachine::Instance {
    let mut tx = datum_db::Tx::begin(write, ctx).await.expect("begin");
    eng.persist(&mut tx).await.expect("persist");
    let inst = eng.spawn(&mut tx, doc, "Draft").await.expect("spawn");
    tx.commit().await.expect("commit spawn");
    inst
}

pub fn _action(doc: &DocRef, edge: &str) -> String {
    action_for(&doc.doc_type, edge)
}

pub fn _decl_name(d: &SignatureDeclaration) -> &'static str {
    match d {
        SignatureDeclaration::Required(_) => "required",
        SignatureDeclaration::NotRequired { .. } => "not_required",
    }
}

pub async fn table_count(pool: &PgPool, sql: &'static str) -> i64 {
    sql_query_scalar(sql).fetch_one(pool).await.expect("count")
}

pub async fn machine_id_for(pool: &PgPool, doc_type: &str) -> uuid::Uuid {
    sql_query_scalar("SELECT id FROM sm.machine WHERE doc_type = $1")
        .bind(doc_type)
        .fetch_one(pool)
        .await
        .expect("machine id")
}

pub async fn catalog_audit_count(pool: &PgPool) -> i64 {
    sql_query_scalar(
        "SELECT count(*) FROM audit.event WHERE table_name IN ('machine', 'state', 'edge')",
    )
    .fetch_one(pool)
    .await
    .expect("catalog audit")
}

pub async fn raw_insert_machine(pool: &PgPool) -> sqlx::Error {
    sql_query(
        r#"INSERT INTO sm.machine (id, doc_type, regulated)
           VALUES ($1, 'raw-write', false)"#,
    )
    .bind(Identifier::generate().as_uuid())
    .execute(pool)
    .await
    .expect_err("raw write must fail")
}
