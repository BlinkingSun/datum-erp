#![allow(dead_code, unused_imports)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use datum_core::{
    Actor, ActorKind, Identifier, PermissionKey, RecordRef, SignatureMeaning, SignatureRequirement,
};
use datum_db::{WriteContext, WritePool};
use datum_esign::{InstanceTriple, LiveDoc, MintRequest, SessionPolicy, Signature};
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{
    Principal, PrincipalKind, create_principal, seed_builtins, set_login_credential,
    set_signing_credential,
};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::{query as sql_query, query_as as sql_query_as, query_scalar as sql_query_scalar};
use uuid::Uuid;

pub const SIGNING_SECRET: &str = "signing-secret-ok";
pub const LOGIN_SECRET: &str = "login-secret-ok";
pub const PERM: &str = "wo.release";
pub const ZONE: &str = "America/New_York";

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
    ctx.reason = Some("esign-test".into());
    ctx
}

pub fn user_ctx(principal: &Principal, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(principal.actor(), action, "ui");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("esign-test".into());
    ctx
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub async fn migrate_esign(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-esign", &datum_esign::MIGRATOR),
        ],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate identity+esign: {e:#}"));
    let write = WritePool::new(db.app_pool().clone());
    let mut tx = datum_db::Tx::begin(&write, &system_ctx("identity.seed"))
        .await
        .expect("seed begin");
    seed_builtins(&mut tx).await.expect("seed_builtins");
    tx.commit().await.expect("seed commit");
}

pub async fn migrate_esign_sm(db: &datum_test::TestDb) {
    migrate_esign(db).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[("datum-statemachine", &datum_statemachine::MIGRATOR)],
    )
    .await
    .unwrap_or_else(|e| panic!("migrate sm: {e:#}"));
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub async fn signer_with_perm(write: &WritePool, username: &str, perm: &str) -> Principal {
    let mut tx = datum_db::Tx::begin(write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, username, "M. Reyes")
        .await
        .expect("create");
    set_login_credential(&mut tx, p.id, LOGIN_SECRET)
        .await
        .expect("login cred");
    set_signing_credential(&mut tx, p.id, SIGNING_SECRET)
        .await
        .expect("signing cred");
    let roles = seed_bundles(
        &mut tx,
        &[RoleBundle {
            name: format!("r-{username}"),
            permissions: vec![perm.to_owned()],
        }],
    )
    .await
    .expect("role");
    assign_role(&mut tx, p.id, roles[0]).await.expect("assign");
    tx.commit().await.expect("commit actor");
    p
}

pub fn record(doc_id: Identifier, version: i64) -> RecordRef {
    RecordRef {
        table: "sm.instance".into(),
        id: doc_id,
        version,
    }
}

pub fn instance(doc_id: Identifier, version: i64, state: &str) -> InstanceTriple {
    InstanceTriple {
        doc_type: "wo".into(),
        doc_id,
        state: state.into(),
        version,
    }
}

pub fn mint_req(
    principal: Principal,
    body: Value,
    rec: RecordRef,
    inst: InstanceTriple,
    components: Vec<String>,
    policy: SessionPolicy,
) -> MintRequest {
    let code = if components.iter().any(|c| c == "code") {
        Some(principal.username.clone())
    } else {
        None
    };
    MintRequest {
        components,
        code,
        secret: SIGNING_SECRET.into(),
        meaning: SignatureMeaning("Released".into()),
        reason: None,
        record: rec,
        doc_type: "wo".into(),
        projection: body,
        instance: inst,
        permission: PermissionKey(PERM.into()),
        signed_at_zone: ZONE.into(),
        policy,
        principal,
        login_session_id: None,
        device_fingerprint: Some("tablet-1".into()),
        source_ip: Some("127.0.0.1".into()),
        boot_epoch: "1".into(),
        credential_kind: "signing_password".into(),
    }
}

pub fn two_components() -> Vec<String> {
    vec!["code".into(), "secret".into()]
}

pub fn secret_only() -> Vec<String> {
    vec!["secret".into()]
}

pub fn live_doc(
    sig: &Signature,
    body: Value,
    inst: InstanceTriple,
    principal: &Principal,
) -> LiveDoc {
    LiveDoc {
        record: sig.record.clone(),
        doc_type: sig.doc_type.clone(),
        projection: body,
        instance: inst,
        signer_status: principal.status,
    }
}

pub fn required() -> SignatureRequirement {
    SignatureRequirement {
        meaning: SignatureMeaning("Released".into()),
        permission: PermissionKey(PERM.into()),
    }
}

pub async fn count_audit(pool: &PgPool, table: &str) -> i64 {
    sql_query_scalar("SELECT count(*) FROM audit.event WHERE table_name = $1")
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("audit count")
}

pub async fn count_security(pool: &PgPool) -> i64 {
    sql_query_scalar(
        r#"SELECT count(*) FROM audit.event
            WHERE source_kind = 'app_event' AND action LIKE 'security.%'
               OR table_name IS NULL AND action LIKE 'esign.%'"#,
    )
    .fetch_one(pool)
    .await
    .expect("security count")
}

/// A shipped (or test-variant) profile's session policy, parsed from TOML.
#[derive(Debug, Clone)]
pub struct ProfileCase {
    /// Short slug for database names.
    pub slug: &'static str,
    /// Profile file id (`plain-shop`, `regulated-device`, …).
    pub id: &'static str,
    /// `[signature_gate_binding]` sub-keys.
    pub policy: SessionPolicy,
}

/// Both shipped profile TOMLs (SPEC-profiles key 4). Missing sub-keys default off/300/900.
pub fn both_profiles() -> [ProfileCase; 2] {
    [
        parse_profile(
            "ps",
            "plain-shop",
            include_str!("../../../../profiles/plain-shop.toml"),
        ),
        parse_profile(
            "rd",
            "regulated-device",
            include_str!("../../../../profiles/regulated-device.toml"),
        ),
    ]
}

/// Relaxation-on variant for `one_component_continuation_accepted_when_on` only.
pub fn relaxation_on_profile() -> ProfileCase {
    parse_profile(
        "on",
        "regulated-device-on",
        r#"
[signature_gate_binding]
gate = "datum-esign"
continuous_session = "on"
idle_timeout_secs = 300
max_window_secs = 900
"#,
    )
}

pub fn parse_profile(slug: &'static str, id: &'static str, toml: &str) -> ProfileCase {
    let mut policy = SessionPolicy::default();
    let mut in_gate = false;
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_gate = line.starts_with("[signature_gate_binding]");
            continue;
        }
        if !in_gate || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_matches('"');
        match k {
            "continuous_session" => policy.continuous_session = v.to_owned(),
            "idle_timeout_secs" => {
                if let Ok(n) = v.parse() {
                    policy.idle_timeout_secs = n;
                }
            }
            "max_window_secs" => {
                if let Ok(n) = v.parse() {
                    policy.max_window_secs = n;
                }
            }
            _ => {}
        }
    }
    ProfileCase { slug, id, policy }
}
