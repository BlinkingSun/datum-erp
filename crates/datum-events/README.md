# datum-events

Transactional outbox: a module publishes a typed `Event` inside the same
`datum_db::Tx` as its business change. The `app.event` row is visible only if
that change commits. `Dispatcher` delivers to in-process subscribers at least
once, outside the originating transaction, as a named service principal
(`WriteContext.source_kind = "job"`). Handlers must be idempotent.

## Public API (`src/lib.rs`)

- `Dispatcher` / `DEFAULT_MAX_ATTEMPTS` — claim `FOR UPDATE SKIP LOCKED`, invoke, retry, dead-letter
- `Error` / `Result`
- `Event` / `EventBuilder` / `EventKind` — typed payload; builder validates against schema
- `publish` — insert `app.event` in the caller's `tx`
- `EventSchema` / `Field` / `SchemaRegistry` — append-only payload contracts
- `EventHandler` / `HandlerFuture` / `Registry` — in-process `(name, subscriber) -> handler`
- `enable_subscription` — persist an enabled `app.subscription` row in `tx`
- `MIGRATOR` — `placeholder` + `0001_events`
- Modules: `dispatch`, `error`, `event`, `publish`, `schema`, `subscribe`

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_events` — `app.event` (app), `app.subscription` (app),
  `transient.delivery` (transient; not audited)

## Tests (`tests/`)

- `event_is_invisible_until_commit` / `rolled_back_transaction_publishes_nothing`
- `delivery_is_at_least_once` / `handler_runs_as_service_principal`
- `dead_letter_after_n_attempts` / `skip_locked_allows_two_dispatchers`
- `every_event_table_is_audited_except_transient` / `app_event_has_no_delete_path`
- `migration_is_reversible`

Lib: `builder_refuses_unknown_schema`, `builder_refuses_missing_required_field`,
`schema_registry_rejects_removed_field`.

## Frozen / seams

Frozen: `publish` in the caller's `Tx`, `enable_subscription`, `EventHandler::handle`
(CONTRACT §4). Payload contracts are append-only within a version. Kernel
composition (`datum-module`) currently constructs `Registry::new()` and does
not register the jobs genealogy bridge (FINDINGS-0 #1/#2).
