# wicket-db

Kernel persistence: write/read pools, the sealed `Tx` that binds every `wicket.*`
setting, the multi-crate migration runner, and catalogue DDL lints. Raw SQL is
legal here (D3 §11). Event-trigger attach is `wicket-audit`.

## Public API (`src/lib.rs`)

- `Pool` — `sqlx::PgPool` alias
- `MIGRATOR` — embedded `placeholder` + `0001_wicket_schema`
- `WICKET_SETTINGS` — the 18 `wicket.*` names `Tx::begin` binds (D3 §2.1 + inv. 17)
- `WritePool` — write pool; no `Deref` / `as_pool` / `into_inner`
- `ReadPool` — read pool; never `set_config`; `fetch_one` / `fetch_optional` /
  `fetch_all` (no `execute`); `as_pool` for kernel read APIs still taking `Pool`
- `WriteContext` — actor and provenance built here, not from module strings
- `SessionCtx` — `actor()` for anything the persistence layer can read
- `app_version` — crate version plus optional `WICKET_GIT_DESCRIBE`
- `connect` / `connect_with` — pool with D3 §2.2 `after_connect` / `after_release`
- `apply_ddl` — stub; returns `Unimplemented` (audit owns attach)
- `attach_audit_trigger` — stub; returns `Unimplemented` (`wicket-audit::attach`)
- `runtime_kind` — `"tokio"`
- `Error` / `Result` / `SqlState` — `Refused(42501)`, `Serialization(40001)`, `Ddl`, `Poisoned`
- `Tx` — sealed write transaction (`begin`, `begin_serializable`, `execute`,
  `fetch_one`/`optional`/`all`, `setting`, `pg_txid`, `bind_esign_id`, `commit`,
  `rollback`)
- `retry_serializable` — re-run on `Error::Serialization`
- `ddl` — `ddl::check` (CASCADE ban, stray tables, app DELETE outside transient, TRUNCATE grants)
- `migrate` — `migrate::run`, `migrate::ADVISORY_LOCK_KEY`
- `security` — `UnattributableWrite`, `log_unattributable_write` (separate connection)

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_wicket_schema` — `wicket.schema_history` (app), `wicket.schema_class` (app);
  seeds class rows for `wicket`/`app`/`transient`/`audit`

## Tests (`tests/`)

- `tx_actor_bound_to_transaction_id` / `tx_sets_every_setting` /
  `bind_esign_id_stamps_mid_transaction`
- `connect_sets_utc_and_application_name` / `connection_defaults_survive_release`
- `write_without_context_aborts` / `session_level_actor_is_refused` / `refused_maps_42501`
- `pooled_connection_cannot_leak_actor` / `after_release_resets_state`
- `serialization_retry_reruns_on_40001` / `execute_and_fetch_helpers`
- `read_pool_selects_under_app_and_migrate` / `read_pool_insert_fails_at_grant`
- `failing_statement_then_commit_is_poisoned_and_persists_nothing` / `clean_tx_still_commits`
- `fetch_optional_none_does_not_poison`
- `migrate_runs_in_order_under_lock` / `migrate_is_idempotent` / `migrate_down_then_up`
- `tables_not_owned_by_login_role`
- `ddl_check_rejects_cascade` / `ddl_check_rejects_app_delete_outside_transient`
- `log_unattributable_write_unimplemented` / `log_unattributable_write_calls_audit_log_event`
- trybuild: `tx_has_no_deref`, `tx_has_no_public_constructor`,
  `read_pool_has_no_deref`, `read_pool_has_no_execute`

## Tx::commit and poison

Execute, fetch, and savepoint-style helpers on the sealed `Tx` mark it poisoned
on non-aborting `Err` (RowNotFound, decode — including a hook that reports
failure through those helpers). `Tx::commit` on a poisoned `Tx` rolls back
and returns [`Error::Poisoned`] instead of persisting partial work. A clean
`Tx` still commits. A PostgreSQL error already aborts the server transaction
(`COMMIT` is ROLLBACK), so those paths are not poisoned: callers that catch
`23505` / `42501` and still `commit` are unchanged. This is in-struct state
(no new public fields).

`Tx::commit` still does not consult the ledger's in-process unfinalized-sink
flag (there is no `Tx::poison`). An unfinalized posting sink marks poison by
transaction id when it is dropped; committing that work must go through
[`wicket_ledger::commit`], which returns `Error::Unfinalized` and rolls back.
Module code that posts through the kernel transition path finalizes or posts
inside the same `Tx` and then calls `Tx::commit` on success paths only.

## Frozen / seams

Frozen: `Tx::begin` / `WritePool` / `ReadPool` / `connect` hooks (CONTRACT §4 REAL
parts; D3 §2.1–§2.3). Other crates bind writes through `Tx`, never a raw pool
(CONTRACT §5a). `apply_ddl` / `attach_audit_trigger` remain `Unimplemented`;
`wicket-audit` is the attach path. Three URLs are never derived from each other
(CONTRACT §8): `WICKET_DATABASE_URL`, `WICKET_MIGRATE_DATABASE_URL`,
`WICKET_BOOTSTRAP_URL`.
