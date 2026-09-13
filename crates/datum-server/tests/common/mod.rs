//! Shared boot and HTTP helpers for server-slice tests.

#![allow(dead_code, unused_imports, clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{Tx, WriteContext, WritePool};
use datum_documents::FsBlobStore;
use datum_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use datum_identity::{
    PrincipalKind, create_principal, set_login_credential, set_signing_credential,
};
use datum_module::{Profile, ProfileId};
use datum_server::{App, AppState, install_test_blob_root, router};
use serde_json::{Value, json};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Once;
use tower::ServiceExt;
use uuid::Uuid;

fn rewrite_db(url: &str, database: &str) -> String {
    let (head, query) = url.split_once('?').unwrap_or((url, ""));
    let prefix = head.rsplit_once('/').map(|(p, _)| p).unwrap_or(head);
    if query.is_empty() {
        format!("{prefix}/{database}")
    } else {
        format!("{prefix}/{database}?{query}")
    }
}

pub const PASSWORD: &str = "reyes-login";
pub const USERNAME: &str = "mreyes";
pub const SIGNING_SECRET: &str = "signing-secret-ok";
pub const NOPERM_USER: &str = "noperm";
pub const NOPERM_PASSWORD: &str = "noperm-login";

/// Per-test filesystem blob root. Removed on drop.
pub struct BlobRoot {
    path: PathBuf,
}

impl Drop for BlobRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl BlobRoot {
    /// Unique writable directory under the test harness temp dir.
    pub fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("datum-server-blobs-{}-{}", tag, Uuid::now_v7()));
        std::fs::create_dir_all(&path).expect("per-test blob root");
        Self { path }
    }

    /// Configured path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Store rooted at this directory.
    pub fn store(&self) -> FsBlobStore {
        FsBlobStore::new(self.path.clone())
    }
}

fn pin_process_blob_root_for_app_boot() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let path =
            std::env::temp_dir().join(format!("datum-server-blobs-process-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("process blob root");
        install_test_blob_root(path);
    });
}

/// Skip cleanly when Postgres is absent and `DATUM_REQUIRE_PG` is not set.
pub fn skip_if_no_pg() -> bool {
    pin_process_blob_root_for_app_boot();
    if std::env::var("DATUM_REQUIRE_PG").ok().as_deref() == Some("1") {
        datum_test::require_postgres();
        return false;
    }
    if let Err(reason) = datum_test::postgres_available() {
        eprintln!("skip: postgres unavailable: {reason}");
        return true;
    }
    false
}

pub struct World {
    pub app: Router,
    pub pool: datum_db::Pool,
    pub profile: ProfileId,
    pub csrf: String,
    pub cookie: String,
    pub calibration_doc: Option<String>,
    pub state: AppState,
    _db: datum_test::TestDb,
    _blob_root: BlobRoot,
}

pub async fn boot(profile: Profile) -> World {
    if skip_if_no_pg() {
        panic!("boot() called without postgres; tests must call skip_if_no_pg first");
    }
    let db = datum_test::TestDb::case("srv")
        .await
        .unwrap_or_else(|e| panic!("test database: {e}"));
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let base = std::env::var("DATUM_DATABASE_URL").expect("DATUM_DATABASE_URL");
    let app_url = rewrite_db(&base, db.database());
    let blob_root = BlobRoot::new("srv");
    let app = App::boot_pools(
        profile.clone(),
        db.app_pool().clone(),
        db.migrate_pool(),
        &boot,
        bind,
        app_url,
        blob_root.store(),
    )
    .await
    .unwrap_or_else(|e| panic!("boot: {e:#}"));
    let calibration_doc = app.calibration_doc.map(|i| i.to_string());
    let pool = app.kernel.pool().clone();
    seed_operator(&pool, &profile).await;
    let state = app.state();
    let router = router(state.clone());
    let mut world = World {
        app: router,
        pool,
        profile: profile.id,
        csrf: String::new(),
        cookie: String::new(),
        calibration_doc,
        state,
        _db: db,
        _blob_root: blob_root,
    };
    world.login().await;
    boot.close().await;
    world
}

async fn seed_operator(pool: &datum_db::Pool, profile: &Profile) {
    let write = WritePool::new(pool.clone());
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "server.test.seed",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.config_version = Some(profile.spec_version.clone());
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin seed");
    let p = create_principal(&mut tx, PrincipalKind::User, USERNAME, "M. Reyes")
        .await
        .expect("principal");
    set_login_credential(&mut tx, p.id, PASSWORD)
        .await
        .expect("password");
    set_signing_credential(&mut tx, p.id, SIGNING_SECRET)
        .await
        .expect("signing cred");
    let roles = seed_bundles(
        &mut tx,
        &[RoleBundle {
            name: "slice-operator".into(),
            permissions: vec![
                "identity.session".into(),
                "items.view".into(),
                "items.edit".into(),
                "items.release".into(),
                "locations.view".into(),
                "locations.edit".into(),
                "lots.view".into(),
                "lots.edit".into(),
                "lots.release".into(),
                "inventory.view".into(),
                "inventory.receive".into(),
                "inventory.issue".into(),
                "inventory.move".into(),
                "inventory.count".into(),
                "inventory.adjust".into(),
                "production.view".into(),
                "production.create".into(),
                "production.release".into(),
                "production.issue".into(),
                "production.complete".into(),
                "genealogy.view".into(),
                "validation.manifest.read".into(),
                "audit.export".into(),
                "wo.release".into(),
                "calibration.approve".into(),
                "esign.bundle.read".into(),
                "customfields.define".into(),
                "customfields.retire".into(),
                "customfields.view".into(),
                "customfields.set".into(),
                "documents.view".into(),
                "documents.edit".into(),
                "documents.approve".into(),
                "documents.release".into(),
                "print.render".into(),
                "print.archive".into(),
                "print.templates".into(),
            ],
        }],
    )
    .await
    .expect("role");
    assign_role(&mut tx, p.id, roles[0]).await.expect("assign");
    let noperm = create_principal(&mut tx, PrincipalKind::User, NOPERM_USER, "No Perm")
        .await
        .expect("noperm principal");
    set_login_credential(&mut tx, noperm.id, NOPERM_PASSWORD)
        .await
        .expect("noperm password");
    let viewer = seed_bundles(
        &mut tx,
        &[RoleBundle {
            name: "slice-session-only".into(),
            permissions: vec!["identity.session".into()],
        }],
    )
    .await
    .expect("session-only role");
    assign_role(&mut tx, noperm.id, viewer[0])
        .await
        .expect("assign noperm");
    tx.commit().await.expect("commit seed");
}

impl World {
    pub async fn login(&mut self) {
        let (status, headers, body) = self
            .call(
                "POST",
                "/api/v1/identity/login",
                None,
                Some(json!({"username": USERNAME, "password": PASSWORD})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "login {body}");
        for val in headers.get_all("set-cookie") {
            let s = val.to_str().unwrap_or("");
            if let Some(rest) = s.strip_prefix("datum_session=") {
                self.cookie = rest.split(';').next().unwrap_or("").to_string();
            }
            if let Some(rest) = s.strip_prefix("datum_csrf=") {
                self.csrf = rest.split(';').next().unwrap_or("").to_string();
            }
        }
        if self.csrf.is_empty()
            && let Some(c) = body.get("csrf").and_then(Value::as_str)
        {
            self.csrf = c.to_string();
        }
        assert!(!self.cookie.is_empty(), "session cookie");
        assert!(!self.csrf.is_empty(), "csrf");
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        extra: Option<Vec<(&'static str, String)>>,
        json_body: Option<Value>,
    ) -> (StatusCode, axum::http::HeaderMap, Value) {
        let mut b = Request::builder().method(method).uri(uri);
        if !self.cookie.is_empty() {
            b = b.header(
                "cookie",
                format!("datum_session={}; datum_csrf={}", self.cookie, self.csrf),
            );
            b = b.header("x-csrf-token", &self.csrf);
        }
        if method == "POST" || method == "PATCH" {
            let has_key = extra.as_ref().is_some_and(|hs| {
                hs.iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("idempotency-key"))
            });
            if !has_key {
                b = b.header("idempotency-key", Uuid::now_v7().to_string());
            }
            b = b.header("content-type", "application/json");
        }
        if let Some(hs) = extra {
            for (k, v) in hs {
                b = b.header(k, v);
            }
        }
        let body = match json_body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        };
        let req = b.body(body).expect("request");
        let resp = self.app.clone().oneshot(req).await.expect("oneshot");
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .expect("bytes");
        let val = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        (status, headers, val)
    }

    pub async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        let (s, _, v) = self.call("POST", uri, None, Some(body)).await;
        (s, v)
    }

    pub async fn post_if_match(&self, uri: &str, body: Value, version: i64) -> (StatusCode, Value) {
        let (s, _, v) = self
            .call(
                "POST",
                uri,
                Some(vec![("if-match", format!("\"{version}\""))]),
                Some(body),
            )
            .await;
        (s, v)
    }

    pub async fn post_key(
        &self,
        uri: &str,
        body: Value,
        key: &str,
        extra: Vec<(&'static str, String)>,
    ) -> (StatusCode, Value) {
        let mut hs = extra;
        hs.push(("idempotency-key", key.to_string()));
        let (s, _, v) = self.call("POST", uri, Some(hs), Some(body)).await;
        (s, v)
    }

    pub async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let (s, _, v) = self.call("GET", uri, None, None).await;
        (s, v)
    }

    pub async fn login_as(&mut self, username: &str, password: &str) {
        self.cookie.clear();
        self.csrf.clear();
        let (status, headers, body) = self
            .call(
                "POST",
                "/api/v1/identity/login",
                None,
                Some(json!({"username": username, "password": password})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "login {username} {body}");
        for val in headers.get_all("set-cookie") {
            let s = val.to_str().unwrap_or("");
            if let Some(rest) = s.strip_prefix("datum_session=") {
                self.cookie = rest.split(';').next().unwrap_or("").to_string();
            }
            if let Some(rest) = s.strip_prefix("datum_csrf=") {
                self.csrf = rest.split(';').next().unwrap_or("").to_string();
            }
        }
        if self.csrf.is_empty()
            && let Some(c) = body.get("csrf").and_then(Value::as_str)
        {
            self.csrf = c.to_string();
        }
    }
}

pub fn pass(item: u8, detail: &str) {
    println!("PASS {item}: {detail}");
}

pub fn qty(amount: &str, unit: i64, dim: &str) -> Value {
    json!({"amount": amount, "unit": unit, "dimension": dim})
}
