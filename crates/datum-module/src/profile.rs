//! Installation profiles (D-W1-5 eleven keys).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::toml::{self, Value};
use crate::{Error, Result};

const REGULATED_TOML: &str = include_str!("../../../profiles/regulated-device.toml");
const PLAIN_TOML: &str = include_str!("../../../profiles/plain-shop.toml");

/// Keys that may differ between the two shipped profiles (SPEC-profiles).
pub const DELTA_ALLOWED: &[&str] = &[
    "modules",
    "signature_edges",
    "signature_gate_binding",
    "navigation",
    "numbering",
    "seeded_permissions",
];

/// Profile id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProfileId {
    /// Beachhead / regulated device shop.
    RegulatedDevice,
    /// Bracket shop: no `regulated = true` module enabled.
    PlainShop,
}

impl ProfileId {
    /// Parse the frozen ids.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "regulated-device" => Ok(Self::RegulatedDevice),
            "plain-shop" => Ok(Self::PlainShop),
            other => Err(Error::Profile(format!("unknown profile id {other}"))),
        }
    }

    /// Wire id.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegulatedDevice => "regulated-device",
            Self::PlainShop => "plain-shop",
        }
    }
}

/// One compiled-in module row (key 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileModule {
    /// Module id.
    pub id: String,
    /// Version.
    pub version: String,
    /// Module-owned regulated flag.
    pub regulated: bool,
    /// Always `true` for first-party modules (schema identical).
    pub installed: bool,
    /// Enablement flag (the profile's only lever).
    pub enabled: bool,
}

/// Generated signature declaration (key 3). Never taken from TOML values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureEdge {
    /// `Required { meaning, permission }`.
    Required {
        /// Module / document type.
        module: String,
        /// Edge name.
        edge: String,
        /// Meaning.
        meaning: String,
        /// Permission key.
        permission: String,
    },
    /// `NotRequired { reason }`.
    NotRequired {
        /// Module / document type.
        module: String,
        /// Edge name.
        edge: String,
        /// Reason.
        reason: String,
    },
}

impl SignatureEdge {
    /// True when this is [`SignatureEdge::Required`].
    pub fn is_required(&self) -> bool {
        matches!(self, Self::Required { .. })
    }
}

/// Which [`datum_core::SignatureGate`] the composition root binds (key 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateBinding {
    /// `NoSignatures` (pre-Wave-2b and tests).
    NoSignatures,
    /// `datum-esign` (Wave 2b).
    DatumEsign,
}

impl GateBinding {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "NoSignatures" => Ok(Self::NoSignatures),
            "datum-esign" => Ok(Self::DatumEsign),
            other => Err(Error::Profile(format!("unknown gate {other}"))),
        }
    }

    /// Wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoSignatures => "NoSignatures",
            Self::DatumEsign => "datum-esign",
        }
    }
}

/// Numbering template for one document type (key 7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberingSpec {
    /// Format template (`WO-{yyyy}-{0000}`).
    pub format: String,
    /// Prefix.
    pub prefix: String,
    /// Scope (`document`, …).
    pub scope: String,
    /// Gap-free where D3 §8 requires it.
    pub gap_free: bool,
    /// Reset policy (`never`, `yearly`, `monthly`).
    pub reset: String,
}

impl NumberingSpec {
    /// Map to [`datum_numbering::ResetPolicy`].
    pub fn reset_policy(&self) -> Result<datum_numbering::ResetPolicy> {
        match self.reset.as_str() {
            "never" => Ok(datum_numbering::ResetPolicy::Never),
            "yearly" => Ok(datum_numbering::ResetPolicy::Yearly),
            "monthly" => Ok(datum_numbering::ResetPolicy::Monthly),
            other => Err(Error::Profile(format!("unknown reset policy {other}"))),
        }
    }
}

/// Role bundle seeded per profile (key 10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileBundle {
    /// Role name.
    pub name: String,
    /// Permission keys.
    pub permissions: Vec<String>,
}

/// The eleven keys of D-W1-5 as authored for one installation profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Key 1.
    pub id: ProfileId,
    /// Display name.
    pub display_name: String,
    /// Spec version.
    pub spec_version: String,
    /// Key 2.
    pub modules: Vec<ProfileModule>,
    /// Key 3 — filled from the registry, never from TOML values.
    pub signature_edges: Vec<SignatureEdge>,
    /// Key 4.
    pub signature_gate_binding: GateBinding,
    /// Key 5.
    pub validation_manifest: ValidationManifest,
    /// Key 6.
    pub navigation: Navigation,
    /// Key 7.
    pub numbering: BTreeMap<String, NumberingSpec>,
    /// Key 8.
    pub kernel_always_on: Vec<String>,
    /// Key 9.
    pub anchor_sink: AnchorSink,
    /// Key 10.
    pub seeded_permissions: SeededPermissions,
    /// Key 11.
    pub acceptance: Acceptance,
}

/// Key 5: validation manifest is always generated; only navigation is hidden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationManifest {
    /// Always true.
    pub generated: bool,
    /// HTTP route.
    pub route: String,
    /// CLI.
    pub cli: String,
    /// Permission that reads it.
    pub permission: String,
    /// Audit export route (stays available in both profiles).
    pub audit_export_route: String,
    /// Audit export CLI.
    pub audit_export_cli: String,
    /// Profiles must not hide this surface.
    pub hidden_by_profile: bool,
}

/// Key 6: the only place profile-driven hiding is expressed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Navigation {
    /// Visible nav ids.
    pub visible: Vec<String>,
    /// Hidden nav ids (`plain-shop` hides Validation/IQ and regulated modules).
    pub hidden: Vec<String>,
}

/// Key 9: off-box anchor is per-instance, not a profile switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorSink {
    /// Per-instance configuration.
    pub per_instance: bool,
    /// IQ suite treats the sink as per-instance (fail vs nag is not this key).
    pub iq_suite: String,
}

/// Key 10.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededPermissions {
    /// Role bundles.
    pub bundles: Vec<ProfileBundle>,
    /// Base currency.
    pub base_currency: String,
    /// Stock UOM system.
    pub stock_uom_system: String,
    /// Display timezone (stored time is UTC).
    pub display_timezone: String,
}

/// Key 11.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acceptance {
    /// Wave 2s items that differ by profile (7 and 11 only).
    pub wave2s_items_that_differ: Vec<i64>,
}

impl Profile {
    /// Load `regulated-device`.
    pub fn regulated_device() -> Result<Self> {
        Self::parse(REGULATED_TOML)
    }

    /// Load `plain-shop`.
    pub fn plain_shop() -> Result<Self> {
        Self::parse(PLAIN_TOML)
    }

    /// Load by id.
    pub fn load(id: ProfileId) -> Result<Self> {
        match id {
            ProfileId::RegulatedDevice => Self::regulated_device(),
            ProfileId::PlainShop => Self::plain_shop(),
        }
    }

    /// Parse a profile document. `signature_edges` values in TOML are discarded.
    pub fn parse(toml_src: &str) -> Result<Self> {
        let root = toml::parse(toml_src)?;
        let present = profile_keys_present(&root)?;
        if present.len() != 11 {
            return Err(Error::Profile(format!(
                "expected 11 keys, found {}",
                present.len()
            )));
        }
        let header = toml::require_table(&root, "profile")?;
        let id = ProfileId::parse(&toml::require_str(header, "id")?)?;
        let display_name = toml::require_str(header, "name")
            .or_else(|_| toml::require_str(header, "display_name"))?;
        let spec_version = toml::require_str(header, "spec_version")?;
        let modules = parse_modules(&root)?;
        if !root.contains_key("signature_edges") {
            return Err(Error::Profile("missing signature_edges".into()));
        }
        let gate_tbl = toml::require_table(&root, "signature_gate_binding")?;
        let signature_gate_binding = GateBinding::parse(&toml::require_str(gate_tbl, "gate")?)?;
        let vm = toml::require_table(&root, "validation_manifest")?;
        let validation_manifest = ValidationManifest {
            generated: toml::require_bool(vm, "generated")?,
            route: toml::require_str(vm, "route")?,
            cli: toml::require_str(vm, "cli")?,
            permission: toml::require_str(vm, "permission")?,
            audit_export_route: toml::require_str(vm, "audit_export_route")?,
            audit_export_cli: toml::require_str(vm, "audit_export_cli")?,
            hidden_by_profile: toml::require_bool(vm, "hidden_by_profile")?,
        };
        if validation_manifest.hidden_by_profile {
            return Err(Error::Profile(
                "validation manifest must not be hidden by a profile".into(),
            ));
        }
        let nav = toml::require_table(&root, "navigation")?;
        let navigation = Navigation {
            visible: root_array(nav, "visible")?,
            hidden: root_array(nav, "hidden")?,
        };
        let numbering = parse_numbering(&root)?;
        let kernel_always_on = match root.get("kernel_always_on") {
            Some(v) => toml::string_array(v)?,
            None => return Err(Error::Profile("missing kernel_always_on".into())),
        };
        let sink = toml::require_table(&root, "anchor_sink")?;
        let anchor_sink = AnchorSink {
            per_instance: toml::require_bool(sink, "per_instance")?,
            iq_suite: toml::require_str(sink, "iq_suite")?,
        };
        let sp = toml::require_table(&root, "seeded_permissions")?;
        let seeded_permissions = SeededPermissions {
            bundles: parse_bundles(sp)?,
            base_currency: toml::require_str(sp, "base_currency")?,
            stock_uom_system: toml::require_str(sp, "stock_uom_system")?,
            display_timezone: toml::require_str(sp, "display_timezone")?,
        };
        let acc = toml::require_table(&root, "acceptance")?;
        let acceptance = Acceptance {
            wave2s_items_that_differ: match acc.get("wave2s_items_that_differ") {
                Some(v) => toml::int_array(v)?,
                None => return Err(Error::Profile("missing wave2s_items_that_differ".into())),
            },
        };
        if acceptance.wave2s_items_that_differ != [7, 11] {
            return Err(Error::Profile(
                "acceptance.wave2s_items_that_differ must be [7, 11]".into(),
            ));
        }
        if id == ProfileId::PlainShop {
            for m in &modules {
                if m.regulated && m.enabled {
                    return Err(Error::Profile(
                        "plain-shop must not enable a regulated = true module".into(),
                    ));
                }
            }
        }
        Ok(Self {
            id,
            display_name,
            spec_version,
            modules,
            signature_edges: Vec::new(),
            signature_gate_binding,
            validation_manifest,
            navigation,
            numbering,
            kernel_always_on,
            anchor_sink,
            seeded_permissions,
            acceptance,
        })
    }

    /// Replace `signature_edges` from the live registry (never TOML).
    pub fn with_registry_edges(mut self, edges: Vec<SignatureEdge>) -> Self {
        self.signature_edges = edges;
        self
    }

    /// Required-set for this profile (asserted empty for `plain-shop`).
    pub fn required_edges(&self) -> Vec<&SignatureEdge> {
        self.signature_edges
            .iter()
            .filter(|e| e.is_required())
            .collect()
    }

    /// Dump used by the delta test (profile id / display name omitted).
    pub fn effective_dump(&self) -> Result<BTreeMap<String, serde_json::Value>> {
        let mut map = BTreeMap::new();
        map.insert(
            "profile".into(),
            serde_json::json!({ "spec_version": self.spec_version }),
        );
        map.insert("modules".into(), serde_json::to_value(&self.modules)?);
        map.insert(
            "signature_edges".into(),
            serde_json::to_value(&self.signature_edges)?,
        );
        map.insert(
            "signature_gate_binding".into(),
            serde_json::to_value(&self.signature_gate_binding)?,
        );
        map.insert(
            "validation_manifest".into(),
            serde_json::to_value(&self.validation_manifest)?,
        );
        map.insert("navigation".into(), serde_json::to_value(&self.navigation)?);
        map.insert("numbering".into(), serde_json::to_value(&self.numbering)?);
        map.insert(
            "kernel_always_on".into(),
            serde_json::to_value(&self.kernel_always_on)?,
        );
        map.insert(
            "anchor_sink".into(),
            serde_json::to_value(&self.anchor_sink)?,
        );
        map.insert(
            "seeded_permissions".into(),
            serde_json::to_value(&self.seeded_permissions)?,
        );
        map.insert("acceptance".into(), serde_json::to_value(&self.acceptance)?);
        Ok(map)
    }
}

fn profile_keys_present(root: &BTreeMap<String, Value>) -> Result<BTreeSet<String>> {
    let required = [
        "profile",
        "modules",
        "signature_edges",
        "signature_gate_binding",
        "validation_manifest",
        "navigation",
        "numbering",
        "kernel_always_on",
        "anchor_sink",
        "seeded_permissions",
        "acceptance",
    ];
    let mut present = BTreeSet::new();
    for key in required {
        if root.contains_key(key) {
            present.insert(key.to_string());
        } else {
            return Err(Error::Profile(format!("missing key {key}")));
        }
    }
    Ok(present)
}

fn parse_modules(root: &BTreeMap<String, Value>) -> Result<Vec<ProfileModule>> {
    let Some(arr) = root.get("modules").and_then(Value::as_array) else {
        return Err(Error::Profile("modules must be an array of tables".into()));
    };
    let mut out = Vec::new();
    for item in arr {
        let t = item
            .as_table()
            .ok_or_else(|| Error::Profile("module row is not a table".into()))?;
        let row = ProfileModule {
            id: toml::require_str(t, "id")?,
            version: toml::require_str(t, "version")?,
            regulated: toml::require_bool(t, "regulated")?,
            installed: toml::require_bool(t, "installed")?,
            enabled: toml::require_bool(t, "enabled")?,
        };
        if !row.installed {
            return Err(Error::Profile(format!(
                "module {} must have installed = true",
                row.id
            )));
        }
        out.push(row);
    }
    Ok(out)
}

fn parse_numbering(root: &BTreeMap<String, Value>) -> Result<BTreeMap<String, NumberingSpec>> {
    let table = toml::require_table(root, "numbering")?;
    let mut out = BTreeMap::new();
    for (doc, v) in table {
        let t = v
            .as_table()
            .ok_or_else(|| Error::Profile(format!("numbering.{doc} is not a table")))?;
        out.insert(
            doc.clone(),
            NumberingSpec {
                format: toml::require_str(t, "format")?,
                prefix: toml::require_str(t, "prefix")?,
                scope: toml::require_str(t, "scope")?,
                gap_free: toml::require_bool(t, "gap_free")?,
                reset: toml::require_str(t, "reset")?,
            },
        );
    }
    Ok(out)
}

fn parse_bundles(sp: &BTreeMap<String, Value>) -> Result<Vec<ProfileBundle>> {
    let Some(arr) = sp.get("bundles").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in arr {
        let t = item
            .as_table()
            .ok_or_else(|| Error::Profile("bundle is not a table".into()))?;
        out.push(ProfileBundle {
            name: toml::require_str(t, "name")?,
            permissions: match t.get("permissions") {
                Some(v) => toml::string_array(v)?,
                None => Vec::new(),
            },
        });
    }
    Ok(out)
}

fn root_array(table: &BTreeMap<String, Value>, key: &str) -> Result<Vec<String>> {
    match table.get(key) {
        Some(v) => toml::string_array(v),
        None => Ok(Vec::new()),
    }
}

/// Keys whose dumped values differ.
pub fn delta_keys(
    a: &BTreeMap<String, serde_json::Value>,
    b: &BTreeMap<String, serde_json::Value>,
) -> Vec<String> {
    let mut keys: BTreeSet<String> = a.keys().cloned().collect();
    keys.extend(b.keys().cloned());
    keys.into_iter().filter(|k| a.get(k) != b.get(k)).collect()
}

/// `plain-shop` must not rewrite a signature declaration that `regulated-device` owns.
pub fn profile_does_not_rewrite_edges(regulated: &Profile, plain: &Profile) -> bool {
    // Enablement is the only lever: every Required edge on the plain profile
    // would be a rewrite. The generated sets are compared by the caller;
    // this helper is the structural claim on the TOML-authored enablement.
    !plain.modules.iter().any(|m| m.regulated && m.enabled)
        && regulated.modules.iter().all(|rm| {
            plain
                .modules
                .iter()
                .find(|pm| pm.id == rm.id)
                .is_none_or(|pm| pm.regulated == rm.regulated)
        })
}
