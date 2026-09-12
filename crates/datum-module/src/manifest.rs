//! Module manifest (`docs/03` §2).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::order::ModuleNode;
use crate::semver::{Range, Version};
use crate::toml::{self, Value};
use crate::{Error, Result};

/// Kernel package version compiled into this crate.
pub fn kernel_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version {
        major: 0,
        minor: 1,
        patch: 0,
    })
}

/// Parsed `module.toml` (`docs/03` §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleManifest {
    /// Module id (`mod-items`, `calibration`, …).
    pub id: String,
    /// Semver version.
    pub version: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Required dependencies (`id` → caret range).
    pub dependencies: BTreeMap<String, String>,
    /// Optional dependencies.
    pub optional_dependencies: BTreeMap<String, String>,
    /// Permission keys declared by this module.
    pub permissions: BTreeMap<String, String>,
    /// Permission keys that require a signature.
    pub requires_signature: Vec<String>,
    /// `regulated = true` in `[capabilities]` (must be present).
    pub regulated: bool,
}

impl ModuleManifest {
    /// Parse and validate a `module.toml`.
    pub fn parse(toml: &str) -> Result<Self> {
        let root = toml::parse(toml)?;
        let module = toml::require_table(&root, "module")?;
        let id = toml::require_str(module, "id")?;
        let version = toml::require_str(module, "version")?;
        let _ = Version::parse(&version)?;
        let name = toml::require_str(module, "name")?;
        let description = module
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let dependencies = string_map(root.get("dependencies"))?;
        let optional_dependencies = string_map(root.get("optional-dependencies"))?;
        let permissions = string_map(root.get("permissions"))?;
        let capabilities = toml::require_table(&root, "capabilities")?;
        let regulated = toml::require_bool(capabilities, "regulated")?;
        let requires_signature = match capabilities.get("requires-signature") {
            Some(v) => toml::string_array(v)?,
            None => Vec::new(),
        };
        let parsed = Self {
            id,
            version,
            name,
            description,
            dependencies,
            optional_dependencies,
            permissions,
            requires_signature,
            regulated,
        };
        parsed.validate()?;
        Ok(parsed)
    }

    /// Semver / permission / range checks.
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(Error::Manifest("empty module id".into()));
        }
        let _ = Version::parse(&self.version)?;
        for (dep, range) in self
            .dependencies
            .iter()
            .chain(self.optional_dependencies.iter())
        {
            let r = Range::parse(range)?;
            if dep == "kernel" && !r.matches(&kernel_version()) {
                return Err(Error::Manifest(format!(
                    "kernel {} does not satisfy {range}",
                    kernel_version().to_canonical()
                )));
            }
        }
        let declared: BTreeSet<&str> = self.permissions.keys().map(String::as_str).collect();
        for key in &self.requires_signature {
            if !declared.contains(key.as_str()) {
                return Err(Error::Manifest(format!(
                    "requires-signature {key} is not declared in [permissions]"
                )));
            }
        }
        Ok(())
    }

    /// Graph node: required dependencies excluding `kernel`.
    pub fn node(&self) -> ModuleNode {
        ModuleNode {
            id: self.id.clone(),
            depends_on: self
                .dependencies
                .keys()
                .filter(|d| *d != "kernel")
                .cloned()
                .collect(),
        }
    }

    /// Canonical bytes hashed into `module.installed.manifest_hash`.
    pub fn hash_input(&self, enabled: bool) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct HashBody<'a> {
            id: &'a str,
            version: &'a str,
            regulated: bool,
            enabled: bool,
            permissions: &'a BTreeMap<String, String>,
            dependencies: &'a BTreeMap<String, String>,
        }
        Ok(serde_json::to_vec(&HashBody {
            id: &self.id,
            version: &self.version,
            regulated: self.regulated,
            enabled,
            permissions: &self.permissions,
            dependencies: &self.dependencies,
        })?)
    }

    /// Hex SHA-256 of [`Self::hash_input`].
    pub fn manifest_hash(&self, enabled: bool) -> Result<String> {
        Ok(hex(datum_audit::sha256::digest(&self.hash_input(enabled)?)))
    }
}

fn string_map(value: Option<&Value>) -> Result<BTreeMap<String, String>> {
    let Some(v) = value else {
        return Ok(BTreeMap::new());
    };
    let table = v
        .as_table()
        .ok_or_else(|| Error::Toml("expected table".into()))?;
    let mut out = BTreeMap::new();
    for (k, val) in table {
        let s = val
            .as_str()
            .ok_or_else(|| Error::Toml(format!("{k} is not a string")))?;
        out.insert(k.clone(), s.to_string());
    }
    Ok(out)
}

/// Hex encode 32 bytes.
pub(crate) fn hex(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

/// Compiled-in Wave 2s module manifests (PLAN §3).
pub fn compiled_in() -> Result<Vec<ModuleManifest>> {
    const ITEMS: &str = r#"
[module]
id = "mod-items"
version = "0.1.0"
name = "Items"
description = "Part master"

[dependencies]
kernel = "^0.1"

[permissions]
"items.view" = "View items"

[capabilities]
requires-signature = []
regulated = false
"#;
    const LOCATIONS: &str = r#"
[module]
id = "mod-locations"
version = "0.1.0"
name = "Locations"
description = "Warehouses, bins, virtual locations"

[dependencies]
kernel = "^0.1"

[permissions]
"items.view" = "View locations through items"

[capabilities]
requires-signature = []
regulated = false
"#;
    const LOTS: &str = r#"
[module]
id = "mod-lots"
version = "0.1.0"
name = "Lots"
description = "Lot and serial identity"

[dependencies]
kernel = "^0.1"

[permissions]
"items.view" = "View lots"

[capabilities]
requires-signature = []
regulated = false
"#;
    const INVENTORY: &str = r#"
[module]
id = "mod-inventory"
version = "0.1.0"
name = "Inventory"
description = "Receipts, issues, moves"

[dependencies]
kernel = "^0.1"
mod-items = "^0.1"
mod-locations = "^0.1"
mod-lots = "^0.1"

[permissions]
"inventory.view" = "View inventory"

[capabilities]
requires-signature = []
regulated = false
"#;
    const PRODUCTION: &str = r#"
[module]
id = "mod-production-min"
version = "0.1.0"
name = "Production (slice)"
description = "Minimal work order"

[dependencies]
kernel = "^0.1"
mod-inventory = "^0.1"

[permissions]
"production.view" = "View work orders"
"wo.release" = "Release a work order"

[capabilities]
requires-signature = []
regulated = false
"#;
    const GENEALOGY: &str = r#"
[module]
id = "mod-genealogy"
version = "0.1.0"
name = "Genealogy"
description = "Forward and backward trace"

[dependencies]
kernel = "^0.1"
mod-inventory = "^0.1"
mod-lots = "^0.1"
mod-production-min = "^0.1"

[permissions]
"genealogy.view" = "View genealogy"

[capabilities]
requires-signature = []
regulated = false
"#;
    Ok(vec![
        ModuleManifest::parse(ITEMS)?,
        ModuleManifest::parse(LOCATIONS)?,
        ModuleManifest::parse(LOTS)?,
        ModuleManifest::parse(INVENTORY)?,
        ModuleManifest::parse(PRODUCTION)?,
        ModuleManifest::parse(GENEALOGY)?,
    ])
}

/// Graph of [`compiled_in`].
pub fn compiled_in_graph() -> Result<Vec<ModuleNode>> {
    Ok(compiled_in()?.into_iter().map(|m| m.node()).collect())
}
