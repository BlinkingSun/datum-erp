//! Process configuration: TOML file plus environment.

use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

use datum_module::{Profile, ProfileId};

use crate::error::{Error, Result};

/// Runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Installation profile.
    pub profile: Profile,
    /// Bind address (`0.0.0.0:8080`).
    pub bind: SocketAddr,
    /// App role URL (`DATUM_DATABASE_URL`).
    pub database_url: String,
    /// Migrate role URL.
    pub migrate_url: String,
    /// Bootstrap / owner URL.
    pub bootstrap_url: String,
}

impl Config {
    /// Load from CLI flags, optional TOML, and environment.
    pub fn load(
        profile: Option<&str>,
        bind: Option<&str>,
        config_path: Option<&PathBuf>,
    ) -> Result<Self> {
        let file = if let Some(path) = config_path {
            let text = fs::read_to_string(path)
                .map_err(|e| Error::Config(format!("config file {}: {e}", path.display())))?;
            parse_toml(&text)?
        } else if let Ok(path) = std::env::var("DATUM_CONFIG") {
            let text = fs::read_to_string(&path)
                .map_err(|e| Error::Config(format!("DATUM_CONFIG {path}: {e}")))?;
            parse_toml(&text)?
        } else {
            FileConfig::default()
        };

        let profile_id = profile
            .map(str::to_owned)
            .or_else(|| std::env::var("DATUM_PROFILE").ok())
            .or(file.profile)
            .ok_or_else(|| {
                Error::Config("profile is required (--profile or DATUM_PROFILE)".into())
            })?;
        let id = ProfileId::parse(&profile_id).map_err(|e| Error::Config(e.to_string()))?;
        let profile = Profile::load(id).map_err(|e| Error::Config(e.to_string()))?;

        let bind_s = bind
            .map(str::to_owned)
            .or_else(|| std::env::var("DATUM_BIND").ok())
            .or(file.bind)
            .unwrap_or_else(|| "0.0.0.0:8080".into());
        let bind: SocketAddr = bind_s
            .parse()
            .map_err(|e| Error::Config(format!("bind {bind_s}: {e}")))?;

        let database_url =
            with_os_userinfo(&required_url("DATUM_DATABASE_URL", file.database_url)?);
        let migrate_url = with_os_userinfo(&required_url(
            "DATUM_MIGRATE_DATABASE_URL",
            file.migrate_url,
        )?);
        let bootstrap_url = bootstrap_against_app(
            &with_os_userinfo(&required_url("DATUM_BOOTSTRAP_URL", file.bootstrap_url)?),
            &database_url,
        );

        Ok(Self {
            profile,
            bind,
            database_url,
            migrate_url,
            bootstrap_url,
        })
    }
}

/// Point the bootstrap URL at the app database so privileged install does not
/// run against the `postgres` maintenance database (`.env.example`).
pub fn bootstrap_against_app(bootstrap_url: &str, database_url: &str) -> String {
    rewrite_database(bootstrap_url, &database_name(database_url))
}

/// Database name from a PostgreSQL URL (`…/name` or `…/name?…`).
pub fn database_name(url: &str) -> String {
    let (head, _) = url.split_once('?').unwrap_or((url, ""));
    head.rsplit_once('/')
        .map(|(_, d)| d.to_string())
        .unwrap_or_default()
}

/// Replace the database path, keeping userinfo and query.
pub fn rewrite_database(url: &str, database: &str) -> String {
    let (head, query) = url.split_once('?').unwrap_or((url, ""));
    let prefix = head.rsplit_once('/').map(|(p, _)| p).unwrap_or(head);
    if query.is_empty() {
        format!("{prefix}/{database}")
    } else {
        format!("{prefix}/{database}?{query}")
    }
}

/// sqlx treats a URL with no userinfo as role `anonymous`. libpq uses the OS
/// account; TestDb already special-cases this. Serve/connect share that rule.
pub fn with_os_userinfo(url: &str) -> String {
    if url_has_userinfo(url) {
        return url.to_string();
    }
    let Some(user) = os_username() else {
        return url.to_string();
    };
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    format!("{scheme}://{user}@{rest}")
}

fn url_has_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    rest.split(['/', '?']).next().unwrap_or("").contains('@')
}

fn os_username() -> Option<String> {
    std::env::var("PGUSER")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("LOGNAME").ok())
        .filter(|u| !u.is_empty() && u != "anonymous")
        .or_else(|| {
            std::process::Command::new("id")
                .arg("-un")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|u| !u.is_empty() && u != "anonymous")
        })
}

#[derive(Debug, Default)]
struct FileConfig {
    profile: Option<String>,
    bind: Option<String>,
    database_url: Option<String>,
    migrate_url: Option<String>,
    bootstrap_url: Option<String>,
}

fn parse_toml(text: &str) -> Result<FileConfig> {
    let mut out = FileConfig::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = unquote(v.trim());
        match key {
            "profile" => out.profile = Some(val),
            "bind" => out.bind = Some(val),
            "database_url" => out.database_url = Some(val),
            "migrate_url" => out.migrate_url = Some(val),
            "bootstrap_url" => out.bootstrap_url = Some(val),
            _ => {}
        }
    }
    Ok(out)
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').trim_matches('\'').to_string()
}

fn required_url(name: &str, from_file: Option<String>) -> Result<String> {
    if let Ok(v) = std::env::var(name)
        && !v.is_empty()
    {
        return Ok(v);
    }
    from_file.ok_or_else(|| Error::Config(format!("{name} is required")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_without_userinfo_does_not_default_to_anonymous() {
        let rewritten = with_os_userinfo("postgres://127.0.0.1:5432/postgres?sslmode=disable");
        assert!(!rewritten.contains("anonymous@"), "rewritten={rewritten}");
        if os_username().is_some() {
            assert!(
                rewritten.contains('@'),
                "OS user must be injected: {rewritten}"
            );
        }
    }

    #[test]
    fn url_with_userinfo_is_unchanged() {
        let u = "postgres://datum_app:datum@127.0.0.1:5432/datum_test?sslmode=disable";
        assert_eq!(with_os_userinfo(u), u);
    }

    #[test]
    fn bootstrap_against_postgres_maintenance_targets_app_db() {
        let boot = "postgres://127.0.0.1:5432/postgres?sslmode=disable";
        let app = "postgres://datum_app:datum@127.0.0.1:5432/datum_test?sslmode=disable";
        let rewritten = bootstrap_against_app(&with_os_userinfo(boot), app);
        assert!(rewritten.contains("/datum_test?"), "rewritten={rewritten}");
        assert!(
            !rewritten.contains("/postgres?"),
            "must not install against maintenance db: {rewritten}"
        );
    }
}
