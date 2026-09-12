# datum-jobs

Durable background jobs: enqueue in the caller transaction; workers run as
service principals. Queue rows are transient working state; `app.run_log` is
the audited history.

## Public API (`src/lib.rs`)

- `Error` / `Result`
- `HandlerOutcome` / `JobHandler` / `Registry` — handler map (`kind` → handler)
- `Progress` — percent + note reported from compute work
- `DEFAULT_MAX_ATTEMPTS` / `EnqueueOptions` / `JobState` / `JobStatus`
- `enqueue` / `cancel` / `progress` / `status` — queue operations (`enqueue` in `tx`)
- `Worker` / `register_maintenance` — claim/run loop; maintenance registrar (currently a no-op)
- `JobId` — job identifier
- `ServicePrincipal` — `Actor` with `ActorKind::ServicePrincipal`
- `MIGRATOR` — `placeholder` + `0001_jobs`
- Modules: `error`, `events`, `handler`, `progress`, `queue`, `worker`
- `events::GenealogyRefreshBridge` — subscriber that enqueues `genealogy.refresh` on `inventory.lot_received`
- `events::enable_genealogy_bridge` — register + persist the subscription
- `events::bridge_subscriber` / `events::bridge_job_kind` — ids for tests

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_jobs` — `transient.job` (transient), `transient.schedule` (transient),
  `app.run_log` (app)

## Tests (`tests/`)

- `job_invisible_until_enqueuing_tx_commits`
- `worker_runs_as_service_principal_with_job_source_kind`
- `retry_then_failed_after_max_attempts` / `crashed_worker_job_is_reclaimed`
- `skip_locked_two_workers_no_double_run`
- `compute_job_uses_multiple_threads_and_reports_progress`
- `cancel_before_start_never_runs`
- `run_log_is_append_only_and_audited`
- `event_bridge_enqueues_job` / `writes_go_through_tx`
- `jobs_tables_match_schema_classes` / `migration_is_reversible`

## Frozen / seams

Frozen: `enqueue` inside the caller's `Tx`; worker `source_kind = "job"`
(CONTRACT §4). `register_maintenance` is a no-op. `enable_genealogy_bridge`
exists as a library bridge; `Kernel::build` does not enable it (FINDINGS-0
#1/#2). SM does not depend on this crate.
