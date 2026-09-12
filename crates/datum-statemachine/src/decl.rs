//! Machine, edge, and signature declarations (D-W1-4 (c)).

use datum_core::{Identifier, PermissionKey, SignatureRequirement};

use crate::{Error, Result};

/// Opaque machine identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MachineId(pub Identifier);

impl MachineId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(Identifier::generate())
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0.as_uuid()
    }
}

/// State name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct State(pub String);

/// Applied (or intended) transition view. Wave-1 stub name, kept.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Transition {
    /// Machine.
    pub machine: MachineId,
    /// From state.
    pub from: State,
    /// To state.
    pub to: State,
}

/// Document the executor is about to advance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocRef {
    /// Document type (`wo`, `calibration.certificate`, …).
    pub doc_type: String,
    /// Document id.
    pub doc_id: Identifier,
}

/// Live instance row (`sm.instance`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// Document type.
    pub doc_type: String,
    /// Document id.
    pub doc_id: Identifier,
    /// Machine this instance is running.
    pub machine_id: MachineId,
    /// Current state. Only [`crate::Engine::transition`] mutates this column.
    pub state: State,
    /// Optimistic-concurrency version, starting at 1.
    pub version: i64,
    /// When the current state was entered (server `now()`).
    pub entered_at: chrono::DateTime<chrono::Utc>,
}

/// Total signature declaration on an edge. No [`Default`]: omission is unrepresentable
/// on a finished [`Edge`], and a `regulated = true` builder refuses an unfinished one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureDeclaration {
    /// Executor must `verify` before mutate.
    Required(SignatureRequirement),
    /// Explicitly not required, with a reason the configuration manifest prints.
    NotRequired {
        /// Why this edge does not demand a signature.
        reason: &'static str,
    },
}

/// One permitted transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// Source state.
    pub from: State,
    /// Target state.
    pub to: State,
    /// Edge name (`release`, `approve`, …).
    pub name: String,
    /// RBAC key checked by the executor via `datum_identity::rbac::has_permission`.
    pub permission: PermissionKey,
    /// Total signature declaration.
    pub signature: SignatureDeclaration,
    /// When false, hook registration for this edge is refused and hooks do not run.
    pub hooks_allowed: bool,
}

/// Declared machine: states, edges, and whether the owning module is regulated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    /// Machine id.
    pub id: MachineId,
    /// Document type this machine advances.
    pub doc_type: String,
    /// Declared states.
    pub states: Vec<State>,
    /// Declared edges. For a regulated machine every edge has an explicit declaration.
    pub edges: Vec<Edge>,
    /// Owning module's `regulated = true` flag.
    pub regulated: bool,
}

/// Builder for [`Edge`]. The signature may be omitted until [`EdgeBuilder::finish`];
/// a regulated machine refuses that omission at registration.
#[derive(Debug, Clone)]
pub struct EdgeBuilder {
    from: String,
    to: String,
    name: String,
    permission: String,
    signature: Option<SignatureDeclaration>,
    hooks_allowed: bool,
}

impl EdgeBuilder {
    /// Start an edge. `permission` is the RBAC key.
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        name: impl Into<String>,
        permission: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            name: name.into(),
            permission: permission.into(),
            signature: None,
            hooks_allowed: true,
        }
    }

    /// Mark the edge `Required`.
    pub fn required(mut self, requirement: SignatureRequirement) -> Self {
        self.signature = Some(SignatureDeclaration::Required(requirement));
        self
    }

    /// Mark the edge `NotRequired` with a manifest reason.
    pub fn not_required(mut self, reason: &'static str) -> Self {
        self.signature = Some(SignatureDeclaration::NotRequired { reason });
        self
    }

    /// Whether hooks may register against this edge.
    pub fn hooks_allowed(mut self, allowed: bool) -> Self {
        self.hooks_allowed = allowed;
        self
    }

    /// Finish the edge. `regulated` machines require an explicit declaration.
    pub fn finish(self, regulated: bool, machine_label: &str) -> Result<Edge> {
        let signature = match self.signature {
            Some(s) => s,
            None if regulated => {
                return Err(Error::MissingSignatureDeclaration {
                    machine: machine_label.to_owned(),
                    edge: self.name,
                });
            }
            None => SignatureDeclaration::NotRequired {
                reason: "non-regulated module; absence means none",
            },
        };
        Ok(Edge {
            from: State(self.from),
            to: State(self.to),
            name: self.name,
            permission: PermissionKey(self.permission),
            signature,
            hooks_allowed: self.hooks_allowed,
        })
    }
}

/// Builder for [`Machine`].
#[derive(Debug, Clone)]
pub struct MachineBuilder {
    id: MachineId,
    doc_type: String,
    states: Vec<String>,
    edges: Vec<EdgeBuilder>,
    regulated: bool,
}

impl Machine {
    /// Start a machine for `doc_type`.
    pub fn builder(doc_type: impl Into<String>) -> MachineBuilder {
        MachineBuilder {
            id: MachineId::generate(),
            doc_type: doc_type.into(),
            states: Vec::new(),
            edges: Vec::new(),
            regulated: false,
        }
    }
}

impl MachineBuilder {
    /// Pin the machine id (tests and composition-root mirrors).
    pub fn id(mut self, id: MachineId) -> Self {
        self.id = id;
        self
    }

    /// Mark the owning module `regulated = true` (total declaration required).
    pub fn regulated(mut self, regulated: bool) -> Self {
        self.regulated = regulated;
        self
    }

    /// Declare a state.
    pub fn state(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        if !self.states.iter().any(|s| s == &name) {
            self.states.push(name);
        }
        self
    }

    /// Declare an edge (unfinished until [`MachineBuilder::build`]).
    pub fn edge(mut self, edge: EdgeBuilder) -> Self {
        if !self.states.iter().any(|s| s == &edge.from) {
            self.states.push(edge.from.clone());
        }
        if !self.states.iter().any(|s| s == &edge.to) {
            self.states.push(edge.to.clone());
        }
        self.edges.push(edge);
        self
    }

    /// Finish. A regulated machine with any edge lacking a declaration is an error.
    pub fn build(self) -> Result<Machine> {
        let label = self.doc_type.clone();
        let mut edges = Vec::with_capacity(self.edges.len());
        for e in self.edges {
            edges.push(e.finish(self.regulated, &label)?);
        }
        Ok(Machine {
            id: self.id,
            doc_type: self.doc_type,
            states: self.states.into_iter().map(State).collect(),
            edges,
            regulated: self.regulated,
        })
    }
}

/// One edge as listed by [`crate::Engine::edges_for_manifest`] (`docs/03` §8).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ManifestEdge {
    /// Document type.
    pub doc_type: String,
    /// Edge name.
    pub edge: String,
    /// From state.
    pub from: String,
    /// To state.
    pub to: String,
    /// Permission key.
    pub permission: String,
    /// `required` or `not_required`.
    pub kind: String,
    /// Meaning when `Required`.
    pub meaning: Option<String>,
    /// Permission snapshot key when `Required`.
    pub signature_permission: Option<String>,
    /// Reason when `NotRequired`.
    pub reason: Option<String>,
}

impl ManifestEdge {
    pub(crate) fn from_edge(doc_type: &str, edge: &Edge) -> Self {
        match &edge.signature {
            SignatureDeclaration::Required(req) => Self {
                doc_type: doc_type.to_owned(),
                edge: edge.name.clone(),
                from: edge.from.0.clone(),
                to: edge.to.0.clone(),
                permission: edge.permission.0.clone(),
                kind: "required".into(),
                meaning: Some(req.meaning.0.clone()),
                signature_permission: Some(req.permission.0.clone()),
                reason: None,
            },
            SignatureDeclaration::NotRequired { reason } => Self {
                doc_type: doc_type.to_owned(),
                edge: edge.name.clone(),
                from: edge.from.0.clone(),
                to: edge.to.0.clone(),
                permission: edge.permission.0.clone(),
                kind: "not_required".into(),
                meaning: None,
                signature_permission: None,
                reason: Some((*reason).to_owned()),
            },
        }
    }
}

/// `"<doc_type>.<edge>"` — the `WriteContext.action` the executor requires.
pub fn action_for(doc_type: &str, edge: &str) -> String {
    format!("{doc_type}.{edge}")
}

/// Set `action` (and `doc_type` / `doc_id`) on a context the caller then passes to
/// [`datum_db::Tx::begin`]. `datum-db` does not expose `WriteContext::with_action`
/// or a session-GUC rebind; the executor verifies the bound action via `Tx::setting`.
pub fn with_action(
    mut ctx: datum_db::WriteContext,
    doc: &DocRef,
    edge: &str,
) -> datum_db::WriteContext {
    ctx.action = action_for(&doc.doc_type, edge);
    ctx.doc_type = Some(doc.doc_type.clone());
    ctx.doc_id = Some(doc.doc_id.to_string());
    ctx
}
