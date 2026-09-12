//! `datum` CLI: serve, migrate, db check, iq, manifest export.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use datum_module::ProfileId;

use crate::boot::{self, App};
use crate::config::Config;
use crate::error::{Error, Result};

/// Datum HTTP process.
#[derive(Parser, Debug)]
#[command(name = "datum", version, about = "Datum ERP server")]
pub struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Serve the HTTP API.
    Serve {
        /// Installation profile.
        #[arg(long)]
        profile: String,
        /// Bind address.
        #[arg(long, default_value = "0.0.0.0:8080")]
        bind: String,
        /// Optional TOML config file.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Apply kernel + module migrations.
    Migrate {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        profile: Option<String>,
    },
    /// `datum db check`
    Db {
        #[command(subcommand)]
        cmd: DbCmd,
    },
    /// Run customer-facing IQ and print the configuration-manifest hash.
    Iq {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// `datum manifest export`
    Manifest {
        #[command(subcommand)]
        cmd: ManifestCmd,
    },
}

#[derive(Subcommand, Debug)]
enum DbCmd {
    /// Connect and SELECT 1.
    Check,
}

#[derive(Subcommand, Debug)]
enum ManifestCmd {
    /// Print the stored configuration manifest as JSON.
    Export {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

/// Parse and run.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Serve {
            profile,
            bind,
            config,
        } => {
            let cfg = Config::load(Some(&profile), Some(&bind), config.as_ref())?;
            let app = App::boot(cfg).await?;
            crate::http::serve(app.state()).await
        }
        Command::Migrate { config, profile } => {
            let cfg = Config::load(profile.as_deref(), None, config.as_ref())?;
            migrate(&cfg).await
        }
        Command::Db { cmd: DbCmd::Check } => db_check().await,
        Command::Iq { profile, config } => {
            let cfg = Config::load(profile.as_deref(), None, config.as_ref())?;
            iq(&cfg).await
        }
        Command::Manifest {
            cmd: ManifestCmd::Export { profile, config },
        } => {
            let cfg = Config::load(profile.as_deref(), None, config.as_ref())?;
            manifest_export(&cfg).await
        }
    }
}

async fn migrate(cfg: &Config) -> Result<()> {
    let _ = App::boot(cfg.clone()).await?;
    println!("migrate ok");
    Ok(())
}

async fn db_check() -> Result<()> {
    let url = std::env::var("DATUM_DATABASE_URL")
        .map_err(|_| Error::Config("DATUM_DATABASE_URL is required".into()))?;
    let pool = datum_db::connect(&crate::config::with_os_userinfo(&url)).await?;
    let ok: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await?;
    pool.close().await;
    if ok.0 == 1 {
        println!("db check ok");
        Ok(())
    } else {
        Err(Error::Config("db check failed".into()))
    }
}

async fn iq(cfg: &Config) -> Result<()> {
    let (hash, profile) = boot::run_iq(cfg.clone()).await?;
    println!("iq ok");
    println!("manifest_hash={hash}");
    println!("profile={profile}");
    let _ = ProfileId::PlainShop;
    Ok(())
}

async fn manifest_export(cfg: &Config) -> Result<()> {
    let app = App::boot(cfg.clone()).await?;
    let m = boot::live_manifest(app.kernel.pool()).await?;
    println!("{}", serde_json::to_string_pretty(&m)?);
    Ok(())
}
