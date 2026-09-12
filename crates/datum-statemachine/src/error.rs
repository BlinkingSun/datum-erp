//! Crate error type.

use datum_core::SignatureError;

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from declaration, registration, hooks, and the transition executor.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] datum_db::Error),
    /// Identity / RBAC error.
    #[error(transparent)]
    Identity(#[from] datum_identity::Error),
    /// A `regulated = true` machine registered an edge with no signature declaration.
    #[error("regulated machine {machine} edge {edge} is missing a signature declaration")]
    MissingSignatureDeclaration {
        /// Machine id or doc_type.
        machine: String,
        /// Edge name.
        edge: String,
    },
    /// Release build with `NoSignatures` bound and a `Required` edge in the enabled set.
    #[error("startup: Required edge {doc_type}.{edge} meets NoSignatures in release")]
    StartupGate {
        /// Document type.
        doc_type: String,
        /// Edge name.
        edge: String,
    },
    /// Actor does not hold the edge permission.
    #[error("permission denied: {permission}")]
    PermissionDenied {
        /// Permission key that was required.
        permission: String,
    },
    /// Signature gate refused the token, or a `Required` edge was missing one.
    #[error(transparent)]
    Signature(#[from] SignatureError),
    /// A before-transition hook vetoed.
    #[error("hook veto by {module}: {reason}")]
    Veto {
        /// Registering module id.
        module: String,
        /// Structured reason.
        reason: String,
    },
    /// A hook exceeded its registered time budget.
    #[error("hook budget exceeded: {module} budget_ms={budget_ms}")]
    HookBudgetExceeded {
        /// Registering module id.
        module: String,
        /// Budget in milliseconds.
        budget_ms: u64,
    },
    /// An after-transition hook attempted to veto.
    #[error("after hook {module} cannot veto")]
    AfterHookCannotVeto {
        /// Registering module id.
        module: String,
    },
    /// No machine is registered for this document type.
    #[error("unknown machine for doc_type {0}")]
    UnknownMachine(String),
    /// No such edge on the machine.
    #[error("unknown edge {edge} on {doc_type}")]
    UnknownEdge {
        /// Document type.
        doc_type: String,
        /// Edge name.
        edge: String,
    },
    /// Instance is not in the edge's `from` state.
    #[error("instance in state {actual}, edge requires {expected}")]
    InvalidState {
        /// Edge `from`.
        expected: String,
        /// Live instance state.
        actual: String,
    },
    /// No `sm.instance` row for this document.
    #[error("instance not found: {doc_type}/{doc_id}")]
    InstanceNotFound {
        /// Document type.
        doc_type: String,
        /// Document id.
        doc_id: String,
    },
    /// `WriteContext.action` / bound GUC is not `"<doc_type>.<edge>"`.
    #[error("action mismatch: expected {expected}, bound {actual}")]
    ActionMismatch {
        /// Expected `"<doc_type>.<edge>"`.
        expected: String,
        /// Value bound on the transaction.
        actual: String,
    },
    /// Posting sink rejected a contribution or finalize.
    #[error(transparent)]
    Posting(#[from] datum_core::PostingError),
    /// Hook or machine registered after the engine was frozen.
    #[error("engine is frozen")]
    Frozen,
    /// `persist` / `spawn` / `transition` called before [`crate::Engine::freeze`].
    /// Hook order is computed once at startup (SPEC deliverable 4).
    #[error("engine is not frozen; hook order is computed once at startup")]
    NotFrozen,
    /// Duplicate machine, edge, or instance.
    #[error("duplicate: {0}")]
    Duplicate(String),
    /// Hook named a module that is not in the dependency graph.
    #[error("unknown module {0}")]
    UnknownModule(String),
    /// Module dependency graph contains a cycle.
    #[error("module dependency cycle")]
    DependencyCycle,
    /// Hooks are not allowed on this edge.
    #[error("hooks are not allowed on {doc_type}.{edge}")]
    HooksNotAllowed {
        /// Document type.
        doc_type: String,
        /// Edge name.
        edge: String,
    },
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}
