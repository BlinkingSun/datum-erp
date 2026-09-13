# PLAN AUDIT — Wicket ERP foundation build

**Auditor:** grok-master (plan-audit, stays open)  
**Date:** 2026-09-11  
**Roster rev:** 4  
**Verdict: REVISE**

Sweep: 5 cursor / 5 grok (`sweep-plan-*`). Native `spawn_subagent` lookups: 0.  
Sources: PLAN.md, `docs/00`–`04`, ADRs 0001–0007, ten sweep reports under `_team/reports/sweep-plan-*.md`, eCFR 21 CFR 11 (2026-09-10), PostgreSQL 17/18 docs, ERPNext `develop` SLE/Bin, Odoo CE/EE quality split, Paperless Parts.

This file is the sole writer of `plan-audit.md`. Sweep reports are evidence, not the deliverable.

---

## 0. The question you asked

**The ledger thesis is not wrong. The invariant PLAN wrote down is.**

An append-only posting journal with rebuildable projections is a sound foundation for this domain, and it is also **not a market invention** — ERPNext already derives qty from `tabStock Ledger Entry` and treats Bin as a cache (and still desyncs them in production; GitHub erpnext#54528, #51562). Wicket’s bet is executing that thesis **without mutation, without stored running balances on the posting, and with a DB-enforced conservation law**. That is an implementation bet, not a wedge.

What *is* wrong, and must not be typed into thirteen crates:

1. **“Every posting group sums to zero” as `SUM(quantity)` over `group_id`.** A work-order completion of −4 screws, −1 housing, +1 assembly is a legitimate business transaction whose scalar sum is −4. Virtual locations close *appearance from nowhere*; they do not make unlike items add. The real invariant is **per balance slice**: inventory `(group_id, item_id, uom_id)`; cost/labor `(group_id, ledger)` (finer keys TBD).
2. **`Quantity` carrying UoM as a type parameter.** Customer-defined, item-specific units are runtime data. Parameterize **dimension** (Length vs Mass vs Count). Units are `UnitId`.
3. **“Audit produced by the persistence layer” via SQLx.** SQLx has no interceptor. The guarantee is a PostgreSQL trigger. The ADR’s “app role has INSERT on audit” **contradicts** “cannot write a false one.”
4. **Bundled PostgreSQL as the ten-minute install on Windows.** Not realistic for v1 without admin + a Windows Service. ADR 0003 already named the revisit; fire it now.

Until those four are decided in writing, Wave 1 must not freeze public types.

---

## 1. RISKS (edge cases included)

### R1. Scalar zero-sum rejects real production (or accepts lies)
BOM completion, mixed UoM (feet/inches/pounds), labor hours mixed with inventory qty, and multi-ledger groups (inventory+cost in one `group_id`) all break `SUM(quantity) = 0` over the group. If Wave 2 encodes PLAN §6.2 as written, either WO completion cannot commit or the constraint is theater.  
**Forcing case:** 4 EA screws + 1 EA housing → 1 EA assembly.  
**Empty-group hole:** a `FOR EACH ROW` deferred constraint trigger **does not fire** on `COMMIT` with zero inserts. An empty group would “balance.” Need a `posting_group` header row (or an application refuse **and** a test).  
**Concurrent projection:** the DB constraint does not protect the cache. Same-txn projection upsert + row locks, or floor reads will tear. ADR 0004 rejected CQRS lag; it did not specify the lock.

### R2. `Quantity<U: Unit>` poisons `wicket-core`
PLAN §5 and ADR 0002’s “inches and millimetres are different types” are false for an ERP whose units live in a database. Freezing `Quantity<Foot>` in Wave 1 is a workspace-wide rewrite. Catch-weight (each + kg) cannot live in one conservation column.

### R3. SQLx cannot be the audit persistence layer
No UoW, no callbacks, no dyn-safe `Executor` wrapper that sees OLD/NEW. Pool hooks are connection-lifetime. `query!` is opt-in. A sealed trait + clippy grep is house style — every Wave 2 crate can `sqlx::query!` around it. **Triggers are the guarantee; `Tx::begin(Actor)` is how context is set.**  
**GUC leakage:** session-level `SET` (not `SET LOCAL` / `set_config(..., true)`) + SQLx pool without `DISCARD ALL` audits the next HTTP request as the previous operator.  
**`session_replication_role = replica` and `DISABLE TRIGGER`:** owner/superuser bypass.

### R4. Grant-level append-only is theater on a bundled cluster
The Windows/macOS user who owns `PGDATA` is superuser. They can `UPDATE audit.event`, disable triggers, restore a tampered dump. PLAN invariant 3 as inspector-facing prose is a 483. App-role grants are necessary against **application bugs** and insufficient against the **shop admin**. Hash chain is tamper-evident only if something **off the box** verifies it.

### R5. Wave 2 “thirteen parallel lanes” is a false DAG story
Declared graph is acyclic. Practice:
- First demanded **edge:** `wicket-documents → wicket-statemachine` (approval workflow **is** a state machine). Also `documents → numbering`.
- First demanded **cycle:** `statemachine → ledger` (hooks contribute postings) **and** `ledger → statemachine` (posting marks the source document Posted). Invert with `PostingSink` in `wicket-core`. Events cannot substitute (async, different transaction, group cannot balance).
- `wicket-ledger → wicket-identity` is a **false alarm** if `posted_by: Actor` stays in core.
- `wicket-module` depends on everyone → not a parallel Wave 2 lane.
- Phase 0 has 13 **components**; PLAN §5 has 15 **directories**; Wave 2 says 13 **lanes** and names none. Reporting+print is in the kernel list and **absent from the crate graph**.

### R6. Wave 1 stubs will not let Wave 2 compile tests
`todo!()` + `clippy::todo = deny` + isolated worktrees = Wave 2 property tests cannot compile against sibling crates. sqlx `prepare --workspace` at repo root is a 13-way `.sqlx` merge bomb. `sqlx::test` wrapping in a rolled-back transaction **never fires deferred constraint triggers**.

### R7. Bundled PostgreSQL on Windows fails the ten-minute test
User-process `pg_ctl` dies on logoff (shop tablets go dark). Windows Service needs admin. Firewall dialog, Defender locking WAL, `localhost` → `::1` 10s fallback vs 500ms scan budget, `pg_upgrade` needs both binaries + admin, VC++ redist UAC. **No named production desktop app silently lifecycle-manages Postgres on Windows without admin and without Docker.** Odoo’s all-in-one requires UAC and discourages Windows production. PGlite is single-connection WASM — rejects ADR 0004’s concurrency and grants.

### R8. Part 11 is not covered by the kernel list as PLAN implements it
Covered (if Wave 2 actually builds them): 11.10(e) generation, 11.70 binding, identity/RBAC, state machines, e-sign hash, re-auth sentence.  
**Missing crates/scope that inspectors walk:** 11.10(b)(c) complete copies + retention, 11.50(b) manifestation **on printouts**, backup/restore (Annex 11 §7.2), 11.300 aging/lockout/unauthorized-use reporting, 11.200 two components + admin-reset collusion, gap-free numbering (PG `SEQUENCE` is not transactional — rolled-back insert leaves a gap you have to explain).  
QMSR replaced 820.70(i) on 2026-02-02; `docs/06` does not exist yet (gated — good) and must not cite withdrawn clauses.

### R9. Five extension points cannot express the customization shops pay for
Regulated **server PDF / traveler / CoC / DHR print templates** match none of events, hooks, custom fields, routes, or UI slots. That is the fork driver (every beachhead shop customizes travelers). Print is kernel in `docs/02`/`docs/04` and missing from PLAN and from `docs/03` §3. Second fork: inserting a host state (`FirstArticleHold`) — hooks can only veto.

### R10. Differentiation claim in `docs/01` §5 is half-true
- Append-only inventory ledger: **ERPNext already does the thesis**, badly. Not a wedge.
- Quality first-class: ERPNext ships QC in **core**; Odoo Quality is **Enterprise-only**. Wicket’s depth is Phase 4.
- CAD-native estimating: **Paperless Parts already owns CAD-to-quote** for this exact buyer and integrates into JobBOSS/ProShop. Phase 7 module, not an unfair advantage to name in §5.
- License: ADR 0006 says ERPNext and Odoo Community use AGPL. **False.** ERPNext is GPL-3.0; Frappe is MIT; Odoo CE is LGPLv3 since Odoo 9.
- Real wedge: **ProShop’s ERP+QMS thesis, as OSS, with Part 11 structural, at Qualio money.** That is Phase 4. Kernel-only is not demoable. Phase 1 inventory+lots **must** land in this build (catalog: that is how you find out the ledger is wrong). PLAN Wave 3 already includes Phase 1; PLAN §9 “Waves 1 and 2 are kernel only” will be misread as scope.

### R11. License ADR is Open and Wave 1 would stamp it
ADR 0006 is the one irreversible decision. `doc-repo` writing `LICENSE` and workspace `Cargo.toml` `license =` before it closes is a relicensing trap.

---

## 2. GAPS (missing subtasks / untested acceptance criteria)

| ID | Gap | Why it blocks |
|---|---|---|
| G1 | No opus DECISION files for Quantity, ledger slice keys, audit persistence, bundle-vs-escape-hatch, print crate, hash-chain | Wave 1 would freeze the wrong API |
| G2 | PLAN §6.2 invariant is false as a scalar; no slice keys; no empty-group header | Highest-value test has nothing true to assert |
| G3 | PLAN §7 is a slogan, not ACs | Wave 2 can ship `sum(qty)==0` on `Vec<i32>` and go green |
| G4 | Wave 2 lanes unnamed; “13” ≠ crate list; no SPECs | Exec-master cannot write a manifest |
| G5 | Stub contract unspecified (`todo!()`, sqlx offline, TestDb, toolchain pin) | Isolated worktrees cannot compile tests |
| G6 | No `wicket-test` crate; no `dev/compose.yml` Postgres; no two `DATABASE_URL`s (app vs migrate) | Invariant 3 untestable; `query!` unpreparable |
| G7 | `wicket-core` treated as a stub peer; it must be **complete** in Wave 1 after Quantity DECISION | Everything imports it |
| G8 | Missing `wicket-print` (or explicit fold into `wicket-documents`) | 11.50(b); Annex 11 §8; extension-point fork |
| G9 | Missing backup/restore task (claimed in `docs/02` §8) | Hidden PG + 11.10(c) |
| G10 | No roles/GRANTs/`Tx::begin`/fail-closed trigger/clippy deny-sqlx in Wave 1 | “Produced by persistence layer” is a comment |
| G11 | ADR 0005 grants app INSERT on audit | Contradicts “cannot write a false one” |
| G12 | `wicket-documents` missing SM and numbering edges | Lane cannot implement approval workflow |
| G13 | No `PostingSink` in core | Hook ABI will create SM↔ledger cycle |
| G14 | `wicket-module` placed as parallel Wave 2 | Cannot honestly register real ABIs against stubs |
| G15 | Hook ABI (veto schema, posting envelope, order, budget) unstated | Wave 3 training/calibration veto will invent traits |
| G16 | `wicket-numbering` algorithm unstated; SEQUENCE hole | Historical gaps are permanent audit questions |
| G17 | `wicket-identity` 11.200/11.300 scope unstated (aging, lockout, two components, admin reset) | Schema now or a finding later |
| G18 | Audit row type missing reason / optional `source_device_id` / export DTO | Cannot reconstruct; 11.10(h); Annex 11 §9 |
| G19 | Hashed module-manifest format unstated | IQ attachment in Phase 6 has nothing to attach |
| G20 | Customer-runnable IQ suite unmentioned in PLAN §7 | `docs/02` §9 claim; test-name stability now |
| G21 | sqlx/`sqlx-cli`/rustc unpinned; edition/MSRV unstated | Worktree/CI drift |
| G22 | ADR 0008 referenced as binding, file not written; ADR 0006 Open | First public commit blockers |
| G23 | `docs/01` §5 and ADR 0006 license facts need sentence-level edits | README (`doc-repo`) will copy the lie |
| G24 | `doc-datamodel` in Wave 1 parallel with unsettled ledger/UoM | Will be rewritten |
| G25 | `.gitignore` dual-owned by `workspace` and `doc-repo` | Collision |
| G26 | Bundling accidentally in-scope via Wave 3 Tauri, zero lanes | Unestimated surprise |
| G27 | Property-test suite vs `sqlx::test` rollback vs deferred triggers unspecified | Tests that never COMMIT “pass” |
| G28 | `cargo test --workspace` after Wave 2 will exceed 10 min if unsharded | Importeng-class serial tail |

---

## 3. Decisions that must land before Wave 1 types freeze

Opus, laned, written into SPEC + `_team/reports/DECISION-*.md`. Not Claude build lanes.

| Decision | Options (recommended first) | Blocks |
|---|---|---|
| **D1 Quantity** | Dimension phantom + runtime `UnitId` + separate `Money`; not `Quantity<U: Unit>` | `wicket-core`, every crate |
| **D2 Ledger slice** | Inventory `(group, item, uom)`; lot/serial **not** conservation keys; cost/labor per ledger; canonical stocking UoM before insert; `posting_group` header for empty-group | `wicket-ledger` trigger + proptest ACs |
| **D3 Audit persistence** | Trigger + fail-closed GUC + sealed `Tx` + lint; **revoke INSERT** on audit from app role (`SECURITY DEFINER` only); time = `clock_timestamp()` or freeze `now()` with one-group-per-txn | `wicket-db`, `wicket-audit`, Wave 1 roles |
| **D4 Tamper-evidence** | Grant+trigger for v1 app-bug net; hash chain **if** marketing keeps “cannot obscure,” with off-box verify; **do not** pgcrypto with key in PGDATA | `wicket-audit` row type |
| **D5 Bundle PG** | **Escape-hatch-first** (require external Postgres for v1); amend ADR 0003 + vision §7; Wave 1 still ships test Postgres | Wave 3 Tauri, ten-minute claim |
| **D6 Print** | Add `wicket-print` to §5 **or** fold into `wicket-documents` with esign edge, **and** sixth extension point (template packs) | Wave 1 member list; 11.50(b) |
| **D7 License** | Close ADR 0006 before `LICENSE` / `Cargo.toml license` | `doc-repo`, workspace |
| **D8 Conservative 11.200** | Always all identification components (ignore continuous-session relaxation) until a shop asks | `wicket-esign` |

D1–D3–D5–D6 are the ones that change PLAN text before any executor types.

---

## 4. Per-subtask EXECUTOR / SPLIT / TIER

Legend: **differs from PLAN** is marked. PLAN currently says `assign` for everything and deep only for `workspace`, `doc-regulatory`, and (implied) core.

### 4.1 Wave 1

| Lane | EXECUTOR | SPLIT | TIER | Notes |
|---|---|---|---|---|
| `workspace` | **grok** (not blind-race) | **serial**; do **not** split skeleton vs stub-APIs (they collide on every `Cargo.toml`) | **deep + `audit.double`** | Gated on D1, D3, D6, stub SPEC (sweep-plan-wave1-stubs.md §4). Ships **complete** `wicket-core`, `wicket-test`, roles SQL, `Tx::begin` stub that is real, no-op migrations, per-crate `.sqlx` policy, `dev/compose.yml`, rustc pin. |
| `doc-datamodel` | grok or cursor | after D1/D2 or it rewrites | standard | **Defer until slice keys exist** (PLAN has it parallel — differ). |
| `doc-roadmap` | grok/cursor | parallel | standard | |
| `doc-api` | grok/cursor | parallel | standard | |
| `doc-repo` | grok/cursor | **after D7** for LICENSE | standard | Coordinate `.gitignore` with workspace. Do not copy `docs/01` §5 unedited. |
| `doc-adr` | grok/cursor | parallel, but must **amend 0003/0004/0005** not only write 0008/0009 | standard | 0008 is referenced as binding and missing. |
| `doc-regulatory` | grok/cursor | gated on spike — keep | **deep** (agree) | Clause table; QMSR retarget; closed-system claim. |
| `doc-landscape` | grok/cursor | gated on spike — keep | **deep** (PLAN said standard — **differ**: this is the wedge document) | Lead with ProShop + Qualio, not Odoo vs SAP. |
| `spike-ledger-constraint` | cursor | **add**; after D2 | standard | PG17 EXPLAIN of deferred trigger; empty-group hole; sqlx commit mapping. |
| `spike-ledger-proptest` | grok | **add**; after workspace stubs | standard | Failing harness against frozen API. Not inside the workspace lane. |

### 4.2 Wave 2 kernel (after merged Wave 1)

PLAN “all parallel” is compile-parallel against stubs only. **Done-parallel is batched.** Ledger remains the Wave 2 gate (PLAN §9) — keep that, and stop calling it one of thirteen equals.

| Lane | EXECUTOR | SPLIT | TIER |
|---|---|---|---|
| `wicket-core` | **cursor**; **serial batch 1**; **not a stub** | one crate | **deep + double** |
| `wicket-db` | **cursor** | serial batch 2 after core | **deep** (PLAN unspecified — **differ**) |
| `wicket-audit` | **cursor** | serial after db | **deep + double** |
| `wicket-identity` | **cursor** | after audit; authn/RBAC/principals one crate | **deep** |
| `wicket-esign` | **cursor** | serial after identity | **deep** |
| `wicket-uom` | **cursor** | after core D1; ∥ identity | **deep** |
| `wicket-numbering` | **grok** | after core+db; **no SEQUENCE** | **deep** for gap-free (PLAN would be standard — **differ**) |
| `wicket-events` | grok | after core+db | standard |
| `wicket-jobs` | grok | after events; **no identity edge** (`Actor` at construct) | standard |
| `wicket-customfields` | grok | after audit | standard |
| `wicket-ledger` **engine** | **cursor**; **blind-race after D2 freeze** | `src/` + migrations; serial after uom+audit | **deep + double** |
| `wicket-ledger` **proptest** | **grok** (other family) | `tests/` or `wicket-ledger-proptest`; **split from engine** (protocol — **differ**) | **deep + double** |
| `wicket-statemachine` | **cursor** | after identity+esign; `PostingSink` not ledger types | **deep** |
| `wicket-documents` | **cursor** | **serial after SM + numbering** (new edges) | **deep** (PLAN would treat as standard — **differ**) |
| `wicket-print` | grok | **add**; after documents / with documents | **deep** once added |
| `wicket-module` | **cursor** | **serial Wave 2.5** (or thin registry ∥ with deps `core db audit` only) | **deep** |
| `wicket-server` | cursor | Wave 3 composition root | **deep** |

**Do not add:** `ledger → identity`, `statemachine → ledger`, `jobs → identity`, `uom → items`, `wicket-gl`.  
**Do add:** `documents → statemachine`, `documents → numbering`, `PostingSink` in core.

### 4.3 Wave 3

Phase 1 modules (`items`, `locations`, `inventory`, `lots`; valuation may slip) are **in this build** (PLAN §3 Wave 3 + catalog de-risk). **Do not** pull Phase 4. **Do not** treat Wave 3 as optional. UI gate does not block backend Phase 1. Bundling is **out of this build** (D5).

---

## 5. SHARD (suites likely > 10 minutes)

| Suite | Shard |
|---|---|
| Wave 1 `cargo test --workspace --lib` | **no** (seconds–1 min) |
| Wave 1 clippy `-D warnings` 15 crates | own job; rust-cache; OS matrix already shards |
| `cargo test -p wicket-ledger` default | **fast set only** (<2 min target): in-memory 256 cases + 64 PG cases P1/P2/P5/P6. See sweep-plan-proptests.md §7.4 |
| Ledger PG every-prefix rebuild | `gates-shard-ledger-pg-rebuild-prefix` (will exceed 10 min if combined) |
| Ledger concurrency P8 | `gates-shard-ledger-conc` |
| Ledger extremal Decimal / 20-row / phantom / hooks | `gates-shard-ledger-extremal` |
| Per-crate migration up/down | `gates-shard-migrate` |
| 10M posting rebuild | **bench, not `cargo test`** |
| `cargo sqlx prepare --check` | per-crate, not `--workspace`; fold into clippy or sqlx shard |
| Windows + sqlx macros cold | OS matrix shard; do not serialize Linux then Windows |

Pre-declare shards in the Wave 2 manifest. Do not discover this at INTEGRATE.

---

## 6. PLAN text that must change (minimum)

1. §5: replace “`Quantity` carrying its unit of measure as a type parameter” with dimension+`UnitId` (D1). Add `PostingSink`. Add `wicket-test`. Add `wicket-print` or fold. Name every Wave 2 lane. Add documents→SM, documents→numbering.
2. §6.2: per-slice zero-sum; lot/serial not conservation keys; empty-group header.
3. §6.3: app role **SELECT only** on audit; INSERT via `SECURITY DEFINER` trigger; qualify “append-only” as application-role.
4. §7: replace the one-liner with pointer to ledger ACs (sweep-plan-proptests.md §2). Distinguish in-memory vs PG vs cross-crate. Ban `sqlx::test` rollback as the deferred-constraint test.
5. §3: Wave 2 is compile-parallel, done in batches; `wicket-module` is 2.5; Phase 1 modules are a named gate of **this** build (Wave 3), not “kernel only” as scope.
6. §9: ledger risk is **engineering**; Phase 1 inventory+lots is how it is de-risked. Stop implying Wave 2 close = ledger bet won.
7. §10: add **bundled PG lifecycle out of scope**; v1 = escape hatch. Keep Phase 2+ out.
8. Wave 1 workspace acceptance: stub SPEC (sweep-plan-wave1-stubs.md §12), rustc pin, two roles, compose Postgres, no workspace-root `.sqlx`, no `todo!()`.
9. Docs: `docs/01` §5 sentence-level honesty (competitive report §10); ADR 0003 split keep-dialect/defer-bundle; ADR 0004 slice keys; ADR 0005 INSERT grant; ADR 0006 license facts (ERPNext is GPL-3.0, Odoo CE is LGPL).

---

## 7. What is *not* a reason to kill the project

- The append-only journal idea. ERPNext’s bugs (ghost Bin, concurrent `qty_after_transaction`, cancel mutates SLE) are evidence the thesis is right and their execution is not.
- Kernel audit + e-sign as architecture. That is the actual wedge vs OCA auditlog / Frappe Version (deletable, opt-in) and vs Odoo CE (no quality, no sign).
- Modular monolith, Postgres-only, no GL first, Phase 4 as the product, Phase 1 as the de-risk. Those hold.
- Five extension points as an anti-Odoo line. Keep the line; add print templates as a bounded sixth; refuse behavior override.

---

## 8. Sweep index

| Slice | Lane | Family | Report |
|---|---|---|---|
| 1 Ledger constraint | sweep-plan-ledger | cursor | `_team/reports/sweep-plan-ledger.md` |
| 2 Audit/SQLx | sweep-plan-audit-sqlx | grok | `_team/reports/sweep-plan-audit-sqlx.md` |
| 3 Crate graph | sweep-plan-crate-graph | cursor | `_team/reports/sweep-plan-crate-graph.md` |
| 4 Wave 1 stubs | sweep-plan-wave1-stubs | grok | `_team/reports/sweep-plan-wave1-stubs.md` |
| 5 Typed quantities | sweep-plan-typed-qty | cursor | `_team/reports/sweep-plan-typed-qty.md` |
| 6 Property tests | sweep-plan-proptests | grok | `_team/reports/sweep-plan-proptests.md` |
| 7 Bundled PG | sweep-plan-bundled-pg | cursor | `_team/reports/sweep-plan-bundled-pg.md` |
| 8 Part 11 | sweep-plan-part11 | grok | `_team/reports/sweep-plan-part11.md` |
| 9 Extension points | sweep-plan-ext-points | cursor | `_team/reports/sweep-plan-ext-points.md` |
| 10 Competitive | sweep-plan-competitive | grok | `_team/reports/sweep-plan-competitive.md` |

Process note: first `team-sweep --wait` returned immediately because this Windows node marks live lanes `DIED` at dispatch; `--wait` treats `DIED` as terminal and the command Job Object killed the workers. Second dispatch used `team-sweep` without `--wait` plus a waiter on report files (same 1:1 split). Do not treat board `DIED` as evidence the research did not run.

---

**End of plan-audit.md.** Channel stays open.
