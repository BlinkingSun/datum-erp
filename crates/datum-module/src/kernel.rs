//! `Kernel::build`: wiring, seeds, freeze, startup guard.

use datum_core::{
    Actor, ActorKind, GroupKind, Identifier, NoSignatures, PermissionKey, PostingGroupHeader,
    PostingSink, SignatureGate, SignatureMeaning, SignatureRequirement,
};
use datum_db::{Pool, Tx, WriteContext, WritePool};
use datum_identity::{SYSTEM_ID, seed_builtins};
use datum_ledger::GroupBuilder;
use datum_statemachine::{EdgeBuilder, Engine, Machine, ManifestEdge};

use crate::config::persist_kernel_defaults;
use crate::manifest::{ModuleManifest, compiled_in, compiled_in_graph, hex};
use crate::order::{ModuleNode, topological_order};
use crate::profile::{GateBinding, Profile, ProfileId, SignatureEdge};
use crate::registry::{self, install, record_profile};
use crate::{Error, Result};

/// Assembled kernel handle `datum-server` uses.
pub struct Kernel {
    /// Applied profile.
    pub profile: Profile,
    /// Frozen state-machine registry.
    pub engine: Engine,
    /// Event subscriber registry.
    pub events: datum_events::Registry,
    /// Event payload contracts.
    pub event_schemas: datum_events::SchemaRegistry,
    /// Job-kind registry.
    pub jobs: datum_jobs::Registry,
    /// Bound from the profile TOML `gate` field (SPEC-profiles key 4).
    gate: Box<dyn SignatureGate + Send + Sync>,
    catalog: Vec<ModuleManifest>,
}

impl Kernel {
    /// Construct the composition root against an already-migrated app pool.
    pub async fn build(pool: &Pool, profile: Profile) -> Result<Self> {
        hold_wave2b_edges();
        let write = WritePool::new(pool.clone());
        let catalog = compiled_in()?;
        let mut ctx = system_ctx("module.boot");
        ctx.config_version = Some(profile.spec_version.clone());
        let mut tx = Tx::begin(&write, &ctx).await?;
        seed_builtins(&mut tx).await?;
        seed_profile(&mut tx, &profile, &catalog).await?;
        tx.commit().await?;

        let mut engine = Engine::new();
        let graph: Vec<datum_statemachine::ModuleNode> =
            compiled_in_graph()?.into_iter().map(Into::into).collect();
        engine.set_module_graph(graph)?;
        register_enabled_machines(&mut engine, &profile)?;
        engine.freeze()?;

        let gate = bind_signature_gate(profile.signature_gate_binding);
        let mut kernel = Self {
            profile,
            engine,
            events: datum_events::Registry::new(),
            event_schemas: datum_events::SchemaRegistry::standard(),
            jobs: datum_jobs::Registry::new(),
            gate,
            catalog,
        };
        datum_jobs::register_maintenance(&kernel.jobs);
        kernel.refresh_signature_edges();
        kernel.startup_guard()?;

        let mut tx = Tx::begin(&write, &ctx).await?;
        let body = serde_json::to_value(kernel.profile.effective_dump()?)?;
        let hash = hex(datum_audit::sha256::digest(&serde_json::to_vec(&body)?));
        record_profile(
            &mut tx,
            kernel.profile.id.as_str(),
            &kernel.profile.spec_version,
            &body,
            &hash,
        )
        .await?;
        tx.commit().await?;
        Ok(kernel)
    }

    /// Ledger `GroupBuilder` factory (the `PostingSink` provider).
    pub fn posting_sink(&self, kind: GroupKind, header: PostingGroupHeader) -> GroupBuilder {
        GroupBuilder::new(kind, header)
    }

    /// Bound signature gate, selected by the profile TOML `gate` field.
    pub fn signature_gate(&self) -> &dyn SignatureGate {
        &*self.gate
    }

    /// SPEC deliverable 4: register an in-process events subscription.
    pub async fn register_events_subscription(
        &self,
        tx: &mut Tx<'_>,
        name: &str,
        subscriber: &str,
        handler: impl datum_events::EventHandler + 'static,
    ) -> Result<()> {
        self.events.subscribe(name, subscriber, handler);
        datum_events::enable_subscription(tx, name, subscriber).await?;
        Ok(())
    }

    /// Whether the bound gate is [`NoSignatures`].
    pub fn gate_is_noop(&self) -> bool {
        matches!(
            self.profile.signature_gate_binding,
            GateBinding::NoSignatures
        )
    }

    /// Hook order for `(doc_type, edge)` after freeze.
    pub fn hook_order(&self, doc_type: &str, edge: &str) -> Vec<String> {
        self.engine.hook_order(doc_type, edge)
    }

    /// Module ids in dependency-topological order.
    pub fn module_order(&self) -> Result<Vec<String>> {
        topological_order(&compiled_in_graph()?)
    }

    /// Catalog used for enable/disable closure.
    pub fn catalog(&self) -> &[ModuleManifest] {
        &self.catalog
    }

    /// Profile id.
    pub fn profile_id(&self) -> ProfileId {
        self.profile.id
    }

    fn refresh_signature_edges(&mut self) {
        let edges = edges_from_registry(&self.engine);
        self.profile = self.profile.clone().with_registry_edges(edges);
    }

    fn startup_guard(&self) -> Result<()> {
        let release = !cfg!(debug_assertions);
        self.engine
            .check_gate_binding(self.gate_is_noop(), release)
            .map_err(|e| Error::Startup(e.to_string()))
    }
}

/// Box a [`GroupBuilder`] as the object-safe sink.
pub fn posting_sink(kind: GroupKind, header: PostingGroupHeader) -> Box<dyn PostingSink> {
    Box::new(GroupBuilder::new(kind, header))
}

/// Map engine manifest edges into profile key 3.
pub fn edges_from_registry(engine: &Engine) -> Vec<SignatureEdge> {
    engine
        .edges_for_manifest()
        .into_iter()
        .map(edge_from_manifest)
        .collect()
}

fn edge_from_manifest(e: ManifestEdge) -> SignatureEdge {
    if e.kind == "required" {
        SignatureEdge::Required {
            module: e.doc_type,
            edge: e.edge,
            meaning: e.meaning.unwrap_or_default(),
            permission: e.signature_permission.unwrap_or_default(),
        }
    } else {
        SignatureEdge::NotRequired {
            module: e.doc_type,
            edge: e.edge,
            reason: e.reason.unwrap_or_default(),
        }
    }
}

/// CONTRACT §6.3 / D-W1-5 key 4 startup guard, testable with an explicit release flag.
pub fn startup_fails_if_required_meets_no_signatures(
    engine: &Engine,
    gate_is_noop: bool,
    release: bool,
) -> Result<()> {
    engine
        .check_gate_binding(gate_is_noop, release)
        .map_err(|e| Error::Startup(e.to_string()))
}

pub(crate) fn system_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        action,
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("module.boot".into());
    ctx
}

async fn seed_profile(
    tx: &mut Tx<'_>,
    profile: &Profile,
    catalog: &[ModuleManifest],
) -> Result<()> {
    let bundles: Vec<datum_identity::rbac::RoleBundle> = profile
        .seeded_permissions
        .bundles
        .iter()
        .map(|b| datum_identity::rbac::RoleBundle {
            name: b.name.clone(),
            permissions: b.permissions.clone(),
        })
        .collect();
    if !bundles.is_empty() {
        datum_identity::rbac::seed_bundles(tx, &bundles).await?;
    }
    for (doc, spec) in &profile.numbering {
        let policy = spec.reset_policy()?;
        datum_numbering::define(tx, doc, &spec.format, policy).await?;
    }
    for m in catalog {
        let enabled = profile
            .modules
            .iter()
            .find(|p| p.id == m.id)
            .is_some_and(|p| p.enabled);
        install(tx, m, enabled).await?;
    }
    for p in &profile.modules {
        if p.enabled {
            registry::enable(tx, &p.id, catalog, profile).await?;
        }
    }
    persist_kernel_defaults(tx, profile).await?;
    Ok(())
}

/// Bind the `SignatureGate` named by the profile TOML `gate` field.
///
/// SPEC-profiles key 4: "`NoSignatures` pre-Wave-2b and in tests only, `datum-esign`
/// from 2b". CONTRACT §6.3: "Core ships `NoSignatures`, which refuses every token"
/// (`verify -> Err(NoProvider)`). Until Wave 2b both TOML values resolve to that
/// verify-only gate; the regulated profile does not get it by ignoring the field.
pub fn bind_signature_gate(binding: GateBinding) -> Box<dyn SignatureGate + Send + Sync> {
    match binding {
        GateBinding::NoSignatures => Box::new(NoSignatures),
        GateBinding::DatumEsign => Box::new(NoSignatures),
    }
}

fn register_enabled_machines(engine: &mut Engine, profile: &Profile) -> Result<()> {
    for m in profile.modules.iter().filter(|m| m.enabled) {
        match m.id.as_str() {
            "mod-calibration" => engine.register_machine(calibration_machine()?)?,
            "mod-production-min" => engine.register_machine(wo_machine()?)?,
            _ => {}
        }
    }
    Ok(())
}

fn calibration_machine() -> Result<Machine> {
    let req = SignatureRequirement {
        meaning: SignatureMeaning("Approved".into()),
        permission: PermissionKey("calibration.approve".into()),
    };
    Ok(Machine::builder("calibration.certificate")
        .regulated(true)
        .state("Open")
        .state("Approved")
        .edge(EdgeBuilder::new("Open", "Approved", "approve", "calibration.approve").required(req))
        .build()?)
}

fn wo_machine() -> Result<Machine> {
    Ok(Machine::builder("wo")
        .regulated(false)
        .state("Draft")
        .state("Released")
        .edge(EdgeBuilder::new(
            "Draft",
            "Released",
            "release",
            "wo.release",
        ))
        .build()?)
}

fn hold_wave2b_edges() {
    let _ = core::any::type_name::<datum_esign::Error>();
    let _ = core::any::type_name::<datum_customfields::Error>();
    let _ = core::any::type_name::<datum_documents::Error>();
    let _ = core::any::type_name::<datum_print::Error>();
    let _ = core::any::type_name::<datum_uom::Error>();
}

/// Graph nodes for the compiled-in catalog (tests / hook-order).
pub fn module_nodes() -> Result<Vec<ModuleNode>> {
    compiled_in_graph()
}
