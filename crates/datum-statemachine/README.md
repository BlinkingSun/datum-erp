# datum-statemachine

Declarative state machines, uniformly audited, with a frozen hook ABI. Uses
`core::PostingSink` and `core::SignatureGate`; never depends on `datum-ledger` or
`datum-esign` (CONTRACT §4).

## Public API (`src/lib.rs`)

- `DocRef` — `(doc_type, id)` the instance is bound to
- `Edge` / `EdgeBuilder` — named transition; builder sets signature declaration
- `Instance` — persisted `sm.instance` row (state + version)
- `Machine` / `MachineBuilder` / `MachineId` / `State` / `Transition`
- `ManifestEdge` — edge listing for `docs/03` §8 (both Required and NotRequired)
- `SignatureDeclaration` — `Required { meaning, permission }` or `NotRequired { reason }` (no `Default`)
- `action_for` / `with_action` — `datum.action` strings for the audit trigger
- `Engine` — register machines/hooks, freeze, persist/spawn/transition
- `HookPhase` / `HookView` / `Veto` — hook ABI
- `ModuleNode` — module id + `depends_on` (topological order; ties by id)
- `check_gate_binding` — release build fails if a Required edge meets `NoSignatures`
- `current_state` / `instance_exists` / `machine_id_for` — live-state query seam (see table)
- `Error` / `Result` — `Frozen` / `NotFrozen` / `MachineChanged` / `Veto` / `HookBudgetExceeded` / `AfterHookCannotVeto` / `StartupGate` / …
- `MIGRATOR` — `placeholder` + `0001_statemachine` + `0002_query_seam` + `0003_engine_write`

`Engine` methods other crates call: `new`, `set_module_graph`, `register_machine`,
`register_hook`, `freeze`, `hook_order`, `edges_for_manifest`, `persist`, `spawn`,
`transition`.

### Query seam (kernel crates call these; they never read `sm.instance` / `sm.machine`)

Instance identity is `(doc_type, doc_id)` (`DocRef`).

| Function | Signature | Source of truth |
|---|---|---|
| `current_state` | `async fn current_state(tx: &mut Tx<'_>, doc: &DocRef) -> Result<Option<State>>` | `sm.instance.state` via `sm.current_state(text, uuid)` |
| `current_state_on` | `async fn current_state_on(pool: &ReadPool, doc: &DocRef) -> Result<Option<State>>` | same, through `ReadPool` (no actor) |
| `instance_exists` | `async fn instance_exists(tx: &mut Tx<'_>, doc: &DocRef) -> Result<bool>` | `sm.instance` via `sm.instance_exists(text, uuid)` |
| `instance_exists_on` | `async fn instance_exists_on(pool: &ReadPool, doc: &DocRef) -> Result<bool>` | same, through `ReadPool` |
| `machine_id_for` | `async fn machine_id_for(tx: &mut Tx<'_>, doc_type: &str) -> Result<Option<MachineId>>` | `sm.machine.id` via `sm.machine_id_for(text)` |
| `machine_id_for_on` | `async fn machine_id_for_on(pool: &ReadPool, doc_type: &str) -> Result<Option<MachineId>>` | same, through `ReadPool` |

All three SQL helpers are **invoker-rights** (not `SECURITY DEFINER`; R-2s-8 does not
exempt this crate): `datum_app` has `SELECT` on `sm.instance` / `sm.machine`.
`EXECUTE` is granted only to `datum_app` (`REVOKE` from `PUBLIC`). They do **not**
call `audit.require_context`, so a `datum_db::ReadPool` fetch (no actor) succeeds.

### Write seam (R-2s-5)

`datum_app` has **no** `INSERT` / `UPDATE` / `DELETE` on `sm.instance`. A raw
`UPDATE sm.instance` is SQLSTATE `42501`. Spawn and transition write only through
`sm.spawn_instance` / `sm.transition_instance` (invoker-rights; `GRANT EXECUTE` to
`datum_app` only). Those helpers DML `sm.instance_engine`, a `datum_owner` view of
`sm.instance` (default view rights: the owner accesses the base table). A `BEFORE
INSERT OR UPDATE` trigger on `sm.instance` refuses writes unless the helper set
`sm.engine_write` for the statement. No `SECURITY DEFINER`.

| Relation / function | `datum_app` SELECT | INSERT | UPDATE | DELETE | EXECUTE |
|---|---|---|---|---|---|
| `sm.instance` (0001, before 0003) | t | t | t | f | — |
| `sm.instance` (after 0003) | t | f | f | f | — |
| `sm.instance_engine` (view) | t | t | t | f | — |
| `sm.spawn_instance` / `sm.transition_instance` | — | — | — | — | t (`REVOKE` PUBLIC) |

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_statemachine` — schema `sm` (app): `sm.machine`, `sm.state`,
  `sm.edge`, `sm.instance`
- `00000000000002_query_seam` — `sm.current_state(text, uuid)`,
  `sm.instance_exists(text, uuid)`, `sm.machine_id_for(text)` (invoker-rights;
  `GRANT EXECUTE` to `datum_app` only)
- `00000000000003_engine_write` — revoke `INSERT`/`UPDATE`/`DELETE` on
  `sm.instance` from `datum_app`; `sm.instance_engine` view; `sm.spawn_instance`,
  `sm.transition_instance`, guard trigger (invoker-rights; `GRANT EXECUTE` to
  `datum_app` only)

## Tests (`tests/`)

- `hooks_run_in_topological_order_ties_by_module_id`
- `veto_aborts_transaction_and_names_module` / `hook_budget_overrun_fails_loudly`
- `one_sink_per_transaction_and_finalize_after_all_hooks`
- `required_edge_refuses_without_token` / `required_edge_refuses_under_no_signatures`
- `not_required_edge_runs_without_gate_call`
- `transition_writes_one_audit_row_with_action` / `permission_denied_is_typed_and_leaves_no_row`
- `after_hook_cannot_veto` / `before_hook_error_still_finalizes_sink` / `after_hook_error_still_finalizes_sink`
- `no_postings_finalize_reports_no_sink` / `transition_requires_frozen_registry`
- `concurrent_transition_is_rejected` / `writes_go_through_tx`
- `startup_fails_when_required_edge_meets_no_signatures_in_release`
- `build_twice_same_declaration_is_idempotent` / `changed_declaration_is_refused`
- `migration_is_reversible` / `catalogue_accepts_sm_schema`
- Query seam: `current_state_none_then_some_after_spawn_then_released`,
  `query_seam_on_read_pool_under_app_and_migrate` (both LOGIN roles)
- Write seam: `direct_update_instance_as_app_is_42501`,
  `engine_transition_succeeds_for_app_and_migrate` (both LOGIN roles)
- trybuild: `signature_declaration_has_no_default`

Lib: `regulated_machine_requires_total_declaration`, `non_regulated_absence_means_none`,
`manifest_lists_both_kinds_with_reasons`.

## Frozen / seams

Frozen: hook ABI, `SignatureDeclaration`, `Engine::{persist,spawn,transition,freeze}`
(CONTRACT §6.2 rule 8, §6.3). `persist` is idempotent on the declaration
(`doc_type`, states, edges, signature requirements): a second persist of an
identical catalog is a no-op and keeps the existing machine id; a different
declaration for an existing `doc_type` is `Error::MachineChanged` (never a silent
UPDATE; one audit row only when a row is written). Ledger coupling is trait-only
(`&mut dyn PostingSink`); `cargo tree` must not show `datum-ledger`. Real
`SignatureGate` is Wave 2b `datum-esign`. `Kernel::build` currently freezes with
zero `register_machine` (FINDINGS-0 #1/#2).
