//! Kernel wiring: migrate in `CANONICAL_ORDER`, register modules, `Kernel::build`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use sqlx::PgPool;
use uuid::Uuid;
use wicket_core::{Identifier, ItemId};
use wicket_db::{Pool, Tx, WritePool};
use wicket_documents::FsBlobStore;
use wicket_module::{
    ConfigurationManifest, Kernel, Profile, export_manifest, install_slice,
    startup_fails_if_required_meets_no_signatures,
};
use wicket_statemachine::{DocRef, Engine, Machine};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::session;

/// Process-start configuration failures. Both installation profiles require a
/// writable [`WICKET_BLOB_ROOT`](FsBlobStore::from_env); boot never falls back
/// to a temp directory.
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    /// `WICKET_BLOB_ROOT` is missing or empty.
    #[error("WICKET_BLOB_ROOT is not set")]
    BlobRootMissing,
    /// `WICKET_BLOB_ROOT` names a path that is not a writable directory.
    #[error("WICKET_BLOB_ROOT is not a writable directory ({path}): {message}")]
    BlobRootNotWritable {
        /// Configured path.
        path: String,
        /// OS error or reason.
        message: String,
    },
}

impl From<BootError> for Error {
    fn from(err: BootError) -> Self {
        Error::Config(err.to_string())
    }
}

/// Assembled HTTP process.
pub struct App {
    /// Composition root.
    pub kernel: Kernel,
    /// Bind address.
    pub bind: SocketAddr,
    /// Configuration-manifest content hash at boot.
    pub manifest_hash: String,
    /// Calibration certificate spawned when the regulated profile enables the
    /// compiled-in `mod-calibration` machine (item 7).
    pub calibration_doc: Option<Identifier>,
    /// App-role URL (fresh pools for nested block_on).
    pub database_url: String,
    blobs: FsBlobStore,
}

/// Test harness pin used by [`App::boot`] when `WICKET_BLOB_ROOT` is unset.
/// Edition 2024 makes `env::set_var` unsafe; workspace lints forbid `unsafe`.
static TEST_BLOB_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Pin a blob root for tests that boot through [`App::boot`] (IQ) without
/// mutating process environment. Does not override a set `WICKET_BLOB_ROOT`.
pub fn install_test_blob_root(root: PathBuf) {
    let _ = TEST_BLOB_ROOT.set(root);
}

fn blob_root_not_writable(path: &Path, message: impl std::fmt::Display) -> BootError {
    BootError::BlobRootNotWritable {
        path: path.display().to_string(),
        message: message.to_string(),
    }
}

fn open_writable_blob_root(root: &Path) -> Result<FsBlobStore> {
    let meta = std::fs::metadata(root).map_err(|e| blob_root_not_writable(root, e))?;
    if !meta.is_dir() {
        return Err(blob_root_not_writable(root, "not a directory").into());
    }
    let probe = root.join(format!(".wicket-write-probe-{}", Uuid::now_v7()));
    std::fs::write(&probe, b"").map_err(|e| blob_root_not_writable(root, e))?;
    let _ = std::fs::remove_file(&probe);
    Ok(FsBlobStore::new(root))
}

/// Resolve the process blob store. Required on both installation profiles.
/// Canonical name is `WICKET_BLOB_ROOT` ([`FsBlobStore::from_env`]); never a
/// per-boot temp directory.
fn compose_blob_store() -> Result<FsBlobStore> {
    match FsBlobStore::from_env() {
        Ok(store) => open_writable_blob_root(store.root()),
        Err(wicket_documents::Error::BlobRootMissing) => {
            if let Some(root) = TEST_BLOB_ROOT.get() {
                open_writable_blob_root(root)
            } else {
                Err(BootError::BlobRootMissing.into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

impl App {
    /// Migrate, register, freeze, seed locations, record the boot row.
    /// Requires a writable `WICKET_BLOB_ROOT` (both profiles) before any pool
    /// is opened; never substitutes a temp directory.
    pub async fn boot(cfg: Config) -> Result<Self> {
        let blobs = compose_blob_store()?;
        let migrate = wicket_db::connect(&cfg.migrate_url).await?;
        let app_pool = wicket_db::connect(&cfg.database_url).await?;
        let bootstrap = wicket_db::connect(&cfg.bootstrap_url).await?;
        let app = Self::boot_pools(
            cfg.profile,
            app_pool,
            &migrate,
            &bootstrap,
            cfg.bind,
            cfg.database_url.clone(),
            blobs,
        )
        .await?;
        bootstrap.close().await;
        migrate.close().await;
        Ok(app)
    }

    /// Boot against already-open pools (tests). `blobs` is the per-test root.
    pub async fn boot_pools(
        profile: Profile,
        app_pool: Pool,
        migrate: &PgPool,
        bootstrap: &PgPool,
        bind: SocketAddr,
        database_url: String,
        blobs: FsBlobStore,
    ) -> Result<Self> {
        // Same order as `install_upto(..., "wicket-server")` / `install_slice`.
        // `install_privileged` stays up for the whole install (D-2b-11).
        // Slice attach set is `SLICE_AUDIT_RELS` (belt-and-braces).
        adopt_wicket_db_history(migrate).await?;
        install_slice(migrate, bootstrap)
            .await
            .map_err(|e| Error::Config(format!("install slice: {e}")))?;

        let kernel = build_kernel(app_pool.clone(), profile).await?;
        let write = WritePool::new(app_pool.clone());
        let mut ctx = session::system_ctx("server.boot");
        ctx.config_version = Some(kernel.profile.spec_version.clone());
        let mut tx = Tx::begin(&write, &ctx).await?;
        wicket_mod_locations::seed_install(&mut tx).await?;
        wicket_mod_genealogy::wire(&kernel, &mut tx).await?;
        let calibration_doc = spawn_calibration_if_enabled(&kernel, &mut tx).await?;
        let manifest = export_manifest(kernel.pool()).await?;
        let boot_id = Uuid::now_v7();
        tx.execute(
            sqlx::query(
                r#"INSERT INTO server.boot_record
                       (id, profile_id, spec_version, manifest_hash, bind_addr,
                        application_version, configuration_version)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            )
            .bind(boot_id)
            .bind(kernel.profile.id.as_str())
            .bind(&kernel.profile.spec_version)
            .bind(&manifest.content_hash)
            .bind(bind.to_string())
            .bind(wicket_db::app_version())
            .bind(&kernel.profile.spec_version),
        )
        .await?;
        tx.commit().await?;
        Ok(Self {
            manifest_hash: manifest.content_hash,
            kernel,
            bind,
            calibration_doc,
            database_url,
            blobs,
        })
    }

    /// Shared state for axum. Blob store was composed at boot.
    pub fn state(self) -> AppState {
        AppState {
            inner: Arc::new(AppInner {
                kernel: self.kernel,
                bind: self.bind,
                manifest_hash: self.manifest_hash,
                calibration_doc: self.calibration_doc,
                database_url: self.database_url,
                blobs: Arc::new(self.blobs),
            }),
        }
    }
}

/// Axum state.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppInner>,
}

struct AppInner {
    kernel: Kernel,
    bind: SocketAddr,
    manifest_hash: String,
    calibration_doc: Option<Identifier>,
    database_url: String,
    blobs: Arc<FsBlobStore>,
}

impl AppState {
    /// Kernel handle.
    pub fn kernel(&self) -> &Kernel {
        &self.inner.kernel
    }

    /// App pool.
    pub fn pool(&self) -> &Pool {
        self.inner.kernel.pool()
    }

    /// Write pool.
    pub fn write_pool(&self) -> WritePool {
        self.inner.kernel.write_pool()
    }

    /// Manifest hash captured at boot.
    pub fn manifest_hash(&self) -> &str {
        &self.inner.manifest_hash
    }

    /// Bind address.
    pub fn bind(&self) -> SocketAddr {
        self.inner.bind
    }

    /// Calibration document for the regulated signature-refusal probe.
    pub fn calibration_doc(&self) -> Option<Identifier> {
        self.inner.calibration_doc
    }

    /// App-role URL.
    pub fn database_url(&self) -> &str {
        &self.inner.database_url
    }

    /// Composed [`FsBlobStore`] shared with documents attach / print archive.
    pub fn blobs(&self) -> &FsBlobStore {
        &self.inner.blobs
    }
}

/// Run inventory / production / genealogy / server migrators after the kernel.
/// Delegates to [`wicket_module::migrate_slice_modules`] (same migrator set).
pub async fn migrate_slice_modules(pool: &PgPool) -> Result<()> {
    wicket_module::migrate_slice_modules(pool)
        .await
        .map_err(|e| Error::Config(format!("migrate slice: {e}")))
}

/// `sqlx migrate run` records in `_sqlx_migrations`, not `wicket.schema_history`.
/// If `wicket.schema_history` already exists, re-applying wicket-db 0001 fails
/// with `relation "schema_history" already exists`. Record those versions first.
///
/// IQ on a standing DB (schema_history present, no second wicket-db migration run)
/// depends on this adopt plus idempotent `Kernel::build` / `Engine::persist`.
async fn adopt_wicket_db_history(pool: &PgPool) -> Result<()> {
    let exists: bool =
        sqlx::query_scalar("SELECT to_regclass('wicket.schema_history') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    if !exists {
        return Ok(());
    }
    let version = wicket_db::app_version();
    for migration in wicket_db::MIGRATOR.iter() {
        if migration.migration_type.is_down_migration() {
            continue;
        }
        sqlx::query(
            r#"INSERT INTO wicket.schema_history (crate, version, applied_at, app_version)
               VALUES ($1, $2, now(), $3)
               ON CONFLICT (crate, version) DO NOTHING"#,
        )
        .bind("wicket-db")
        .bind(migration.version)
        .bind(&version)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Register every Wave 2s module through `wicket-module` extension points.
pub async fn build_kernel(pool: Pool, profile: Profile) -> Result<Kernel> {
    let mut builder = Kernel::builder(pool, profile.clone());
    wicket_mod_items::register(&mut builder, &profile)?;
    builder.apply_manifest(&wicket_mod_locations::manifest()?)?;
    // ADDENDUM 1 item 5: lots are registered under the live profile so
    // `lot.release` is Required on regulated-device (refused under NoSignatures).
    wicket_mod_lots::register(&mut builder, &profile)?;
    wicket_mod_inventory::register(&mut builder, &profile)?;
    wicket_mod_production_min::register(&mut builder, &profile)?;
    wicket_mod_genealogy::register(&mut builder, &profile)?;
    Ok(builder.build().await?)
}

async fn spawn_calibration_if_enabled(
    kernel: &Kernel,
    tx: &mut Tx<'_>,
) -> Result<Option<Identifier>> {
    let enabled = kernel
        .profile
        .modules
        .iter()
        .any(|m| m.id == "mod-calibration" && m.enabled);
    if !enabled {
        return Ok(None);
    }
    let id = Identifier::generate();
    kernel
        .spawn(
            tx,
            &DocRef {
                doc_type: "calibration.certificate".into(),
                doc_id: id,
            },
            "Open",
        )
        .await?;
    Ok(Some(id))
}

/// Named-test helper: release + NoSignatures + a Required edge fails.
pub fn startup_guard_release(engine: &Engine, gate_is_noop: bool) -> Result<()> {
    Ok(startup_fails_if_required_meets_no_signatures(
        engine,
        gate_is_noop,
        true,
    )?)
}

/// IQ: boot, prove the audit schema exists, then the boot-row trigger + manifest hash.
pub async fn run_iq(cfg: Config) -> Result<(String, String)> {
    let app = App::boot(cfg).await?;
    let hash = app.manifest_hash.clone();
    let profile = app.kernel.profile.id.as_str().to_string();
    let pool = app.kernel.pool().clone();
    let audit_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'audit')")
            .fetch_one(&pool)
            .await?;
    if !audit_exists {
        return Err(Error::Config("schema \"audit\" does not exist".into()));
    }
    let attached: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
             SELECT 1 FROM pg_trigger t
             JOIN pg_class c ON c.oid = t.tgrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'server' AND c.relname = 'boot_record'
               AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
           )"#,
    )
    .fetch_one(&pool)
    .await?;
    if !attached {
        return Err(Error::Config("server.boot_record is not audited".into()));
    }
    let live = live_manifest(&pool).await?;
    live.verify_self()
        .map_err(|e| Error::Config(format!("manifest: {e}")))?;
    Ok((hash, profile))
}

/// IQ: configuration manifest currently stored.
pub async fn live_manifest(pool: &Pool) -> Result<ConfigurationManifest> {
    Ok(export_manifest(pool).await?)
}

/// Keep `Machine` named so clippy sees the calibration path is real.
pub fn _calibration_machine_name() -> &'static str {
    let _ = core::any::type_name::<Machine>();
    let _ = core::any::type_name::<ItemId>();
    "calibration.certificate"
}

#[cfg(test)]
mod blob_root_tests {
    use super::*;
    use std::process::Command;

    fn spawn_child(kind: &str, blob_root: Option<&str>) {
        let exe = std::env::current_exe().expect("current_exe");
        let mut cmd = Command::new(&exe);
        cmd.args(["boot_refuses_without_blob_root", "--exact", "--nocapture"]);
        cmd.env("WICKET_TEST_CHILD", kind);
        match blob_root {
            Some(root) => {
                cmd.env("WICKET_BLOB_ROOT", root);
            }
            None => {
                cmd.env_remove("WICKET_BLOB_ROOT");
            }
        }
        let out = cmd.output().expect("spawn child");
        assert!(
            out.status.success(),
            "child {kind} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn assert_named_config(err: &Error, needle: &str, profile: &str) {
        let msg = err.to_string();
        assert!(
            msg.contains(needle),
            "profile={profile} needle={needle:?} err={msg}"
        );
        assert!(
            matches!(err, Error::Config(_)),
            "profile={profile} expected BootError via Error::Config, got {err:?}"
        );
    }

    #[tokio::test]
    async fn boot_refuses_without_blob_root() {
        match std::env::var("WICKET_TEST_CHILD").ok().as_deref() {
            Some("missing") => {
                for profile in [
                    Profile::plain_shop().unwrap(),
                    Profile::regulated_device().unwrap(),
                ] {
                    let id = profile.id.as_str().to_string();
                    let err = compose_blob_store().expect_err("blob root required");
                    assert_named_config(&err, "WICKET_BLOB_ROOT is not set", &id);
                    let cfg = Config {
                        profile,
                        bind: "127.0.0.1:0".parse().expect("bind"),
                        database_url: "postgres://127.0.0.1:1/none".into(),
                        migrate_url: "postgres://127.0.0.1:1/none".into(),
                        bootstrap_url: "postgres://127.0.0.1:1/none".into(),
                    };
                    let Err(err) = App::boot(cfg).await else {
                        panic!("App::boot must refuse without blob root (profile={id})");
                    };
                    assert_named_config(&err, "WICKET_BLOB_ROOT is not set", &id);
                }
            }
            Some("notdir") => {
                for profile in [
                    Profile::plain_shop().unwrap(),
                    Profile::regulated_device().unwrap(),
                ] {
                    let id = profile.id.as_str().to_string();
                    let err = compose_blob_store().expect_err("blob root not a directory");
                    assert_named_config(&err, "WICKET_BLOB_ROOT is not a writable directory", &id);
                    let cfg = Config {
                        profile,
                        bind: "127.0.0.1:0".parse().expect("bind"),
                        database_url: "postgres://127.0.0.1:1/none".into(),
                        migrate_url: "postgres://127.0.0.1:1/none".into(),
                        bootstrap_url: "postgres://127.0.0.1:1/none".into(),
                    };
                    let Err(err) = App::boot(cfg).await else {
                        panic!("App::boot must refuse a non-directory blob root (profile={id})");
                    };
                    assert_named_config(&err, "WICKET_BLOB_ROOT is not a writable directory", &id);
                }
            }
            _ => {
                spawn_child("missing", None);
                let file =
                    std::env::temp_dir().join(format!("wicket-blob-not-dir-{}", Uuid::now_v7()));
                std::fs::write(&file, b"not-a-dir").expect("notdir file");
                spawn_child("notdir", Some(file.to_str().expect("utf8 path")));
                let _ = std::fs::remove_file(&file);
            }
        }
    }
}
