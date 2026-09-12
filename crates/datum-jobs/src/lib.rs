//! Durable background jobs: enqueue in the caller transaction, workers as service principals.

pub mod error;
pub mod events;
pub mod handler;
pub mod progress;
pub mod queue;
mod sql;
pub mod worker;

pub use error::{Error, Result};
pub use handler::{HandlerOutcome, JobHandler, Registry};
pub use progress::Progress;
pub use queue::{
    DEFAULT_MAX_ATTEMPTS, EnqueueOptions, JobState, JobStatus, cancel, enqueue, progress, status,
};
pub use worker::{Worker, register_maintenance};

use datum_core::{Actor, Identifier};

/// Embedded migrator (`placeholder` + `0001_jobs`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Job id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct JobId(pub Identifier);

impl JobId {
    /// Underlying identifier.
    pub fn id(self) -> Identifier {
        self.0
    }
}

/// Named service principal for background work ([`Actor`] with [`ActorKind::ServicePrincipal`]).
pub type ServicePrincipal = Actor;

#[cfg(test)]
mod tests {
    use super::*;
    use datum_audit as _;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_jobs_migration() {
        assert!(MIGRATOR.migrations.len() >= 2);
        assert!(MIGRATOR.iter().any(|m| m.version == 1));
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
