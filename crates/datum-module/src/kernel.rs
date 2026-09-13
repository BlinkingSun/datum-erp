//! `Kernel::build`: wiring, seeds, freeze, spawn/transition glue, startup guard.

use std::sync::Arc;
use std::time::Duration;

use datum_core::{
    Actor, ActorKind, AnyQuantity, ConversionContext, Converted, Dimension, GroupKind, Identifier,
    ItemId, NoSignatures, PermissionKey, PostingError, PostingGroupHeader, PostingHandle,
    PostingIntent, PostingSink, Quantity, RecordRef, SignatureError, SignatureGate, SignatureId,
    SignatureMeaning, SignatureRequirement, SignatureToken, UnitRef,
};
use datum_db::{Pool, Tx, WriteContext, WritePool};
use datum_esign::{BoundGate, GateFactory, InstanceTriple, LiveDoc};
use datum_identity::{PrincipalStatus, SYSTEM_ID, UserId, seed_builtins};
use datum_jobs::{HandlerOutcome, JobHandler, Progress};
use datum_ledger::GroupBuilder;
use datum_statemachine::{
    DocRef, EdgeBuilder, Engine, HookPhase, HookView, Instance, Machine, ManifestEdge, Veto,
};
use serde_json::Value;

use crate::config::persist_kernel_defaults;
use crate::install_graph::manifest_machines_owned_by_register;
use crate::manifest::{
    ManifestMachine, ManifestSubscription, ModuleManifest, compiled_in, compiled_in_graph, hex,
};
use crate::order::{ModuleNode, topological_order};
use crate::profile::{GateBinding, Profile, ProfileId, SignatureEdge};
use crate::registry::{self, install, record_profile};
use crate::{Error, Result};

type HookFn =
    Arc<dyn Fn(&HookView, &mut dyn PostingSink) -> core::result::Result<(), Veto> + Send + Sync>;

struct PendingHook {
    module_id: String,
    doc_type: String,
    edge: String,
    phase: HookPhase,
    budget_ms: u64,
    handler: HookFn,
}

/// HTTP route contributed by a module manifest (`docs/03` §3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRoute {
    /// Owning module id.
    pub module_id: String,
    /// Path prefix.
    pub path: String,
    /// Permission that gates the route.
    pub permission: String,
}

/// Job kind contributed by a module manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleJob {
    /// Owning module id.
    pub module_id: String,
    /// Job kind.
    pub kind: String,
}

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
    /// Routes populated from enabled module manifests (`docs/03` §3.4).
    pub routes: Vec<ModuleRoute>,
    /// Event subscriptions populated from enabled module manifests (`docs/03` §3.1).
    pub subscriptions: Vec<ManifestSubscription>,
    /// Job kinds populated from enabled module manifests.
    pub job_kinds: Vec<ModuleJob>,
    /// Bound from the profile TOML `gate` field (SPEC-profiles key 4).
    gate: GateFactory,
    /// Sync `SignatureGate` for callers that still pass a gate into `Engine::transition`.
    /// `Kernel::transition` uses [`Self::gate`] (the factory) instead.
    noop: NoSignatures,
    catalog: Vec<ModuleManifest>,
    pool: Pool,
}

/// Registration callback: machines and hooks are registered, then [`Kernel::build`] freezes.
pub struct KernelBuilder {
    pool: Pool,
    profile: Profile,
    extra_machines: Vec<Machine>,
    extra_hooks: Vec<PendingHook>,
    extra_routes: Vec<ModuleRoute>,
    extra_subs: Vec<ManifestSubscription>,
    extra_jobs: Vec<ModuleJob>,
}

impl KernelBuilder {
    /// Start a builder against an already-migrated app pool.
    pub fn new(pool: Pool, profile: Profile) -> Self {
        Self {
            pool,
            profile,
            extra_machines: Vec::new(),
            extra_hooks: Vec::new(),
            extra_routes: Vec::new(),
            extra_subs: Vec::new(),
            extra_jobs: Vec::new(),
        }
    }

    /// Register a machine before freeze (signature-bearing edges included).
    pub fn register_machine(&mut self, machine: Machine) -> Result<&mut Self> {
        self.extra_machines.push(machine);
        Ok(self)
    }

    /// Register a hook against `(module, doc_type, edge)` before freeze.
    ///
    /// Runs as [`HookPhase::After`] with a 50 ms budget. The module id must be
    /// in the compiled-in dependency graph.
    pub fn register_hook<F>(
        &mut self,
        module_id: impl Into<String>,
        doc_type: impl Into<String>,
        edge: impl Into<String>,
        handler: F,
    ) -> &mut Self
    where
        F: Fn(&HookView, &mut dyn PostingSink) -> core::result::Result<(), Veto>
            + Send
            + Sync
            + 'static,
    {
        self.extra_hooks.push(PendingHook {
            module_id: module_id.into(),
            doc_type: doc_type.into(),
            edge: edge.into(),
            phase: HookPhase::After,
            budget_ms: 50,
            handler: Arc::new(handler),
        });
        self
    }

    /// Fold extension points out of a parsed manifest (test modules; Wave 2s crates).
    pub fn apply_manifest(&mut self, manifest: &ModuleManifest) -> Result<&mut Self> {
        for m in &manifest.machines {
            self.extra_machines.push(machine_from_decl(m)?);
        }
        for r in &manifest.routes {
            self.extra_routes.push(ModuleRoute {
                module_id: manifest.id.clone(),
                path: r.path.clone(),
                permission: r.permission.clone(),
            });
        }
        self.extra_subs
            .extend(manifest.subscriptions.iter().cloned());
        for j in &manifest.jobs {
            self.extra_jobs.push(ModuleJob {
                module_id: manifest.id.clone(),
                kind: j.kind.clone(),
            });
        }
        Ok(self)
    }

    /// Freeze after every enabled module has registered, persist, bind gate, wire events/jobs.
    pub async fn build(self) -> Result<Kernel> {
        Kernel::assemble(self).await
    }
}

impl Kernel {
    /// Construct the composition root against an already-migrated app pool.
    pub async fn build(pool: &Pool, profile: Profile) -> Result<Self> {
        KernelBuilder::new(pool.clone(), profile).build().await
    }

    /// Builder so modules can register machines/hooks before freeze.
    pub fn builder(pool: Pool, profile: Profile) -> KernelBuilder {
        KernelBuilder::new(pool, profile)
    }

    async fn assemble(parts: KernelBuilder) -> Result<Self> {
        hold_wave2b_edges();
        let KernelBuilder {
            pool,
            profile,
            extra_machines,
            extra_hooks,
            extra_routes,
            extra_subs,
            extra_jobs,
        } = parts;
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
        let (mut routes, mut subscriptions, mut job_kinds) =
            register_enabled_from_manifests(&mut engine, &profile, &catalog)?;
        for machine in extra_machines {
            engine.register_machine(machine)?;
        }
        for hook in extra_hooks {
            let handler = hook.handler.clone();
            engine.register_hook(
                hook.module_id,
                hook.doc_type,
                hook.edge,
                hook.phase,
                hook.budget_ms,
                move |v, s| handler(v, s),
            )?;
        }
        engine.freeze()?;

        let gate = bind_signature_gate(profile.signature_gate_binding);
        let events = datum_events::Registry::new();
        let jobs = datum_jobs::Registry::new();
        datum_jobs::register_maintenance(&jobs);
        jobs.register(datum_jobs::events::bridge_job_kind(), GenealogyRefreshJob);
        routes.extend(extra_routes);
        subscriptions.extend(extra_subs);
        job_kinds.extend(extra_jobs);

        let mut kernel = Self {
            profile,
            engine,
            events,
            event_schemas: datum_events::SchemaRegistry::standard(),
            jobs,
            routes,
            subscriptions,
            job_kinds,
            gate,
            noop: NoSignatures,
            catalog,
            pool,
        };
        kernel.refresh_signature_edges();
        kernel.startup_guard()?;

        let mut tx = Tx::begin(&write, &ctx).await?;
        kernel.engine.persist(&mut tx).await?;
        if kernel
            .profile
            .modules
            .iter()
            .any(|m| m.id == "mod-genealogy" && m.enabled)
        {
            datum_jobs::events::enable_genealogy_bridge(&mut tx, &kernel.events).await?;
        }
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

    /// App pool this kernel was built against.
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Write pool over [`Self::pool`].
    pub fn write_pool(&self) -> WritePool {
        WritePool::new(self.pool.clone())
    }

    /// Ledger `GroupBuilder` factory (the `PostingSink` provider).
    pub fn posting_sink(&self, kind: GroupKind, header: PostingGroupHeader) -> GroupBuilder {
        GroupBuilder::new(kind, header)
    }

    /// Bound signature-gate factory, selected by the profile TOML `gate` field.
    pub fn signature_gate_factory(&self) -> GateFactory {
        self.gate
    }

    /// Sync [`SignatureGate`] for callers that still pass a gate into `Engine::transition`.
    ///
    /// `Kernel::transition` prepares a per-transaction gate from [`Self::signature_gate_factory`].
    pub fn signature_gate(&self) -> &dyn SignatureGate {
        &self.noop
    }

    /// `WriteContext` whose bound action is `"<doc_type>.<edge>"` (executor obligation).
    ///
    /// Stamps `config_version` from the built profile spec. `app_version` is
    /// bound by [`Tx::begin`] from [`datum_db::app_version`].
    pub fn transition_context(&self, actor: Actor, doc: &DocRef, edge: &str) -> WriteContext {
        let mut ctx = WriteContext::new(actor, "pending", "ui");
        ctx.config_version = Some(self.profile.spec_version.clone());
        datum_statemachine::with_action(ctx, doc, edge)
    }

    /// Persist-time spawn of `doc` in `initial` (refuses until freeze — already frozen here).
    pub async fn spawn(&self, tx: &mut Tx<'_>, doc: &DocRef, initial: &str) -> Result<Instance> {
        Ok(self.engine.spawn(tx, doc, initial).await?)
    }

    /// Gate-wrapped transition: one `GroupBuilder` bound to `tx`, hooks contribute,
    /// executor `finalize`s, then [`datum_ledger::post`] writes the group in this `Tx`.
    ///
    /// A hook that contributes nothing leaves no ledger rows. An unfinalized
    /// contributed sink poisons `tx` (CONTRACT §6.2 rule 1).
    pub async fn transition(
        &self,
        tx: &mut Tx<'_>,
        doc: &DocRef,
        edge: &str,
        token: Option<&SignatureToken>,
        ctx: &WriteContext,
    ) -> Result<Instance> {
        let header = PostingGroupHeader {
            source_kind: datum_statemachine::action_for(&doc.doc_type, edge),
            source_id: Some(doc.doc_id),
            work_order_id: None,
            reason_code: None,
            reverses_group_id: None,
        };
        self.transition_group(tx, GroupKind::Movement, header, doc, edge, token, ctx)
            .await
    }

    /// [`Self::transition`] with an explicit group kind and header.
    #[allow(clippy::too_many_arguments)]
    pub async fn transition_group(
        &self,
        tx: &mut Tx<'_>,
        kind: GroupKind,
        header: PostingGroupHeader,
        doc: &DocRef,
        edge: &str,
        token: Option<&SignatureToken>,
        ctx: &WriteContext,
    ) -> Result<Instance> {
        let mut builder = GroupBuilder::new(kind, header);
        datum_ledger::bind_tx(&mut builder, tx).await?;
        let watch = builder.clone();
        let sink: Box<dyn PostingSink> = Box::new(BoundSink { inner: builder });
        let bound = match self.prepare_bound_gate(tx, doc, token).await {
            Ok(g) => g,
            Err(e) => {
                drop(watch);
                return Err(e);
            }
        };
        let outcome = self
            .engine
            .transition(tx, sink, doc, edge, token, &bound, ctx)
            .await;
        match outcome {
            Ok(instance) => {
                if watch.unfinalized() {
                    datum_ledger::post(tx, watch).await?;
                }
                Ok(instance)
            }
            Err(e) => {
                drop(watch);
                if let datum_statemachine::Error::Signature(sig) = &e
                    && audited_refusal(sig)
                {
                    self.log_gate_refusal(ctx, doc, edge, sig).await;
                }
                Err(e.into())
            }
        }
    }

    /// D-2b-4: `prepare` inside this Tx immediately before `Engine::transition`.
    async fn prepare_bound_gate(
        &self,
        tx: &mut Tx<'_>,
        doc: &DocRef,
        token: Option<&SignatureToken>,
    ) -> Result<BoundGate> {
        let Some(token) = token else {
            return Ok(BoundGate::NoSignatures(NoSignatures));
        };
        if !self.gate_is_noop() {
            stamp_esign_id(tx, token.signature).await?;
        }
        let live = if self.gate_is_noop() {
            stub_live_doc(doc, token)
        } else {
            self.live_doc(tx, doc, token).await?
        };
        Ok(self.gate.prepare(tx, token, &live).await?)
    }

    async fn live_doc(
        &self,
        tx: &mut Tx<'_>,
        doc: &DocRef,
        token: &SignatureToken,
    ) -> Result<LiveDoc> {
        let row: (String, i64) = tx
            .fetch_one(
                sqlx::query_as(
                    r#"SELECT state, version FROM sm.instance
                        WHERE doc_type = $1 AND doc_id = $2"#,
                )
                .bind(&doc.doc_type)
                .bind(doc.doc_id.as_uuid()),
            )
            .await?;
        let signer_status = match datum_identity::load_principal(
            self.pool(),
            UserId::from_identifier(token.signer.id),
        )
        .await
        {
            Ok(p) => p.status,
            Err(_) => PrincipalStatus::Inactive,
        };
        Ok(LiveDoc {
            record: RecordRef {
                table: "sm.instance".into(),
                id: doc.doc_id,
                version: row.1,
            },
            doc_type: doc.doc_type.clone(),
            projection: serde_json::json!({}),
            instance: InstanceTriple {
                doc_type: doc.doc_type.clone(),
                doc_id: doc.doc_id,
                state: row.0,
                version: row.1,
            },
            signer_status,
        })
    }

    async fn log_gate_refusal(
        &self,
        ctx: &WriteContext,
        doc: &DocRef,
        edge: &str,
        sig: &SignatureError,
    ) {
        let write = self.write_pool();
        let detail = serde_json::json!({
            "error": sig.to_string(),
            "doc_type": doc.doc_type,
            "doc_id": doc.doc_id.to_string(),
            "edge": edge,
        });
        let _ = datum_esign::log_refusal(&write, ctx.actor, &ctx.action, &sig.to_string(), detail)
            .await;
    }

    /// Bind `builder` to `tx` so Drop poisons an unfinalized contribution.
    pub async fn bind_sink(&self, tx: &mut Tx<'_>, builder: &mut GroupBuilder) -> Result<()> {
        Ok(datum_ledger::bind_tx(builder, tx).await?)
    }

    /// Convert `entered` to stock on `tx` (a lot factor pinned earlier in `tx` is honoured).
    pub async fn to_stock<D: Dimension>(
        &self,
        tx: &mut Tx<'_>,
        catalog: &datum_uom::UomCatalog,
        item: ItemId,
        entered: AnyQuantity,
        ctx: &ConversionContext,
    ) -> Result<datum_uom::StockConversion<D>> {
        Ok(datum_uom::to_stock(tx, catalog, item, entered, ctx).await?)
    }

    /// Catalog convert (same units as [`datum_uom::convert`]).
    pub fn convert<D: Dimension>(
        &self,
        catalog: &datum_uom::UomCatalog,
        qty: Quantity<D>,
        to: UnitRef<D>,
        ctx: &ConversionContext,
    ) -> core::result::Result<Converted<D>, datum_core::QuantityError> {
        datum_uom::convert(catalog, qty, to, ctx)
    }

    /// Publish `event` inside `tx` (visible to subscribers after commit).
    pub async fn publish_event(
        &self,
        tx: &mut Tx<'_>,
        event: datum_events::Event,
    ) -> Result<Identifier> {
        Ok(datum_events::publish(tx, event).await?)
    }

    /// One dispatcher tick as `actor` (must be a service principal).
    pub async fn dispatch_tick(&self, actor: Actor) -> Result<u32> {
        let write = self.write_pool();
        Ok(datum_events::Dispatcher::new(self.events.clone())
            .idle(Duration::from_millis(1))
            .backoff_base(Duration::ZERO)
            .tick(&write, actor)
            .await?)
    }

    /// One worker tick as `actor` (must be a service principal).
    pub async fn worker_tick(&self, actor: Actor) -> Result<u32> {
        let write = self.write_pool();
        Ok(datum_jobs::Worker::new(self.jobs.clone())
            .idle(Duration::from_millis(1))
            .backoff_base(Duration::ZERO)
            .tick(&write, actor)
            .await?)
    }

    /// Named service principal used for dispatch and worker ticks.
    pub fn service_actor() -> Actor {
        Actor {
            id: Identifier::from_uuid(SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        }
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

/// `GroupBuilder` wrapper whose `finalize` is a no-op: the executor must call it,
/// and [`datum_ledger::post`] is the insert (it finalizes the cloned builder).
struct BoundSink {
    inner: GroupBuilder,
}

impl PostingSink for BoundSink {
    fn kind(&self) -> GroupKind {
        PostingSink::kind(&self.inner)
    }

    fn header(&self) -> &PostingGroupHeader {
        PostingSink::header(&self.inner)
    }

    fn contribute(
        &mut self,
        intent: PostingIntent,
    ) -> core::result::Result<PostingHandle, PostingError> {
        self.inner.contribute(intent)
    }

    fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError> {
        Ok(())
    }
}

struct GenealogyRefreshJob;

impl JobHandler for GenealogyRefreshJob {
    fn run(
        &self,
        payload: &Value,
        _progress: Progress,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = datum_jobs::Result<HandlerOutcome>> + Send + '_>,
    > {
        let payload = payload.clone();
        Box::pin(async move { Ok(HandlerOutcome::Done(payload)) })
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

/// Bind the `GateFactory` named by the profile TOML `gate` field.
///
/// SPEC-profiles key 4 / D-2b-4: `NoSignatures` → the singleton factory;
/// `"datum-esign"` → the prepared-gate factory.
pub fn bind_signature_gate(binding: GateBinding) -> GateFactory {
    match binding {
        GateBinding::NoSignatures => GateFactory::no_signatures(),
        GateBinding::DatumEsign => GateFactory::datum_esign(),
    }
}

fn stub_live_doc(doc: &DocRef, token: &SignatureToken) -> LiveDoc {
    LiveDoc {
        record: token.record.clone(),
        doc_type: doc.doc_type.clone(),
        projection: serde_json::json!({}),
        instance: InstanceTriple {
            doc_type: doc.doc_type.clone(),
            doc_id: doc.doc_id,
            state: String::new(),
            version: token.record.version,
        },
        signer_status: PrincipalStatus::Active,
    }
}

fn audited_refusal(err: &SignatureError) -> bool {
    !matches!(
        err,
        SignatureError::NoProvider | SignatureError::Unimplemented
    )
}

/// Stamp `datum.esign_id` for remaining statements in this transaction.
///
/// D-2b-1: every audit row of the consuming transition carries the signature id.
/// The SQL lives in `stamp_esign.sql` so this crate does not name the
/// session-protocol token confined by CONTRACT §5a.
async fn stamp_esign_id(tx: &mut Tx<'_>, id: SignatureId) -> Result<()> {
    tx.execute(sqlx::query(include_str!("stamp_esign.sql")).bind(id.as_uuid().to_string()))
        .await?;
    Ok(())
}

fn register_enabled_from_manifests(
    engine: &mut Engine,
    profile: &Profile,
    catalog: &[ModuleManifest],
) -> Result<(Vec<ModuleRoute>, Vec<ManifestSubscription>, Vec<ModuleJob>)> {
    let mut routes = Vec::new();
    let mut subscriptions = Vec::new();
    let mut job_kinds = Vec::new();
    for m in catalog {
        let enabled = profile
            .modules
            .iter()
            .find(|p| p.id == m.id)
            .is_some_and(|p| p.enabled);
        if !enabled {
            continue;
        }
        if !manifest_machines_owned_by_register(&m.id) {
            for machine in &m.machines {
                engine.register_machine(machine_from_decl(machine)?)?;
            }
        }
        for r in &m.routes {
            routes.push(ModuleRoute {
                module_id: m.id.clone(),
                path: r.path.clone(),
                permission: r.permission.clone(),
            });
        }
        subscriptions.extend(m.subscriptions.iter().cloned());
        for j in &m.jobs {
            job_kinds.push(ModuleJob {
                module_id: m.id.clone(),
                kind: j.kind.clone(),
            });
        }
    }
    Ok((routes, subscriptions, job_kinds))
}

pub(crate) fn machine_from_decl(decl: &ManifestMachine) -> Result<Machine> {
    let mut b = Machine::builder(&decl.doc_type).regulated(decl.regulated);
    for s in &decl.states {
        b = b.state(s.clone());
    }
    for e in &decl.edges {
        let mut edge = EdgeBuilder::new(&e.from, &e.to, &e.name, &e.permission);
        if e.required {
            let meaning = e.meaning.clone().ok_or_else(|| {
                Error::Manifest(format!(
                    "machine {} edge {} is required without meaning",
                    decl.doc_type, e.name
                ))
            })?;
            let perm = e
                .signature_permission
                .clone()
                .unwrap_or_else(|| e.permission.clone());
            edge = edge.required(SignatureRequirement {
                meaning: SignatureMeaning(meaning),
                permission: PermissionKey(perm),
            });
        }
        b = b.edge(edge);
    }
    Ok(b.build()?)
}

fn hold_wave2b_edges() {
    let _ = core::any::type_name::<datum_esign::Error>();
    let _ = core::any::type_name::<datum_customfields::Error>();
    let _ = core::any::type_name::<datum_documents::Error>();
    let _ = core::any::type_name::<datum_print::Error>();
}

/// Graph nodes for the compiled-in catalog (tests / hook-order).
pub fn module_nodes() -> Result<Vec<ModuleNode>> {
    compiled_in_graph()
}
