//! Module manifest (`docs/03` §2).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::order::{ModuleNode, topological_order};
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
    /// State machines declared on this manifest (`docs/03` §3).
    #[serde(default)]
    pub machines: Vec<ManifestMachine>,
    /// Event subscriptions declared on this manifest (`docs/03` §3.1).
    #[serde(default)]
    pub subscriptions: Vec<ManifestSubscription>,
    /// HTTP routes declared on this manifest (`docs/03` §3.4).
    #[serde(default)]
    pub routes: Vec<ManifestRoute>,
    /// Job kinds declared on this manifest (`docs/03` §3).
    #[serde(default)]
    pub jobs: Vec<ManifestJob>,
    /// `[[custom-fields]]` declared on this manifest.
    #[serde(default, skip)]
    pub custom_fields: datum_customfields::ManifestCustomFields,
    /// SQL applied inside [`crate::install`]'s transaction (`docs/03` §6).
    ///
    /// Compiled-in Wave 2s modules have none; tests supply `'static` statements.
    #[serde(default, skip)]
    pub migrations: Vec<&'static str>,
}

/// One machine declared in `module.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestMachine {
    /// Document type the machine advances.
    pub doc_type: String,
    /// Owning module's regulated flag (total signature declaration when true).
    pub regulated: bool,
    /// Declared states (edges also introduce states).
    pub states: Vec<String>,
    /// Declared edges.
    pub edges: Vec<ManifestMachineEdge>,
}

/// One edge declared in `module.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestMachineEdge {
    /// From state.
    pub from: String,
    /// To state.
    pub to: String,
    /// Edge name.
    pub name: String,
    /// RBAC permission key.
    pub permission: String,
    /// `Required` declaration when true.
    pub required: bool,
    /// Signature meaning when required.
    pub meaning: Option<String>,
    /// Signature permission when required (defaults to [`Self::permission`]).
    pub signature_permission: Option<String>,
    /// `NotRequired` reason when not required.
    pub reason: Option<String>,
}

/// One event subscription declared in `module.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSubscription {
    /// Event name (`inventory.lot_received`).
    pub event: String,
    /// Subscriber id.
    pub subscriber: String,
}

/// One HTTP route declared in `module.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestRoute {
    /// Path prefix (`/api/v1/calibration`).
    pub path: String,
    /// Permission that gates the route.
    pub permission: String,
}

/// One job kind declared in `module.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestJob {
    /// Job kind (`genealogy.refresh`).
    pub kind: String,
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
        let custom_fields = parse_custom_fields(&root, &id)?;
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
            machines: parse_machines(&root)?,
            subscriptions: parse_subscriptions(&root)?,
            routes: parse_routes(&root)?,
            jobs: parse_jobs(&root)?,
            custom_fields,
            migrations: Vec::new(),
        };
        parsed.validate()?;
        Ok(parsed)
    }

    /// Attach SQL that [`crate::install`] runs inside the install transaction.
    pub fn with_migrations(mut self, migrations: Vec<&'static str>) -> Self {
        self.migrations = migrations;
        self
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

fn table_array<'a>(
    root: &'a BTreeMap<String, Value>,
    key: &str,
) -> Result<Vec<&'a BTreeMap<String, Value>>> {
    match root.get(key) {
        None => Ok(Vec::new()),
        Some(v) => {
            let arr = v
                .as_array()
                .ok_or_else(|| Error::Toml(format!("expected [[{key}]]")))?;
            arr.iter()
                .map(|item| {
                    item.as_table()
                        .ok_or_else(|| Error::Toml(format!("{key} item is not a table")))
                })
                .collect()
        }
    }
}

fn parse_machines(root: &BTreeMap<String, Value>) -> Result<Vec<ManifestMachine>> {
    let mut out = Vec::new();
    for table in table_array(root, "machines")? {
        let doc_type = toml::require_str(table, "doc_type")?;
        let regulated = table
            .get("regulated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let states = match table.get("states") {
            Some(v) => toml::string_array(v)?,
            None => Vec::new(),
        };
        let mut edges = Vec::new();
        if let Some(raw) = table.get("edges") {
            let arr = raw
                .as_array()
                .ok_or_else(|| Error::Toml("expected [[machines.edges]]".into()))?;
            for item in arr {
                let e = item
                    .as_table()
                    .ok_or_else(|| Error::Toml("machine edge is not a table".into()))?;
                edges.push(ManifestMachineEdge {
                    from: toml::require_str(e, "from")?,
                    to: toml::require_str(e, "to")?,
                    name: toml::require_str(e, "name")?,
                    permission: toml::require_str(e, "permission")?,
                    required: e.get("required").and_then(Value::as_bool).unwrap_or(false),
                    meaning: e.get("meaning").and_then(Value::as_str).map(str::to_string),
                    signature_permission: e
                        .get("signature_permission")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    reason: e.get("reason").and_then(Value::as_str).map(str::to_string),
                });
            }
        }
        out.push(ManifestMachine {
            doc_type,
            regulated,
            states,
            edges,
        });
    }
    Ok(out)
}

fn parse_subscriptions(root: &BTreeMap<String, Value>) -> Result<Vec<ManifestSubscription>> {
    let mut out = Vec::new();
    for table in table_array(root, "subscriptions")? {
        out.push(ManifestSubscription {
            event: toml::require_str(table, "event")?,
            subscriber: toml::require_str(table, "subscriber")?,
        });
    }
    Ok(out)
}

fn parse_routes(root: &BTreeMap<String, Value>) -> Result<Vec<ManifestRoute>> {
    let mut out = Vec::new();
    for table in table_array(root, "routes")? {
        out.push(ManifestRoute {
            path: toml::require_str(table, "path")?,
            permission: toml::require_str(table, "permission")?,
        });
    }
    Ok(out)
}

fn parse_jobs(root: &BTreeMap<String, Value>) -> Result<Vec<ManifestJob>> {
    let mut out = Vec::new();
    for table in table_array(root, "jobs")? {
        out.push(ManifestJob {
            kind: toml::require_str(table, "kind")?,
        });
    }
    Ok(out)
}

fn parse_custom_fields(
    root: &BTreeMap<String, Value>,
    owner: &str,
) -> Result<datum_customfields::ManifestCustomFields> {
    let mut fields = Vec::new();
    for table in table_array(root, "custom-fields")? {
        let type_name = toml::require_str(table, "type")?;
        let field_type = datum_customfields::FieldType::parse(&type_name)
            .ok_or_else(|| Error::Manifest(format!("unknown custom field type {type_name}")))?;
        let field_owner = table
            .get("owner")
            .and_then(Value::as_str)
            .unwrap_or(owner)
            .to_string();
        fields.push(datum_customfields::ManifestCustomField {
            entity: toml::require_str(table, "entity")?,
            key: toml::require_str(table, "key")?,
            field_type,
            label: toml::require_str(table, "label")?,
            validate: toml::require_str(table, "validate")?,
            audit: toml::require_bool(table, "audit")?,
            required: table
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            indexed: table
                .get("indexed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            owner: field_owner,
        });
    }
    Ok(datum_customfields::ManifestCustomFields { fields })
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

[[routes]]
path = "/api/v1/inventory"
permission = "inventory.view"
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

[[machines]]
doc_type = "wo"
regulated = false
states = ["Draft", "Released"]

[[machines.edges]]
from = "Draft"
to = "Released"
name = "release"
permission = "wo.release"

[[routes]]
path = "/api/v1/production"
permission = "production.view"
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

[[subscriptions]]
event = "inventory.lot_received"
subscriber = "datum-jobs"

[[jobs]]
kind = "genealogy.refresh"

[[routes]]
path = "/api/v1/genealogy"
permission = "genealogy.view"
"#;
    const CALIBRATION: &str = r#"
[module]
id = "mod-calibration"
version = "0.1.0"
name = "Gage Calibration"
description = "Calibration schedules and certificate approval"

[dependencies]
kernel = "^0.1"

[permissions]
"calibration.view" = "View calibration records"
"calibration.approve" = "Approve a calibration certificate"

[capabilities]
requires-signature = ["calibration.approve"]
regulated = true

[[machines]]
doc_type = "calibration.certificate"
regulated = true
states = ["Open", "Approved"]

[[machines.edges]]
from = "Open"
to = "Approved"
name = "approve"
permission = "calibration.approve"
required = true
meaning = "Approved"
signature_permission = "calibration.approve"

[[routes]]
path = "/api/v1/calibration"
permission = "calibration.view"

[[custom-fields]]
entity = "items.item"
key = "udi_device_identifier"
type = "string"
label = "UDI-DI"
validate = "gs1-gtin"
audit = true
required = false
indexed = false
owner = "mod-calibration"
"#;
    let wave = [
        ModuleManifest::parse(crate::install_graph::ITEMS_MANIFEST)?,
        ModuleManifest::parse(crate::install_graph::LOCATIONS_MANIFEST)?,
        ModuleManifest::parse(crate::install_graph::LOTS_MANIFEST)?,
    ];
    let nodes: Vec<ModuleNode> = wave.iter().map(|m| m.node()).collect();
    let order = topological_order(&nodes)?;
    let by_id: std::collections::BTreeMap<&str, &ModuleManifest> =
        wave.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut out: Vec<ModuleManifest> = order
        .iter()
        .map(|id| {
            by_id
                .get(id.as_str())
                .map(|m| (*m).clone())
                .ok_or_else(|| Error::UnknownModule(id.clone()))
        })
        .collect::<Result<Vec<_>>>()?;
    out.push(ModuleManifest::parse(INVENTORY)?);
    out.push(ModuleManifest::parse(PRODUCTION)?);
    out.push(ModuleManifest::parse(GENEALOGY)?);
    out.push(ModuleManifest::parse(CALIBRATION)?);
    Ok(out)
}

/// Graph of [`compiled_in`].
pub fn compiled_in_graph() -> Result<Vec<ModuleNode>> {
    Ok(compiled_in()?.into_iter().map(|m| m.node()).collect())
}
