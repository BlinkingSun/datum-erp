//! Configuration manifest (`docs/03` §8).

use serde::{Deserialize, Serialize};

use crate::manifest::hex;
use crate::order::KERNEL_ORDER;
use crate::profile::SignatureEdge;
use crate::{Error, Result};

/// Every module, version, enabled state, hashed and exportable (`docs/03` §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationManifest {
    /// Profile id.
    pub profile_id: String,
    /// Profile spec version.
    pub spec_version: String,
    /// Application version.
    pub app_version: String,
    /// [`crate::KERNEL_ORDER`].
    pub kernel_order: Vec<String>,
    /// Installed modules.
    pub modules: Vec<ManifestModule>,
    /// Every state-machine edge's declaration (both kinds).
    pub signature_edges: Vec<SignatureEdge>,
    /// Content hash (SHA-256 hex of the canonical body, excluding this field).
    pub content_hash: String,
}

/// One module as listed on the configuration manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestModule {
    /// Module id.
    pub id: String,
    /// Version.
    pub version: String,
    /// Enabled flag.
    pub enabled: bool,
    /// Regulated flag.
    pub regulated: bool,
}

#[derive(Serialize)]
struct HashBody<'a> {
    profile_id: &'a str,
    spec_version: &'a str,
    app_version: &'a str,
    kernel_order: &'a [String],
    modules: &'a [ManifestModule],
    signature_edges: &'a [SignatureEdge],
}

impl ConfigurationManifest {
    /// Build from live pieces and stamp the content hash.
    pub fn assemble(
        profile_id: impl Into<String>,
        spec_version: impl Into<String>,
        modules: Vec<ManifestModule>,
        signature_edges: Vec<SignatureEdge>,
    ) -> Result<Self> {
        let mut live = Self {
            profile_id: profile_id.into(),
            spec_version: spec_version.into(),
            app_version: datum_db::app_version(),
            kernel_order: KERNEL_ORDER.iter().map(|s| (*s).to_string()).collect(),
            modules,
            signature_edges,
            content_hash: String::new(),
        };
        live.content_hash = live.hash()?;
        Ok(live)
    }

    fn hash(&self) -> Result<String> {
        let body = HashBody {
            profile_id: &self.profile_id,
            spec_version: &self.spec_version,
            app_version: &self.app_version,
            kernel_order: &self.kernel_order,
            modules: &self.modules,
            signature_edges: &self.signature_edges,
        };
        Ok(hex(datum_audit::sha256::digest(&serde_json::to_vec(
            &body,
        )?)))
    }

    /// Recompute the hash from the live fields (IQ).
    pub fn verify_self(&self) -> Result<()> {
        if self.hash()? == self.content_hash {
            Ok(())
        } else {
            Err(Error::ManifestMismatch)
        }
    }
}

/// Export the stored configuration plus installed-module rows.
pub async fn export(pool: &datum_db::Pool) -> Result<ConfigurationManifest> {
    let modules: Vec<(String, String, bool, bool)> = sqlx::query_as(
        r#"SELECT id, version, enabled, regulated
           FROM module.installed
           ORDER BY id"#,
    )
    .fetch_all(pool)
    .await?;
    let modules: Vec<ManifestModule> = modules
        .into_iter()
        .map(|(id, version, enabled, regulated)| ManifestModule {
            id,
            version,
            enabled,
            regulated,
        })
        .collect();
    let cfg: Option<(String, String, serde_json::Value, String)> = sqlx::query_as(
        r#"SELECT profile_id, spec_version, body, content_hash
           FROM module.configuration
           WHERE id = 'effective-profile'"#,
    )
    .fetch_optional(pool)
    .await?;
    let (profile_id, spec_version, body, _stored_hash) =
        cfg.ok_or_else(|| Error::Profile("no effective-profile configuration record".into()))?;
    let signature_edges = body
        .get("signature_edges")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    ConfigurationManifest::assemble(profile_id, spec_version, modules, signature_edges)
}

/// Compare a stored manifest to a live one (IQ suite).
pub fn verify(stored: &ConfigurationManifest, live: &ConfigurationManifest) -> Result<()> {
    stored.verify_self()?;
    live.verify_self()?;
    if stored.content_hash == live.content_hash
        && stored.profile_id == live.profile_id
        && stored.modules == live.modules
        && stored.signature_edges == live.signature_edges
        && stored.kernel_order == live.kernel_order
    {
        Ok(())
    } else {
        Err(Error::ManifestMismatch)
    }
}
