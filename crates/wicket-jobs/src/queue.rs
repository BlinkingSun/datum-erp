//! Enqueue, cancel, progress, and read-only status.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use wicket_core::Identifier;
use wicket_db::{Pool, Tx};

use crate::JobId;
use crate::error::{Error, Result};
use crate::sql::{query, query_as};

/// Default attempts before a job is marked failed.
pub const DEFAULT_MAX_ATTEMPTS: i32 = 25;

/// Options when enqueueing a job inside the caller's transaction.
#[derive(Debug, Clone)]
pub struct EnqueueOptions {
    /// Do not claim before this instant (defaults to now).
    pub run_after: Option<DateTime<Utc>>,
    /// Retry budget (defaults to [`DEFAULT_MAX_ATTEMPTS`]).
    pub max_attempts: i32,
}

impl Default for EnqueueOptions {
    fn default() -> Self {
        Self {
            run_after: None,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
        }
    }
}

/// Snapshot of a job row for readers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobStatus {
    /// Job id.
    pub id: JobId,
    /// Kind string.
    pub kind: String,
    /// Payload.
    pub payload: Value,
    /// Queue state.
    pub state: JobState,
    /// Attempt count so far.
    pub attempts: i32,
    /// Maximum attempts.
    pub max_attempts: i32,
    /// Scheduled run time.
    pub run_after: DateTime<Utc>,
    /// Progress percent 0–100.
    pub progress_pct: i16,
    /// Human-readable progress note.
    pub progress_note: Option<String>,
    /// Result JSON when succeeded.
    pub result: Option<Value>,
    /// Last error when failed.
    pub last_error: Option<String>,
}

/// Job queue state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum JobState {
    /// Waiting for a worker.
    Queued,
    /// Claimed and executing.
    Running,
    /// Finished successfully.
    Succeeded,
    /// Exhausted retries or fatal handler error.
    Failed,
    /// Cancelled before run.
    Cancelled,
}

impl JobState {
    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            other => return Err(Error::Invariant(format!("unknown job state {other}"))),
        })
    }
}

#[derive(sqlx::FromRow)]
struct StatusRow {
    id: Uuid,
    kind: String,
    payload: Value,
    state: String,
    attempts: i32,
    max_attempts: i32,
    run_after: DateTime<Utc>,
    progress_pct: i16,
    progress_note: Option<String>,
    result: Option<Value>,
    last_error: Option<String>,
}

fn row_to_status(row: StatusRow) -> Result<JobStatus> {
    Ok(JobStatus {
        id: JobId(Identifier::from_uuid(row.id)),
        kind: row.kind,
        payload: row.payload,
        state: JobState::parse(&row.state)?,
        attempts: row.attempts,
        max_attempts: row.max_attempts,
        run_after: row.run_after,
        progress_pct: row.progress_pct,
        progress_note: row.progress_note,
        result: row.result,
        last_error: row.last_error,
    })
}

/// Enqueue a job inside `tx`. The row is visible only after the caller commits.
pub async fn enqueue(
    tx: &mut Tx<'_>,
    kind: impl Into<String>,
    payload: Value,
    requested_by: Identifier,
    options: EnqueueOptions,
) -> Result<JobId> {
    let kind = kind.into();
    if kind.is_empty() {
        return Err(Error::Invariant("job kind must not be empty".into()));
    }
    let id = Identifier::generate();
    let run_after = options.run_after.unwrap_or_else(Utc::now);
    let max_attempts = options.max_attempts.max(1);
    tx.execute(
        query(
            r#"
            INSERT INTO transient.job (
                id, kind, payload, state, max_attempts, run_after, requested_by
            ) VALUES ($1, $2, $3, 'queued', $4, $5, $6)
            "#,
        )
        .bind(id.as_uuid())
        .bind(&kind)
        .bind(&payload)
        .bind(max_attempts)
        .bind(run_after)
        .bind(requested_by.as_uuid()),
    )
    .await?;
    Ok(JobId(id))
}

/// Cancel a queued job inside `tx`. A running job is not stopped.
pub async fn cancel(tx: &mut Tx<'_>, id: JobId) -> Result<()> {
    let n = tx
        .execute(
            query(
                r#"
                UPDATE transient.job
                   SET state = 'cancelled'
                 WHERE id = $1 AND state = 'queued'
                "#,
            )
            .bind(id.0.as_uuid()),
        )
        .await?
        .rows_affected();
    if n == 0 {
        let exists: (bool,) = tx
            .fetch_one(
                query_as("SELECT EXISTS (SELECT 1 FROM transient.job WHERE id = $1)")
                    .bind(id.0.as_uuid()),
            )
            .await?;
        let exists = exists.0;
        if !exists {
            return Err(Error::NotFound(id.0));
        }
        return Err(Error::Invariant("only queued jobs can be cancelled".into()));
    }
    Ok(())
}

/// Update visible progress inside `tx`.
pub async fn progress(tx: &mut Tx<'_>, id: JobId, pct: i16, note: impl Into<String>) -> Result<()> {
    let pct = pct.clamp(0, 100);
    let n = tx
        .execute(
            query(
                r#"
                UPDATE transient.job
                   SET progress_pct = $2,
                       progress_note = $3,
                       locked_at = CASE WHEN state = 'running' THEN now() ELSE locked_at END
                 WHERE id = $1
                "#,
            )
            .bind(id.0.as_uuid())
            .bind(pct)
            .bind(note.into()),
        )
        .await?
        .rows_affected();
    if n == 0 {
        return Err(Error::NotFound(id.0));
    }
    Ok(())
}

/// Read job status through a read pool (no `Tx::begin` required).
pub async fn status(pool: &Pool, id: JobId) -> Result<Option<JobStatus>> {
    let row: Option<StatusRow> = query_as(
        r#"
        SELECT id, kind, payload, state, attempts, max_attempts, run_after,
               progress_pct, progress_note, result, last_error
          FROM transient.job
         WHERE id = $1
        "#,
    )
    .bind(id.0.as_uuid())
    .fetch_optional(pool)
    .await?;
    row.map(row_to_status).transpose()
}
