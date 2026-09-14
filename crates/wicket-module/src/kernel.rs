//! `Kernel::build`: wiring, seeds, freeze, spawn/transition glue, startup guard.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::Value;
use wicket_core::{
    Actor, ActorKind, AnyQuantity, ConversionContext, Converted, Dimension, GroupKind, Identifier,
    ItemId, NoSignatures, PermissionKey, PostingError, PostingGroupHeader, PostingHandle,
    PostingIntent, PostingSink, Quantity, RecordRef, SignatureError, SignatureGate, SignatureId,
    SignatureMeaning, SignatureRequirement, SignatureToken, UnitRef,
};
use wicket_db::{Pool, Tx, WriteContext, WritePool};
use wicket_esign::{BoundGate, GateFactory, InstanceTriple, LiveDoc};
use wicket_identity::{PrincipalStatus, SYSTEM_ID, UserId, load_principal_on, seed_builtins};
use wicket_jobs::{HandlerOutcome, JobHandler, Progress};
use wicket_ledger::GroupBuilder;
use wicket_statemachine::{
    DocRef, EdgeBuilder, Engine, HookPhase, HookView, Instance, Machine, ManifestEdge, Veto,
};

use crate::config::persist_kernel_defaults;
use crate::documents::{
    emit_effective, emit_revision_created, register_document_event_schemas,
    register_document_machine,
};
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
    /// HTTP method (`GET`, `POST`, `PATCH`, …).
    pub method: String,
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

/// Assembled kernel handle `wicket-server` uses.
pub struct Kernel {
    /// Applied profile.
    pub profile: Profile,
    /// Frozen state-machine registry.
    pub engine: Engine,
    /// Event subscriber registry.
    pub events: wicket_events::Registry,
    /// Event payload contracts.
    pub event_schemas: wicket_events::SchemaRegistry,
    /// Job-kind registry.
    pub jobs: wicket_jobs::Registry,
    /// Routes populated from enabled module manifests (`docs/03` §3.4).
    pub routes: Vec<ModuleRoute>,
    /// Event subscriptions populated from enabled module manifests (`docs/03` §3.1).
    pub subscriptions: Vec<ManifestSubscription>,
    /// Job kinds populated from enabled module manifests.
    pub job_kinds: Vec<ModuleJob>,
    /// Bound from the profile TOML `gate` field (SPEC-profiles key 4).
    gate: GateFactory,
    /// Profile-bound sync gate for `Engine::transition` callers (inventory, production).
    /// [`Kernel::transition`] prepares a per-Tx gate from [`Self::gate`] instead.
    bound: BoundSyncGate,
    catalog: Vec<ModuleManifest>,
    pool: Pool,
    /// In-process live business-record bodies (tests / module-pushed snapshots).
    live_bodies: Arc<Mutex<BTreeMap<(String, Identifier), Value>>>,
}

/// Sync stand-in returned by [`Kernel::signature_gate`].
///
/// [`Kernel::transition`] still calls [`GateFactory::prepare`] inside the
/// transition Tx. This type exists so modules that pass
/// `kernel.signature_gate()` into `Engine::transition` observe the **bound**
/// provider: `NoSignatures` under plain-shop, and `Invalid` (never
/// `NoProvider`) when `wicket-esign` is bound.
#[derive(Debug, Clone, Copy)]
enum BoundSyncGate {
    NoSignatures(NoSignatures),
    WicketEsign,
}

impl BoundSyncGate {
    fn from_binding(binding: GateBinding) -> Self {
        match binding {
            GateBinding::NoSignatures => Self::NoSignatures(NoSignatures),
            GateBinding::WicketEsign => Self::WicketEsign,
        }
    }
}

impl SignatureGate for BoundSyncGate {
    fn verify(
        &self,
        token: &SignatureToken,
        required: &SignatureRequirement,
        record: &RecordRef,
    ) -> core::result::Result<(), SignatureError> {
        match self {
            Self::NoSignatures(g) => g.verify(token, required, record),
            Self::WicketEsign => Err(SignatureError::Invalid("no such signature".into())),
        }
    }
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
    extra_projections: BTreeMap<String, fn(&Value) -> Value>,
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
            extra_projections: BTreeMap::new(),
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

    /// Register the esign projection for `doc_type` (D-2b-3).
    ///
    /// Required for every extra bound machine (tests / `KernelBuilder::register_machine`
    /// types that are not first-party). A bound machine with neither this
    /// registration nor a first-party identity default fails [`Kernel::build`]
    /// with [`Error::MissingProjection`].
    pub fn register_projection(
        &mut self,
        doc_type: impl Into<String>,
        project: fn(&Value) -> Value,
    ) -> &mut Self {
        self.extra_projections.insert(doc_type.into(), project);
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
                method: r.method.clone(),
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
            extra_projections,
        } = parts;
        let write = WritePool::new(pool.clone());
        let catalog = compiled_in()?;
        let mut ctx = system_ctx("module.boot");
        ctx.config_version = Some(profile.spec_version.clone());
        let mut tx = Tx::begin(&write, &ctx).await?;
        seed_builtins(&mut tx).await?;
        seed_profile(&mut tx, &profile, &catalog).await?;
        // 11.50(b): stamp profile.id (never spec_version) and seed templates on
        // assemble's first Tx. Both calls are idempotent across Kernel::build restarts.
        wicket_print::seed_templates(&mut tx).await?;
        wicket_print::set_installation_profile(&mut tx, profile.id.as_str()).await?;
        tx.commit().await?;

        let mut engine = Engine::new();
        let graph: Vec<wicket_statemachine::ModuleNode> =
            compiled_in_graph()?.into_iter().map(Into::into).collect();
        engine.set_module_graph(graph)?;
        // Last extra_machine per doc_type wins. That lets a module's
        // register() overlay a profile-aware freeze (lots release is Required
        // only under regulated-device) on the TOML default without
        // KernelBuilder::register_machine being dropped as a duplicate.
        let mut extra_by_type: BTreeMap<String, Machine> = BTreeMap::new();
        for machine in extra_machines {
            extra_by_type.insert(machine.doc_type.clone(), machine);
        }
        let extra_types: BTreeSet<String> = extra_by_type.keys().cloned().collect();
        let (mut routes, mut subscriptions, mut job_kinds) =
            register_enabled_from_manifests(&mut engine, &profile, &catalog, &extra_types)?;
        register_document_machine(&mut engine, profile.id.as_str())?;
        engine.register_machine(wicket_customfields::definition_machine(
            profile.id.as_str(),
        )?)?;
        for machine in extra_by_type.into_values() {
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
        register_bound_projections(&engine, &extra_projections)?;

        let gate = bind_signature_gate(profile.signature_gate_binding);
        let bound = BoundSyncGate::from_binding(profile.signature_gate_binding);
        let events = wicket_events::Registry::new();
        let jobs = wicket_jobs::Registry::new();
        wicket_jobs::register_maintenance(&jobs);
        jobs.register(wicket_jobs::events::bridge_job_kind(), GenealogyRefreshJob);
        routes.extend(extra_routes);
        subscriptions.extend(extra_subs);
        job_kinds.extend(extra_jobs);

        let mut event_schemas = wicket_events::SchemaRegistry::standard();
        register_document_event_schemas(&mut event_schemas)?;

        let mut kernel = Self {
            profile,
            engine,
            events,
            event_schemas,
            jobs,
            routes,
            subscriptions,
            job_kinds,
            gate,
            bound,
            catalog,
            pool,
            live_bodies: Arc::new(Mutex::new(BTreeMap::new())),
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
            wicket_jobs::events::enable_genealogy_bridge(&mut tx, &kernel.events).await?;
        }
        let body = serde_json::to_value(kernel.profile.effective_dump()?)?;
        let hash = hex(wicket_audit::sha256::digest(&serde_json::to_vec(&body)?));
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

    /// Push a live business-record body for `doc_type`/`doc_id` (tests; module snapshots).
    ///
    /// [`Self::live_record`] prefers this over the default document/lot loaders.
    pub fn set_live_record(&self, doc_type: impl Into<String>, doc_id: Identifier, body: Value) {
        let mut guard = match self.live_bodies.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.insert((doc_type.into(), doc_id), body);
    }

    /// Live business-record JSON hashed with the `sm.instance` triple (D-2b-3).
    ///
    /// Mint and consume both call this so a body-only edit between them is
    /// [`wicket_core::SignatureError::HashMismatch`].
    pub async fn live_record(
        &self,
        tx: &mut Tx<'_>,
        doc_type: &str,
        doc_id: Identifier,
    ) -> Result<Value> {
        {
            let guard = match self.live_bodies.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if let Some(body) = guard.get(&(doc_type.to_owned(), doc_id)) {
                return Ok(body.clone());
            }
        }
        default_live_record(tx, doc_type, doc_id).await
    }

    /// Live `sm.instance` triple for minting (composition-root read; esign does not `SELECT sm.*`).
    pub async fn load_sm_instance(
        &self,
        tx: &mut Tx<'_>,
        doc_id: Identifier,
    ) -> Result<Option<(String, String, i64)>> {
        Ok(tx
            .fetch_optional(
                sqlx::query_as(
                    r#"SELECT doc_type, state, version FROM sm.instance
                        WHERE doc_id = $1"#,
                )
                .bind(doc_id.as_uuid()),
            )
            .await?)
    }

    /// Bound [`SignatureGate`] for callers that pass a gate into `Engine::transition`.
    ///
    /// Plain-shop: [`NoSignatures`]. Regulated-device: the esign binding (dummy /
    /// unprepared tokens are [`SignatureError::Invalid`], never `NoProvider`).
    /// [`Kernel::transition`] prepares a per-transaction gate from
    /// [`Self::signature_gate_factory`] so a real two-component token is claimed
    /// in the same Tx as `sm.instance` and audit.
    pub fn signature_gate(&self) -> &dyn SignatureGate {
        &self.bound
    }

    /// Load a [`SignatureToken`] for `X-Wicket-Signature` (composition-root read).
    pub async fn load_signature_token(
        &self,
        tx: &mut Tx<'_>,
        id: SignatureId,
    ) -> Result<Option<SignatureToken>> {
        let row: Option<(
            sqlx::types::Uuid,
            String,
            String,
            sqlx::types::Uuid,
            i64,
            Vec<u8>,
        )> = tx
            .fetch_optional(
                sqlx::query_as(
                    r#"SELECT signer_id, meaning, record_table, record_id, record_version,
                              record_content_hash
                         FROM esign.signature
                        WHERE signature_id = $1"#,
                )
                .bind(id.as_uuid()),
            )
            .await?;
        let Some((signer_id, meaning, table, rec_id, version, hash_bytes)) = row else {
            return Ok(None);
        };
        let mut record_content_hash = [0u8; 32];
        if hash_bytes.len() == 32 {
            record_content_hash.copy_from_slice(&hash_bytes);
        }
        Ok(Some(SignatureToken {
            signature: id,
            signer: Actor {
                id: Identifier::from_uuid(signer_id),
                kind: ActorKind::User,
            },
            meaning: SignatureMeaning(meaning),
            record: RecordRef {
                table,
                id: Identifier::from_uuid(rec_id),
                version,
            },
            record_content_hash,
        }))
    }

    /// `WriteContext` whose bound action is `"<doc_type>.<edge>"` (executor obligation).
    ///
    /// Stamps `config_version` from the built profile spec. `app_version` is
    /// bound by [`Tx::begin`] from [`wicket_db::app_version`].
    pub fn transition_context(&self, actor: Actor, doc: &DocRef, edge: &str) -> WriteContext {
        let mut ctx = WriteContext::new(actor, "pending", "ui");
        ctx.config_version = Some(self.profile.spec_version.clone());
        wicket_statemachine::with_action(ctx, doc, edge)
    }

    /// Persist-time spawn of `doc` in `initial` (refuses until freeze — already frozen here).
    pub async fn spawn(&self, tx: &mut Tx<'_>, doc: &DocRef, initial: &str) -> Result<Instance> {
        Ok(self.engine.spawn(tx, doc, initial).await?)
    }

    /// Allocate a document master and spawn the documents machine at `Draft`.
    pub async fn create_document(
        &self,
        tx: &mut Tx<'_>,
        kind: &str,
        title: &str,
        retention_class: &str,
    ) -> Result<wicket_documents::DocumentId> {
        Ok(wicket_documents::create(tx, &self.engine, kind, title, retention_class).await?)
    }

    /// Insert a revision and publish `documents.revision_created` in the same `Tx`.
    pub async fn new_document_revision(
        &self,
        tx: &mut Tx<'_>,
        doc: wicket_documents::DocumentId,
        label: &str,
        manifest: wicket_documents::Manifest,
    ) -> Result<wicket_documents::RevisionId> {
        let id = wicket_documents::new_revision(tx, doc, label, manifest).await?;
        emit_revision_created(tx, doc, id, label).await?;
        Ok(id)
    }

    /// Gate-wrapped transition: one `GroupBuilder` bound to `tx`, hooks contribute,
    /// executor `finalize`s, then [`wicket_ledger::post`] writes the group in this `Tx`.
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
            source_kind: wicket_statemachine::action_for(&doc.doc_type, edge),
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
        wicket_ledger::bind_tx(&mut builder, tx).await?;
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
                    wicket_ledger::post(tx, watch).await?;
                }
                if doc.doc_type == wicket_documents::DOC_TYPE && edge == "make_effective" {
                    emit_effective(tx, wicket_documents::DocumentId(doc.doc_id)).await?;
                }
                Ok(instance)
            }
            Err(e) => {
                drop(watch);
                if let wicket_statemachine::Error::Signature(sig) = &e
                    && audited_refusal(sig)
                {
                    // D-2b-5: refusal audit after rollback, never while this Tx
                    // still holds `identity.principal` / `esign.signature` FOR UPDATE.
                    if abort_claim_tx(tx).await.is_ok() {
                        self.log_gate_refusal(ctx, doc, edge, sig).await;
                    }
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
        let signer_status =
            match load_principal_on(tx, UserId::from_identifier(token.signer.id)).await {
                Ok(p) => p.status,
                Err(_) => PrincipalStatus::Inactive,
            };
        let projection = self.live_record(tx, &doc.doc_type, doc.doc_id).await?;
        Ok(LiveDoc {
            record: RecordRef {
                table: "sm.instance".into(),
                id: doc.doc_id,
                version: row.1,
            },
            doc_type: doc.doc_type.clone(),
            projection,
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
        let _ = wicket_esign::log_refusal(&write, ctx.actor, &ctx.action, &sig.to_string(), detail)
            .await;
    }

    /// Bind `builder` to `tx` so Drop poisons an unfinalized contribution.
    pub async fn bind_sink(&self, tx: &mut Tx<'_>, builder: &mut GroupBuilder) -> Result<()> {
        Ok(wicket_ledger::bind_tx(builder, tx).await?)
    }

    /// Convert `entered` to stock on `tx` (a lot factor pinned earlier in `tx` is honoured).
    pub async fn to_stock<D: Dimension>(
        &self,
        tx: &mut Tx<'_>,
        catalog: &wicket_uom::UomCatalog,
        item: ItemId,
        entered: AnyQuantity,
        ctx: &ConversionContext,
    ) -> Result<wicket_uom::StockConversion<D>> {
        Ok(wicket_uom::to_stock(tx, catalog, item, entered, ctx).await?)
    }

    /// Catalog convert (same units as [`wicket_uom::convert`]).
    pub fn convert<D: Dimension>(
        &self,
        catalog: &wicket_uom::UomCatalog,
        qty: Quantity<D>,
        to: UnitRef<D>,
        ctx: &ConversionContext,
    ) -> core::result::Result<Converted<D>, wicket_core::QuantityError> {
        wicket_uom::convert(catalog, qty, to, ctx)
    }

    /// Publish `event` inside `tx` (visible to subscribers after commit).
    pub async fn publish_event(
        &self,
        tx: &mut Tx<'_>,
        event: wicket_events::Event,
    ) -> Result<Identifier> {
        Ok(wicket_events::publish(tx, event).await?)
    }

    /// One dispatcher tick as `actor` (must be a service principal).
    pub async fn dispatch_tick(&self, actor: Actor) -> Result<u32> {
        let write = self.write_pool();
        Ok(wicket_events::Dispatcher::new(self.events.clone())
            .idle(Duration::from_millis(1))
            .backoff_base(Duration::ZERO)
            .tick(&write, actor)
            .await?)
    }

    /// One worker tick as `actor` (must be a service principal).
    pub async fn worker_tick(&self, actor: Actor) -> Result<u32> {
        let write = self.write_pool();
        Ok(wicket_jobs::Worker::new(self.jobs.clone())
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
        handler: impl wicket_events::EventHandler + 'static,
    ) -> Result<()> {
        self.events.subscribe(name, subscriber, handler);
        wicket_events::enable_subscription(tx, name, subscriber).await?;
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
/// and [`wicket_ledger::post`] is the insert (it finalizes the cloned builder).
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
        Box<dyn std::future::Future<Output = wicket_jobs::Result<HandlerOutcome>> + Send + '_>,
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
    let bundles: Vec<wicket_identity::rbac::RoleBundle> = profile
        .seeded_permissions
        .bundles
        .iter()
        .map(|b| wicket_identity::rbac::RoleBundle {
            name: b.name.clone(),
            permissions: b.permissions.clone(),
        })
        .collect();
    if !bundles.is_empty() {
        wicket_identity::rbac::seed_bundles(tx, &bundles).await?;
    }
    for (doc, spec) in &profile.numbering {
        let policy = spec.reset_policy()?;
        wicket_numbering::define(tx, doc, &spec.format, policy).await?;
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

/// Abort the claim Tx without consuming the caller's `Tx` handle (D-2b-5).
///
/// `Tx::rollback` takes `self`. Gate refusal must release `FOR UPDATE` on
/// `identity.principal` before `audit.log_event` on a second pool connection
/// (`max_connections=2`).
async fn abort_claim_tx(tx: &mut Tx<'_>) -> Result<()> {
    tx.execute(sqlx::query("ROLLBACK")).await?;
    Ok(())
}

/// First-party machines may use the identity projection; extra bound machines
/// must call [`KernelBuilder::register_projection`] or [`Kernel::build`] fails.
fn first_party_projection_types() -> Result<BTreeSet<String>> {
    let mut types = BTreeSet::new();
    types.insert(wicket_documents::DOC_TYPE.to_owned());
    types.insert(wicket_customfields::DOC_TYPE.to_owned());
    for m in compiled_in()? {
        for machine in m.machines {
            types.insert(machine.doc_type);
        }
    }
    Ok(types)
}

/// Register a projection per bound machine (D-2b-3).
///
/// Explicit builder registrations win. First-party machines default to
/// [`wicket_esign::identity_projection`]. Any other bound machine without a
/// registration is [`Error::MissingProjection`].
fn register_bound_projections(
    engine: &Engine,
    extra: &BTreeMap<String, fn(&Value) -> Value>,
) -> Result<()> {
    let known = first_party_projection_types()?;
    let mut types: BTreeSet<String> = engine
        .edges_for_manifest()
        .into_iter()
        .map(|e| e.doc_type)
        .collect();
    types.extend(extra.keys().cloned());
    for doc_type in types {
        let project = extra.get(&doc_type).copied().or_else(|| {
            known
                .contains(&doc_type)
                .then_some(wicket_esign::identity_projection)
        });
        let Some(project) = project else {
            return Err(Error::MissingProjection(doc_type));
        };
        wicket_esign::register_projection(&doc_type, project);
    }
    Ok(())
}

async fn default_live_record(tx: &mut Tx<'_>, doc_type: &str, doc_id: Identifier) -> Result<Value> {
    if doc_type == wicket_documents::DOC_TYPE {
        match wicket_documents::history(tx, wicket_documents::DocumentId(doc_id)).await {
            Ok(revs) => {
                return Ok(revs
                    .last()
                    .map(|r| r.manifest.content.clone())
                    .unwrap_or_else(|| serde_json::json!({})));
            }
            Err(_) => return Ok(serde_json::json!({})),
        }
    }
    if doc_type == "lot" {
        let row: Option<(Value,)> = tx
            .fetch_optional(
                sqlx::query_as("SELECT to_jsonb(l) FROM lots.lot l WHERE id = $1")
                    .bind(doc_id.as_uuid()),
            )
            .await?;
        return Ok(row.map(|r| r.0).unwrap_or_else(|| serde_json::json!({})));
    }
    Ok(serde_json::json!({}))
}

/// Bind the `GateFactory` named by the profile TOML `gate` field.
///
/// SPEC-profiles key 4 / D-2b-4: `NoSignatures` → the singleton factory;
/// `"wicket-esign"` → the prepared-gate factory.
pub fn bind_signature_gate(binding: GateBinding) -> GateFactory {
    match binding {
        GateBinding::NoSignatures => GateFactory::no_signatures(),
        GateBinding::WicketEsign => GateFactory::wicket_esign(),
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

/// Stamp `wicket.esign_id` for remaining statements in this transaction.
///
/// D-2b-1: every audit row of the consuming transition carries the signature id.
/// The GUC write lives on [`Tx::bind_esign_id`] (CONTRACT §5a).
async fn stamp_esign_id(tx: &mut Tx<'_>, id: SignatureId) -> Result<()> {
    tx.bind_esign_id(id.as_uuid().to_string()).await?;
    Ok(())
}

fn register_enabled_from_manifests(
    engine: &mut Engine,
    profile: &Profile,
    catalog: &[ModuleManifest],
    extra_types: &BTreeSet<String>,
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
        for machine in &m.machines {
            if extra_types.contains(&machine.doc_type) {
                continue;
            }
            engine.register_machine(machine_from_decl(machine)?)?;
        }
        for r in &m.routes {
            routes.push(ModuleRoute {
                module_id: m.id.clone(),
                method: r.method.clone(),
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

/// Intern a TOML reason so [`EdgeBuilder::not_required`] can take `&'static str`.
fn intern_static(s: &str) -> &'static str {
    static POOL: OnceLock<Mutex<BTreeSet<&'static str>>> = OnceLock::new();
    let pool = POOL.get_or_init(|| Mutex::new(BTreeSet::new()));
    let mut g = match pool.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if let Some(existing) = g.get(s).copied() {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
    g.insert(leaked);
    leaked
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
        } else if let Some(reason) = e.reason.as_deref() {
            edge = edge.not_required(intern_static(reason));
        }
        b = b.edge(edge);
    }
    Ok(b.build()?)
}

fn hold_wave2b_edges() {
    let _ = core::any::type_name::<wicket_esign::Error>();
    let _ = core::any::type_name::<wicket_documents::Error>();
    let _ = core::any::type_name::<wicket_print::Error>();
}

/// Graph nodes for the compiled-in catalog (tests / hook-order).
pub fn module_nodes() -> Result<Vec<ModuleNode>> {
    compiled_in_graph()
}

#[cfg(test)]
mod machine_decl_tests {
    use super::*;
    use wicket_statemachine::SignatureDeclaration;

    #[test]
    fn not_required_reason_survives_machine_from_decl() {
        let m = ModuleManifest::parse(crate::install_graph::ITEMS_MANIFEST).expect("items");
        let machine = machine_from_decl(&m.machines[0]).expect("machine");
        assert_eq!(machine.edges.len(), 2);
        for e in &machine.edges {
            match &e.signature {
                SignatureDeclaration::NotRequired { reason } => {
                    assert_eq!(
                        *reason,
                        "item release is not a regulated signature point in v1"
                    );
                }
                other => panic!("expected NotRequired, got {other:?}"),
            }
        }
    }

    #[test]
    fn required_meaning_survives_machine_from_decl() {
        let m = ModuleManifest::parse(crate::install_graph::CALIBRATION_MANIFEST).expect("cal");
        let machine = machine_from_decl(&m.machines[0]).expect("machine");
        match &machine.edges[0].signature {
            SignatureDeclaration::Required(req) => {
                assert_eq!(req.meaning.0, "Approved");
                assert_eq!(req.permission.0, "calibration.approve");
            }
            other => panic!("expected Required, got {other:?}"),
        }
    }
}
