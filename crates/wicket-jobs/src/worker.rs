//! Background worker: claim, run handlers, retry, heartbeat.

use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;
use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::{Tx, WriteContext, WritePool};

use crate::JobId;
use crate::error::{Error, Result};
use crate::handler::{HandlerOutcome, Registry};
use crate::progress::Progress;
use crate::sql::{query, query_as};

/// Stale `locked_at` after which a running job may be reclaimed.
pub const STALE_LOCK: Duration = Duration::from_secs(30);

/// Default idle sleep when no work is available.
pub const DEFAULT_IDLE: Duration = Duration::from_millis(50);

/// Default exponential backoff base.
pub const DEFAULT_BACKOFF_BASE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, sqlx::FromRow)]
struct ClaimRow {
    id: Uuid,
    kind: String,
    payload: Value,
    attempts: i32,
    max_attempts: i32,
}

/// Claims queued jobs with `FOR UPDATE SKIP LOCKED` and runs registered handlers.
#[derive(Clone, Debug)]
pub struct Worker {
    registry: Registry,
    stale_lock: Duration,
    backoff_base: Duration,
    idle: Duration,
}

impl Worker {
    /// Worker over `registry`.
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            stale_lock: STALE_LOCK,
            backoff_base: DEFAULT_BACKOFF_BASE,
            idle: DEFAULT_IDLE,
        }
    }

    /// Override stale-lock reclaim interval (tests).
    pub fn stale_lock(mut self, d: Duration) -> Self {
        self.stale_lock = d;
        self
    }

    /// Override backoff base (`Duration::ZERO` retries immediately in tests).
    pub fn backoff_base(mut self, d: Duration) -> Self {
        self.backoff_base = d;
        self
    }

    /// Sleep when a tick finds nothing.
    pub fn idle(mut self, d: Duration) -> Self {
        self.idle = d;
        self
    }

    /// In-process handler registry.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Background loop with `concurrency` parallel claimers.
    pub async fn run(
        pool: &WritePool,
        service_actor: Actor,
        registry: Registry,
        concurrency: u32,
    ) -> Result<()> {
        let worker = Self::new(registry);
        let n = concurrency.max(1);
        let mut tasks = Vec::new();
        for _ in 0..n {
            let w = worker.clone();
            let pool = pool.clone();
            tasks.push(tokio::spawn(async move {
                w.loop_one(pool, service_actor).await
            }));
        }
        for t in tasks {
            t.await.expect("worker task")?;
        }
        Ok(())
    }

    async fn loop_one(self, pool: WritePool, service_actor: Actor) -> Result<()> {
        loop {
            let n = self.tick(&pool, service_actor).await?;
            if n == 0 {
                tokio::time::sleep(self.idle).await;
            }
        }
    }

    /// Reclaim stale locks, then claim and run up to one job. Returns 1 if work ran.
    pub async fn tick(&self, pool: &WritePool, service_actor: Actor) -> Result<u32> {
        if service_actor.kind != ActorKind::ServicePrincipal {
            return Err(Error::NotServicePrincipal);
        }
        reclaim_stale(pool, service_actor, self.stale_lock).await?;
        match self.claim_and_run(pool, service_actor).await? {
            true => Ok(1),
            false => Ok(0),
        }
    }

    async fn claim_and_run(&self, pool: &WritePool, service_actor: Actor) -> Result<bool> {
        let worker_id = format!("{}", Identifier::generate());
        let claim_ctx = job_ctx(service_actor, "jobs.claim");
        let mut claim = Tx::begin(pool, &claim_ctx).await?;

        let row: Option<ClaimRow> = claim
            .fetch_optional(query_as(
                r#"
                    SELECT id, kind, payload, attempts, max_attempts
                      FROM transient.job
                     WHERE state = 'queued'
                       AND run_after <= now()
                     ORDER BY run_after, created_at
                     FOR UPDATE SKIP LOCKED
                     LIMIT 1
                    "#,
            ))
            .await?;

        let Some(row) = row else {
            claim.commit().await?;
            return Ok(false);
        };

        let job_id = JobId(Identifier::from_uuid(row.id));
        let attempt = row.attempts + 1;
        let log_id = Identifier::generate();

        claim
            .execute(
                query(
                    r#"
                    UPDATE transient.job
                       SET state = 'running',
                           attempts = $2,
                           locked_by = $3,
                           locked_at = now(),
                           progress_pct = 0,
                           progress_note = NULL,
                           last_error = NULL
                     WHERE id = $1
                    "#,
                )
                .bind(row.id)
                .bind(attempt)
                .bind(&worker_id),
            )
            .await?;

        claim.commit().await?;

        {
            let ctx = job_ctx(service_actor, "jobs.run_start");
            let mut log_tx = Tx::begin(pool, &ctx).await?;
            log_tx
                .execute(
                    query(
                        r#"
                        INSERT INTO app.run_log (id, job_id, attempt, started_at)
                        VALUES ($1, $2, $3, now())
                        "#,
                    )
                    .bind(log_id.as_uuid())
                    .bind(row.id)
                    .bind(attempt),
                )
                .await?;
            log_tx.commit().await?;
        }

        let action = format!("job.{}", row.kind);
        let run_result: Result<Value> = if is_builtin_kind(&row.kind) {
            Ok(json!({}))
        } else {
            let handler = self
                .registry
                .get(&row.kind)
                .ok_or_else(|| Error::UnknownKind(row.kind.clone()))?;
            let progress = Progress::new(pool.clone(), job_id, service_actor);
            match handler.run(&row.payload, progress).await {
                Ok(HandlerOutcome::Done(value)) => Ok(value),
                Ok(HandlerOutcome::Compute(work)) => {
                    let pool = pool.clone();
                    let actor = service_actor;
                    tokio::task::spawn_blocking(move || {
                        let progress = Progress::new(pool, job_id, actor);
                        work(progress)
                    })
                    .await
                    .map_err(|e| Error::Invariant(format!("compute join: {e}")))?
                }
                Err(e) => Err(e),
            }
        };

        match run_result {
            Ok(value) => {
                finish_success(
                    pool,
                    service_actor,
                    &action,
                    job_id,
                    log_id,
                    &row.kind,
                    &row.payload,
                    value,
                )
                .await?;
                Ok(true)
            }
            Err(err) => {
                let msg = err.to_string();
                if attempt >= row.max_attempts {
                    finish_failed(pool, service_actor, &action, job_id, log_id, &msg).await?;
                } else {
                    requeue(
                        pool,
                        service_actor,
                        job_id,
                        log_id,
                        attempt,
                        &msg,
                        self.backoff_base,
                    )
                    .await?;
                }
                Ok(true)
            }
        }
    }
}

pub(crate) fn job_ctx(actor: Actor, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(actor, action, "job");
    if ctx.actor_display.is_none() {
        ctx.actor_display = Some(format!("service:{}", actor.id));
    }
    ctx.reason = Some("background job run".into());
    ctx
}

fn is_builtin_kind(kind: &str) -> bool {
    matches!(
        kind,
        "audit.ensure_partitions" | "audit.anchor" | "jobs.prune_finished"
    )
}

async fn reclaim_stale(pool: &WritePool, actor: Actor, stale: Duration) -> Result<()> {
    let secs = stale.as_secs().max(1) as i32;
    let ctx = job_ctx(actor, "jobs.reclaim");
    let mut tx = Tx::begin(pool, &ctx).await?;
    tx.execute(
        query(
            r#"
            UPDATE transient.job
               SET state = 'queued',
                   locked_by = NULL,
                   locked_at = NULL,
                   run_after = now()
             WHERE state = 'running'
               AND locked_at IS NOT NULL
               AND locked_at < now() - make_interval(secs => $1)
            "#,
        )
        .bind(secs),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn finish_success(
    pool: &WritePool,
    actor: Actor,
    action: &str,
    job_id: JobId,
    log_id: Identifier,
    kind: &str,
    payload: &Value,
    result: Value,
) -> Result<()> {
    let ctx = job_ctx(actor, action);
    let mut tx = Tx::begin(pool, &ctx).await?;
    let result = if is_builtin_kind(kind) {
        run_maintenance_sql(&mut tx, kind, payload).await?
    } else {
        result
    };
    tx.execute(
        query(
            r#"
            UPDATE transient.job
               SET state = 'succeeded',
                   result = $2,
                   progress_pct = 100,
                   locked_by = NULL,
                   locked_at = NULL
             WHERE id = $1
            "#,
        )
        .bind(job_id.0.as_uuid())
        .bind(&result),
    )
    .await?;
    tx.execute(
        query(
            r#"
            UPDATE app.run_log
               SET finished_at = now(),
                   outcome = 'succeeded'
             WHERE id = $1
            "#,
        )
        .bind(log_id.as_uuid()),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn finish_failed(
    pool: &WritePool,
    actor: Actor,
    action: &str,
    job_id: JobId,
    log_id: Identifier,
    last_error: &str,
) -> Result<()> {
    let ctx = job_ctx(actor, action);
    let mut tx = Tx::begin(pool, &ctx).await?;
    tx.execute(
        query(
            r#"
            UPDATE transient.job
               SET state = 'failed',
                   last_error = $2,
                   locked_by = NULL,
                   locked_at = NULL
             WHERE id = $1
            "#,
        )
        .bind(job_id.0.as_uuid())
        .bind(last_error),
    )
    .await?;
    tx.execute(
        query(
            r#"
            UPDATE app.run_log
               SET finished_at = now(),
                   outcome = 'failed',
                   error = $2
             WHERE id = $1
            "#,
        )
        .bind(log_id.as_uuid())
        .bind(last_error),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn requeue(
    pool: &WritePool,
    actor: Actor,
    job_id: JobId,
    log_id: Identifier,
    attempt: i32,
    last_error: &str,
    backoff_base: Duration,
) -> Result<()> {
    let secs = backoff_secs(backoff_base, attempt);
    let ctx = job_ctx(actor, "jobs.retry");
    let mut tx = Tx::begin(pool, &ctx).await?;
    tx.execute(
        query(
            r#"
            UPDATE transient.job
               SET state = 'queued',
                   last_error = $2,
                   locked_by = NULL,
                   locked_at = NULL,
                   run_after = now() + make_interval(secs => $3)
             WHERE id = $1
            "#,
        )
        .bind(job_id.0.as_uuid())
        .bind(last_error)
        .bind(secs),
    )
    .await?;
    tx.execute(
        query(
            r#"
            UPDATE app.run_log
               SET finished_at = now(),
                   outcome = 'failed',
                   error = $2
             WHERE id = $1
            "#,
        )
        .bind(log_id.as_uuid())
        .bind(last_error),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

fn backoff_secs(base: Duration, attempt: i32) -> i32 {
    if base.is_zero() {
        return 0;
    }
    let shift = attempt.saturating_sub(1).min(16);
    let secs = base.as_secs().max(1) << shift;
    secs.min(3600) as i32
}

async fn run_maintenance_sql(tx: &mut Tx<'_>, kind: &str, payload: &Value) -> Result<Value> {
    match kind {
        "audit.ensure_partitions" => {
            let months = payload.get("months").and_then(|v| v.as_i64()).unwrap_or(3) as i32;
            tx.execute(query("SELECT audit.ensure_partitions($1)").bind(months))
                .await?;
            Ok(json!({ "months": months }))
        }
        "audit.anchor" => {
            let seq = payload.get("seq").and_then(|v| v.as_i64()).unwrap_or(1);
            let hash_hex = payload.get("hash").and_then(|v| v.as_str()).unwrap_or("00");
            let sink = payload
                .get("sink")
                .and_then(|v| v.as_str())
                .unwrap_or("test");
            let receipt = payload.get("receipt").and_then(|v| v.as_str());
            tx.execute(
                query("SELECT audit.record_anchor($1, decode($2, 'hex'), $3, $4)")
                    .bind(seq)
                    .bind(hash_hex)
                    .bind(sink)
                    .bind(receipt),
            )
            .await?;
            Ok(json!({ "seq": seq, "sink": sink }))
        }
        "jobs.prune_finished" => {
            let days = payload
                .get("older_than_days")
                .and_then(|v| v.as_i64())
                .unwrap_or(30) as i32;
            let deleted: (i64,) = tx
                .fetch_one(query_as("SELECT transient.prune_finished($1)").bind(days))
                .await?;
            Ok(json!({ "deleted": deleted.0, "older_than_days": days }))
        }
        other => Err(Error::UnknownKind(other.to_string())),
    }
}

/// Register built-in maintenance kinds (handled without custom handlers).
pub fn register_maintenance(_registry: &Registry) {}
