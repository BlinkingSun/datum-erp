//! Crate error type.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from manifests, profiles, lifecycle, and kernel boot.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
    /// Audit error.
    #[error(transparent)]
    Audit(#[from] wicket_audit::Error),
    /// Identity error.
    #[error(transparent)]
    Identity(#[from] wicket_identity::Error),
    /// Numbering error.
    #[error(transparent)]
    Numbering(#[from] wicket_numbering::Error),
    /// Events error.
    #[error(transparent)]
    Events(#[from] wicket_events::Error),
    /// Jobs error.
    #[error(transparent)]
    Jobs(#[from] wicket_jobs::Error),
    /// Ledger error.
    #[error(transparent)]
    Ledger(#[from] wicket_ledger::Error),
    /// State-machine error.
    #[error(transparent)]
    Statemachine(#[from] wicket_statemachine::Error),
    /// Electronic signature error.
    #[error(transparent)]
    Esign(#[from] wicket_esign::Error),
    /// Custom-fields error.
    #[error(transparent)]
    Customfields(#[from] wicket_customfields::Error),
    /// Controlled documents error.
    #[error(transparent)]
    Documents(#[from] wicket_documents::Error),
    /// Print / template-seed error.
    #[error(transparent)]
    Print(#[from] wicket_print::Error),
    /// JSON error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Module or profile TOML could not be parsed.
    #[error("toml: {0}")]
    Toml(String),
    /// Manifest failed validation.
    #[error("manifest: {0}")]
    Manifest(String),
    /// Profile failed validation.
    #[error("profile: {0}")]
    Profile(String),
    /// Semver range is unsatisfied or malformed.
    #[error("semver: {0}")]
    Semver(String),
    /// Disabling a module that others depend on.
    #[error("cannot disable {id}: depended on by {dependents}")]
    DisableRefused {
        /// Module that was requested to disable.
        id: String,
        /// Dependents that are still enabled, named.
        dependents: String,
    },
    /// Enabling a module the active profile disallows.
    #[error("cannot enable {id}: {reason}")]
    EnableRefused {
        /// Module that was requested to enable.
        id: String,
        /// Why the profile refuses this enablement.
        reason: String,
    },
    /// Uninstall is not offered in regulated mode (`docs/03` §6).
    #[error("uninstall is not offered in regulated mode")]
    UninstallForbidden,
    /// Module is not in the registry.
    #[error("unknown module {0}")]
    UnknownModule(String),
    /// Configuration manifest does not match the live system.
    #[error("configuration manifest mismatch")]
    ManifestMismatch,
    /// Dependency cycle in the module graph.
    #[error("module dependency cycle")]
    DependencyCycle,
    /// Release startup guard (Required edge + `NoSignatures`).
    #[error("startup: {0}")]
    Startup(String),
    /// Bound machine has no esign projection (D-2b-3).
    #[error("no esign projection registered for {0}")]
    MissingProjection(String),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}

impl From<wicket_uom::Error> for Error {
    fn from(err: wicket_uom::Error) -> Self {
        match err {
            wicket_uom::Error::Db(e) => Error::Db(e),
            other => Error::Manifest(other.to_string()),
        }
    }
}
