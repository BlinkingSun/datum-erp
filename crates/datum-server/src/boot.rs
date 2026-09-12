//! Kernel wiring: migrate in `KERNEL_ORDER`, register modules, `Kernel::build`.

use std::net::SocketAddr;
use std::sync::Arc;

use datum_core::{Identifier, ItemId};
use datum_db::{Pool, Tx, WritePool};
use datum_module::{
    ConfigurationManifest, Kernel, Profile, attach_kernel_audit, export_manifest, kernel_migrators,
    migrate_prefix, migrate_wave_2s1_modules, startup_fails_if_required_meets_no_signatures,
};
use datum_statemachine::{DocRef, Engine, Machine};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::session;

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
}

impl App {
    /// Migrate, register, freeze, seed locations, record the boot row.
    pub async fn boot(cfg: Config) -> Result<Self> {
        let migrate = datum_db::connect(&cfg.migrate_url).await?;
        let app_pool = datum_db::connect(&cfg.database_url).await?;
        let bootstrap = datum_db::connect(&cfg.bootstrap_url).await?;
        let app = Self::boot_pools(
            cfg.profile,
            app_pool,
            &migrate,
            &bootstrap,
            cfg.bind,
            cfg.database_url.clone(),
        )
        .await?;
        bootstrap.close().await;
        migrate.close().await;
        Ok(app)
    }

    /// Boot against already-open pools (tests).
    pub async fn boot_pools(
        profile: Profile,
        app_pool: Pool,
        migrate: &PgPool,
        bootstrap: &PgPool,
        bind: SocketAddr,
        database_url: String,
    ) -> Result<Self> {
        // Same order as `install_kernel`, with Wave 2s slice migrators applied
        // *before* `attach_kernel_audit` so `datum.schema_history` inserts are
        // not judged by `zz_audit_row` (42501).
        adopt_datum_db_history(migrate).await?;
        migrate_prefix(migrate).await?;
        datum_audit::install_privileged(bootstrap).await?;
        datum_db::migrate::run(migrate, &[("datum-identity", &datum_identity::MIGRATOR)])
            .await
            .map_err(|e| Error::Config(format!("migrate identity: {e}")))?;
        datum_audit::uninstall_privileged(bootstrap).await?;
        for (name, migrator) in kernel_migrators() {
            if matches!(name, "datum-db" | "datum-audit" | "datum-identity") {
                continue;
            }
            datum_db::migrate::run(migrate, &[(name, migrator)])
                .await
                .map_err(|e| Error::Config(format!("migrate {name}: {e}")))?;
        }
        migrate_wave_2s1_modules(migrate).await?;
        migrate_slice_modules(migrate).await?;
        attach_kernel_audit(migrate).await?;
        for rel in [
            "inventory.document",
            "inventory.document_line",
            "production_min.work_order",
            "production_min.issue_line",
            "production_min.completion",
            "server.boot_record",
        ] {
            let exists: bool = sqlx::query_scalar("SELECT to_regclass($1::text) IS NOT NULL")
                .bind(rel)
                .fetch_one(migrate)
                .await?;
            if exists {
                datum_audit::attach(migrate, rel).await?;
            }
        }
        datum_audit::install_privileged(bootstrap).await?;

        let kernel = build_kernel(app_pool.clone(), profile).await?;
        let write = WritePool::new(app_pool.clone());
        let mut ctx = session::system_ctx("server.boot");
        ctx.config_version = Some(kernel.profile.spec_version.clone());
        let mut tx = Tx::begin(&write, &ctx).await?;
        datum_mod_locations::seed_install(&mut tx).await?;
        datum_mod_genealogy::wire(&kernel, &mut tx).await?;
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
            .bind(datum_db::app_version())
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
        })
    }

    /// Shared state for axum.
    pub fn state(self) -> AppState {
        AppState {
            inner: Arc::new(AppInner {
                kernel: self.kernel,
                bind: self.bind,
                manifest_hash: self.manifest_hash,
                calibration_doc: self.calibration_doc,
                database_url: self.database_url,
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
}

/// Run inventory / production / genealogy / server migrators after the kernel.
pub async fn migrate_slice_modules(pool: &PgPool) -> Result<()> {
    let crates: &[(&str, &sqlx::migrate::Migrator)] = &[
        ("datum-mod-inventory", &datum_mod_inventory::MIGRATOR),
        (
            "datum-mod-production-min",
            &datum_mod_production_min::MIGRATOR,
        ),
        ("datum-mod-genealogy", &datum_mod_genealogy::MIGRATOR),
        ("datum-server", &crate::MIGRATOR),
    ];
    for (name, migrator) in crates {
        datum_db::migrate::run(pool, &[(*name, *migrator)])
            .await
            .map_err(|e| Error::Config(format!("migrate {name}: {e}")))?;
    }
    Ok(())
}

/// `sqlx migrate run` records in `_sqlx_migrations`, not `datum.schema_history`.
/// If `datum.schema_history` already exists, re-applying datum-db 0001 fails
/// with `relation "schema_history" already exists`. Record those versions first.
async fn adopt_datum_db_history(pool: &PgPool) -> Result<()> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('datum.schema_history') IS NOT NULL")
        .fetch_one(pool)
        .await?;
    if !exists {
        return Ok(());
    }
    let version = datum_db::app_version();
    for migration in datum_db::MIGRATOR.iter() {
        if migration.migration_type.is_down_migration() {
            continue;
        }
        sqlx::query(
            r#"INSERT INTO datum.schema_history (crate, version, applied_at, app_version)
               VALUES ($1, $2, now(), $3)
               ON CONFLICT (crate, version) DO NOTHING"#,
        )
        .bind("datum-db")
        .bind(migration.version)
        .bind(&version)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Register every Wave 2s module through `datum-module` extension points.
pub async fn build_kernel(pool: Pool, profile: Profile) -> Result<Kernel> {
    let mut builder = Kernel::builder(pool, profile.clone());
    datum_mod_items::register(&mut builder, &profile)?;
    builder.apply_manifest(&datum_mod_locations::manifest()?)?;
    // ADDENDUM 1 item 5: lots are registered under the live profile so
    // `lot.release` is Required on regulated-device (refused under NoSignatures).
    datum_mod_lots::register(&mut builder, &profile)?;
    datum_mod_inventory::register(&mut builder, &profile)?;
    datum_mod_production_min::register(&mut builder, &profile)?;
    datum_mod_genealogy::register(&mut builder, &profile)?;
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
