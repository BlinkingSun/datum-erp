//! Background dispatcher: claim, invoke, record, retry, dead-letter.

use std::time::Duration;

use chrono::{DateTime, Utc};
use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{Tx, WriteContext, WritePool};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::event::{Event, EventKind};
use crate::subscribe::{Registry, enable_subscription};

use crate::sql::{query, query_as};

/// Default attempts before a delivery row is left as a dead letter (`delivered_at` NULL).
pub const DEFAULT_MAX_ATTEMPTS: i32 = 8;

/// Default pause when a tick finds no due work.
pub const DEFAULT_IDLE: Duration = Duration::from_millis(250);

/// Default exponential backoff base (`base * 2^attempts` seconds, capped).
pub const DEFAULT_BACKOFF_BASE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, sqlx::FromRow)]
struct ClaimRow {
    event_id: Uuid,
    subscriber: String,
    attempts: i32,
    name: String,
    version: i16,
    payload: Value,
    actor_id: Uuid,
    source_kind: String,
    doc_type: Option<String>,
    doc_id: Option<Uuid>,
    occurred_at: DateTime<Utc>,
}

/// Claims undelivered rows with `FOR UPDATE SKIP LOCKED` and invokes handlers.
///
/// `Dispatcher::run` is the background loop. Each handler runs in its own
/// [`Tx::begin`] as `service_actor` with `WriteContext.source_kind = "job"`.
///
/// Delivery is at-least-once. Handlers must be idempotent: a crash after side
/// effects and before the delivery row is committed re-delivers the event.
#[derive(Clone, Debug)]
pub struct Dispatcher {
    registry: Registry,
    max_attempts: i32,
    backoff_base: Duration,
    idle: Duration,
    batch_size: u32,
}

impl Dispatcher {
    /// Dispatcher over `registry`. Defaults: 8 attempts, 1s backoff base, batch 16.
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            backoff_base: DEFAULT_BACKOFF_BASE,
            idle: DEFAULT_IDLE,
            batch_size: 16,
        }
    }

    /// Attempts before dead-letter. Must be >= 1.
    pub fn max_attempts(mut self, n: i32) -> Self {
        self.max_attempts = n.max(1);
        self
    }

    /// Base of the exponential backoff. `Duration::ZERO` retries immediately (tests).
    pub fn backoff_base(mut self, d: Duration) -> Self {
        self.backoff_base = d;
        self
    }

    /// Sleep when a tick finds nothing.
    pub fn idle(mut self, d: Duration) -> Self {
        self.idle = d;
        self
    }

    /// In-process registry this dispatcher reads.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Background loop. Returns only on error.
    pub async fn run(&self, pool: &WritePool, service_actor: Actor) -> Result<()> {
        loop {
            let n = self.tick(pool, service_actor).await?;
            if n == 0 {
                tokio::time::sleep(self.idle).await;
            }
        }
    }

    /// Claim and process up to `batch_size` due rows. Returns how many were attempted.
    pub async fn tick(&self, pool: &WritePool, service_actor: Actor) -> Result<u32> {
        if service_actor.kind != ActorKind::ServicePrincipal {
            return Err(Error::NotServicePrincipal);
        }
        let ctx = job_ctx(service_actor, "events.dispatch");
        {
            let mut tx = Tx::begin(pool, &ctx).await?;
            for (name, subscriber) in self.registry.entries() {
                enable_subscription(&mut tx, &name, &subscriber).await?;
            }
            tx.commit().await?;
        }

        let mut attempted = 0u32;
        while attempted < self.batch_size {
            match self.claim_one(pool, service_actor).await? {
                None => break,
                Some(()) => attempted += 1,
            }
        }
        Ok(attempted)
    }

    async fn claim_one(&self, pool: &WritePool, service_actor: Actor) -> Result<Option<()>> {
        let ctx = job_ctx(service_actor, "events.dispatch");
        let mut claim = Tx::begin(pool, &ctx).await?;
        seed_delivery_rows(&mut claim).await?;

        let row: Option<ClaimRow> = claim
            .fetch_optional(
                query_as(
                    r#"
                    SELECT
                        d.event_id,
                        d.subscriber,
                        d.attempts,
                        e.name,
                        e.version,
                        e.payload,
                        e.actor_id,
                        e.source_kind,
                        e.doc_type,
                        e.doc_id,
                        e.occurred_at
                    FROM transient.delivery d
                    JOIN app.event e ON e.id = d.event_id
                    JOIN app.subscription s
                      ON s.subscriber = d.subscriber AND s.name = e.name AND s.enabled
                    WHERE d.delivered_at IS NULL
                      AND d.attempts < $1
                      AND d.next_attempt_at <= now()
                    ORDER BY d.next_attempt_at, d.event_id
                    FOR UPDATE OF d SKIP LOCKED
                    LIMIT 1
                    "#,
                )
                .bind(self.max_attempts),
            )
            .await?;

        let Some(row) = row else {
            claim.commit().await?;
            return Ok(None);
        };

        let Some(handler) = self.registry.handler(&row.name, &row.subscriber) else {
            record_failure(
                &mut claim,
                row.event_id,
                &row.subscriber,
                row.attempts,
                "no in-process handler",
                self.backoff_base,
                self.max_attempts,
            )
            .await?;
            claim.commit().await?;
            return Ok(Some(()));
        };

        let event = event_from_row(&row);
        let mut work = Tx::begin(pool, &ctx).await?;
        let outcome = handler.handle(&mut work, &event).await;
        match outcome {
            Ok(()) => {
                work.commit().await?;
                claim
                    .execute(
                        query(
                            r#"
                            UPDATE transient.delivery
                               SET attempts = attempts + 1,
                                   delivered_at = now(),
                                   last_error = NULL
                             WHERE event_id = $1 AND subscriber = $2
                            "#,
                        )
                        .bind(row.event_id)
                        .bind(&row.subscriber),
                    )
                    .await?;
                claim.commit().await?;
            }
            Err(err) => {
                let _ = work.rollback().await;
                record_failure(
                    &mut claim,
                    row.event_id,
                    &row.subscriber,
                    row.attempts,
                    &err.to_string(),
                    self.backoff_base,
                    self.max_attempts,
                )
                .await?;
                claim.commit().await?;
            }
        }
        Ok(Some(()))
    }
}

fn job_ctx(actor: Actor, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(actor, action, "job");
    if ctx.actor_display.is_none() {
        ctx.actor_display = Some(format!("service:{}", actor.id));
    }
    ctx
}

fn event_from_row(row: &ClaimRow) -> Event {
    Event {
        id: Identifier::from_uuid(row.event_id),
        kind: EventKind::Custom(row.name.clone()),
        name: row.name.clone(),
        version: row.version,
        payload: row.payload.clone(),
        occurred_at: row.occurred_at,
        actor_id: Identifier::from_uuid(row.actor_id),
        source_kind: row.source_kind.clone(),
        doc_type: row.doc_type.clone(),
        doc_id: row.doc_id.map(Identifier::from_uuid),
    }
}

async fn seed_delivery_rows(tx: &mut Tx<'_>) -> Result<()> {
    tx.execute(query(
        r#"
            INSERT INTO transient.delivery (event_id, subscriber, attempts, next_attempt_at)
            SELECT e.id, s.subscriber, 0, now()
              FROM app.event e
              JOIN app.subscription s ON s.name = e.name AND s.enabled
            ON CONFLICT (event_id, subscriber) DO NOTHING
            "#,
    ))
    .await?;
    Ok(())
}

async fn record_failure(
    tx: &mut Tx<'_>,
    event_id: Uuid,
    subscriber: &str,
    attempts: i32,
    last_error: &str,
    backoff_base: Duration,
    max_attempts: i32,
) -> Result<()> {
    let next = attempts.saturating_add(1);
    let secs = backoff_secs(backoff_base, next, max_attempts);
    tx.execute(
        query(
            r#"
            UPDATE transient.delivery
               SET attempts = attempts + 1,
                   last_error = $3,
                   next_attempt_at = now() + make_interval(secs => $4)
             WHERE event_id = $1 AND subscriber = $2
            "#,
        )
        .bind(event_id)
        .bind(subscriber)
        .bind(last_error)
        .bind(secs),
    )
    .await?;
    Ok(())
}

fn backoff_secs(base: Duration, attempts: i32, max_attempts: i32) -> i32 {
    if attempts >= max_attempts {
        return 86_400;
    }
    let exp = u32::try_from(attempts.clamp(0, 10)).unwrap_or(0);
    let factor = 1u64 << exp;
    let secs = base.as_secs().saturating_mul(factor);
    i32::try_from(secs.min(86_400)).unwrap_or(86_400)
}
