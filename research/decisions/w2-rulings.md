# Wave 2 rulings (kernel) — 2026-09-12

Orchestrator rulings and adjudications that bind the kernel as integrated. Each one is evidenced in
an unpublished run record (substance is in this file) and in spec addenda later promoted into
`docs/09-workspace-contract.md` at the Wave 2 close.

## Contract rulings

- **§5a amended.** The raw-SQL fence confines only the session protocol (`set_config`, `current_setting`,
  `raw_sql`, `copy_in_raw`, `QueryBuilder`, `query!` macros) to `wicket-db`, `wicket-audit`, `wicket-test`; the
  `sqlx::query*` functions are allowed everywhere and every crate ships the named test `writes_go_through_tx`.
  Reason: the sealed `Tx` cannot be driven through `query!` macros; the original fence was incompatible with D3.
- **§5a.1 — the fence is a law, not a grep.** Assembling a confined token at build or run time, generating
  query code into `OUT_DIR`, `allow(clippy::disallowed_*)` outside the three crates, `sqlx` in any `build.rs`,
  and `GRANT`/DDL/`CREATE DATABASE` from Rust in a module crate (src or tests) are Fail-class regardless of
  merit. Measured on `wicket-uom` attempt 1 (disqualified) and enforced since by `just lint-sql` greps
  (crate-level allows, `sqlx` in `build.rs`, `GRANT `/`CREATE DATABASE`, cross-module table reads).
- **§2 workspace members** gain the `modules/*` glob so Wave 2s modules register by existing under
  `modules/<name>/`.
- **§7 recipes:** `db-reset` skips `*-gc.sql`; `db-gc` drops stale `wicket_t_*` case databases (opt-in) using
  the harness's `COMMENT ON DATABASE` creation stamp.

## Batch rulings

- **2.1 wicket-db / 2.2 wicket-audit** — see the run record (Wave 1 close and batch 2.1/2.2 lines).
- **2.3 wicket-identity (R-2.3-IDENTITY).** Split audit ruled FAIL with a scoped rework: built-ins are inserted by
  migration 0001 inside one sqlx transaction with the audit trail on; `NOT VALID` on the partitioned
  `audit.event` was a spec defect (constraint added valid); SQLSTATE-and-constraint error mapping; login
  records device/ip; TOTP wrap removed pending Wave 2b. Winner: the grok rework.
- **2.3 wicket-uom (R-2.3-UOM).** Attempt 1 disqualified for fence evasion. Attempt 2: the cursor lineage
  (rework by grok, cycle-1 by cursor) won the final head-to-head over the grok one-shot on spec table names,
  fence-in-tests, and respecting the ledger's `ledger.posting` seam; cycle 2 added inverse inference, re-pin
  closing the open row, and a dependency-guarded `DROP EXTENSION btree_gist` on down. The `to_stock`
  signature was frozen until the ledger landed.
- **2.3 numbering / events** — single lanes, audited pass; private test bootstraps (GRANT from Rust) removed
  by the conformance lane; `TestDb::bootstrap_pool` is the only sanctioned superuser pool.
- **2.4 wicket-ledger (THE GATE).** Both attempts double-failed; ADDENDA 1–2 (18 items) consolidated every
  fail-class finding; the three-way final adjudication chose the grok lineage's rework as the base (keeps
  `post(tx, builder)`, incremental `apply_group`, exact `ZL000`–`ZL007`, a real canary, allocator
  independence); cycle 1 closed per-location allocation, the Drop/xid poison, and the property generator;
  cycle 2 broadened the generator to every group kind and made shards generator-only. Final deep audit PASS
  with the §7 suite green in commit mode and the canary armed.
- **2.5 wicket-statemachine.** Head-to-head returned no winner (both skipped `finalize` on an after-hook error);
  the grok attempt was ruled the base (sealed `HookView` ABI, harness-only tests); the rework race winner (grok)
  fixed finalize-on-error, the four-node diamond topo test, freeze gating, and the lost-update test; cycle 1
  finalized on before-hook errors too and defined `NoPostings`/`NoSink` semantics.
- **2.5 wicket-jobs.** First audit failed on fence allows and schema class; the grok rework won (queue tables in
  `transient`, `run_log` in `app`); a Windows-only timing race in the progress test was replaced by a handshake.
- **2.6 wicket-module.** The cursor attempt won (migrator order with `schema_history` attach and audit on every
  app table; the grok attempt missed `zz_audit_row` on three crates); cycle 1 wired the gate from the profile
  TOML, the §6.3 required-signature set, runtime enable refusal, key-10 persistence, the events hook, and
  install-time migrations in-transaction; cycle 2 (SPEC-module ADDENDUM 1) is the composition glue the
  phase-end master found missing: register-before-freeze, gate-wrapped transitions, hook postings through
  `ledger::post` in the same transaction, units in-transaction, live events/jobs, registries from manifests.

## Process rulings

- A lane that exits with an uncommitted worktree cannot win an adjudication; when a lane's work is complete and
  green the orchestrator may commit it mechanically with an attribution trailer and say so.
- Cycle counter: supervisor first-fail rework races are cycle 0; orchestrator-named post-integration reworks
  (`<crate>-c1`, `-c2`) are not rework-class lanes.
- Every landing gets one reproduction audit before the gate assertion counts it; a killed audit with a
  complete report is re-run rather than overridden.

## Wave 2 close — carry list (FINDINGS-1, 2026-09-12 11:50, PHASE: ready)

FINDINGS-0 findings 1–4 re-scored CLOSED on main 88da4d2 (module-c1 + module-c2-r2 glue, kernel READMEs, `migrate_down_then_up` + cross-module lint, kernel end-to-end path `kernel_e2e.rs` after kernel-e2e-c1). Carried into Wave 2s as the `w2-carry` lane, not blocking the close:

| # | Residual (FINDINGS-1) | Disposition |
|---|---|---|
| 5 | `docs/09` behind the live CONTRACT | closed by this close (promotion) |
| 6 | `uom.posting_stub` leftover (`uom.up.sql:38–44`, `order.rs:126`) | remove; ledger owns `ledger.posting` |
| 7 | identity 0001 seed INSERTs run before `zz_audit_row` attaches | attach kernel audit before builtin seeds; named both-profile test |
| 8 | audit-trigger coverage is list-based (`KERNEL_AUDIT_RELS`) | named both-profile matrix test over discovered app-class tables |
| 9 | `Tx::commit` has no poison check | ruling: by design (ledger `Unfinalized` on Drop; modules commit through the kernel path); document in wicket-db README, no code |
| s1 | `transition_context` does not stamp `config_version` (glue receipt empty) | kernel stamps `app_version` + `config_version` on every transition context |
| s2 | glue item-1 test never spawns `extra.doc` | test spawns and transitions the registered machine |

Seams held for the slices (SPEC-mod-common addendum): hook ABI has no `Tx`; Before-hooks not on the builder; Required edges need a bound gate (Wave 2b esign); lot-less receipts publish `inventory.receipt_posted` v1.
