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
- `Error` / `Result` — `Frozen` / `NotFrozen` / `Veto` / `HookBudgetExceeded` / `AfterHookCannotVeto` / `StartupGate` / …
- `MIGRATOR` — `placeholder` + `0001_statemachine`

`Engine` methods other crates call: `new`, `set_module_graph`, `register_machine`,
`register_hook`, `freeze`, `hook_order`, `edges_for_manifest`, `persist`, `spawn`,
`transition`.

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_statemachine` — schema `sm` (app): `sm.machine`, `sm.state`,
  `sm.edge`, `sm.instance`

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
- `migration_is_reversible` / `catalogue_accepts_sm_schema`
- trybuild: `signature_declaration_has_no_default`

Lib: `regulated_machine_requires_total_declaration`, `non_regulated_absence_means_none`,
`manifest_lists_both_kinds_with_reasons`.

## Frozen / seams

Frozen: hook ABI, `SignatureDeclaration`, `Engine::{persist,spawn,transition,freeze}`
(CONTRACT §6.2 rule 8, §6.3). Ledger coupling is trait-only (`&mut dyn PostingSink`);
`cargo tree` must not show `datum-ledger`. Real `SignatureGate` is Wave 2b
`datum-esign`. `Kernel::build` currently freezes with zero `register_machine`
(FINDINGS-0 #1/#2).
