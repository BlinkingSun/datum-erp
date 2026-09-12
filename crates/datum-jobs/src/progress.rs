//! Progress reporting from compute jobs (short transactions, never held across CPU work).

use datum_core::{Actor, ActorKind};
use datum_db::{Tx, WriteContext, WritePool};

use crate::JobId;
use crate::error::{Error, Result};
use crate::queue::progress;

/// Reports `progress_pct` / `progress_note` in its own `Tx::begin` transactions.
#[derive(Clone)]
pub struct Progress {
    pool: WritePool,
    job_id: JobId,
    service_actor: Actor,
}

impl Progress {
    pub(crate) fn new(pool: WritePool, job_id: JobId, service_actor: Actor) -> Self {
        Self {
            pool,
            job_id,
            service_actor,
        }
    }

    fn job_ctx(action: &str, actor: Actor) -> WriteContext {
        let mut ctx = WriteContext::new(actor, action, "job");
        if ctx.actor_display.is_none() {
            ctx.actor_display = Some(format!("service:{}", actor.id));
        }
        ctx.reason = Some("background job progress".into());
        ctx
    }

    /// Persist progress for the job (opens and commits its own transaction).
    pub async fn report(&self, pct: i16, note: impl Into<String>) -> Result<()> {
        if self.service_actor.kind != ActorKind::ServicePrincipal {
            return Err(Error::NotServicePrincipal);
        }
        let ctx = Self::job_ctx("job.progress", self.service_actor);
        let mut tx = Tx::begin(&self.pool, &ctx).await?;
        progress(&mut tx, self.job_id, pct, note).await?;
        tx.commit().await?;
        Ok(())
    }
}
