# PLAN — Datum ERP, foundation build (v2)

Task slug: `erp` · Workdir: `/Users/jroberts/Desktop/Internal Development/Tools/ERP` · Profile: production
Roster rev 6 · Plan author: fable · v1 2026-09-11 (shop PC, roster rev 4) · **v2 2026-09-12 (MacBook, roster rev 6)**

The v1 text is preserved at `_team/state/PLAN-v1-2026-09-11.md`.

---

## 0. What changed in v2, and why

Read this section first if you read v1.

1. **Drift fixed.** The task master flagged on 2026-09-11 23:02 that §1 still stated the two
   claims the ledger and audit decisions overturned, that §2 still called the spikes "in
   flight", and that §5 lacked `datum-print` and two document edges. All three are corrected
   below. The binding table is now the *amended* record, not the original.
2. **Wave 1 split for width.** v1 put the workspace, a complete `datum-core`, the test
   harness, the roles SQL and CI into one lane with a doubled deep audit. That is a serial
   bottleneck on the critical path of everything. v2 runs three code lanes with disjoint
   file ownership (`ws-skeleton`, `core` as a cross-family race, `harness`) alongside six
   documentation lanes.
3. **Wave 2 is slice-first.** v1 built all thirteen kernel crates and only then a vertical
   slice. The competitive review's warning was that this project dies by building breadth
   before anything works end to end. v2 makes the slice the target: the kernel batches are
   ordered by what the slice needs, and the four crates the slice does not need
   (`datum-esign`, `datum-documents`, `datum-print`, `datum-customfields`) move to Wave 2b,
   after the slice runs on real data. Nothing regulated is lost: those four are workflows
   and mechanisms, and the record properties they rely on (identity lifecycle, server time,
   audit trigger, version stamping, no deletes) are all in the batches that precede them.
4. **A second trait inversion.** `PostingSink` (from the plan audit) removes the
   statemachine/ledger cycle. v2 adds `SignatureGate` in `datum-core` so that a state
   transition that requires a signature asks a trait, not `datum-esign` directly. This
   removes the `statemachine → esign` edge, which is what lets the slice run a work order
   before the signature crate exists. `datum-esign` implements the trait in Wave 2b; the
   composition root wires it. Until then the gate is `NoSignatures`, which refuses any
   transition declared as requiring one, so nothing is silently unsigned.
5. **Build node.** Apple Silicon MacBook is the primary build node (roster design rule).
   Rust is pinned at 1.98.1, PostgreSQL 17 is installed from Homebrew, `just` and
   `sqlx-cli` are on the path. The NUC (Linux) and shop PC (Windows) are CI nodes only, and
   neither has PostgreSQL yet. See §11.
6. **Publication.** The owner authorised a public GitHub repository on 2026-09-12. The
   license (ADR 0006) and the repository name are the owner's calls and are being asked
   for. Until both land, the tree is mirrored to a **private** repository only. See §12.
7. **Multi-application posture made explicit.** See §1a. The owner's stated goal is a
   system that replaces their company's ERP *and* can be configured for other kinds of
   business. The module system was already designed for that; v2 makes the plain-shop
   profile an acceptance target rather than an implication.

## 1. What we are building

An open source, modular ERP for discrete manufacturing, with medical device manufacturing
as the first-class target and every regulated *workflow* delivered as an optional module
over a kernel whose *record properties* are compliance-aware from the first commit.

Read before doing anything: `docs/00-erp-primer.md` through `docs/04-module-catalog.md`
and every file in `docs/adr/`. Those are the governing design documents. This plan
implements them; it does not restate them. The four decision records in
`research/decisions/` are frozen contracts, and the audit slices in `research/audits/`
are the evidence behind them.

The decisions that bind every lane, **as amended**:

| Decision | Value | Source |
|---|---|---|
| Shape | Modular monolith, one binary | ADR 0001 |
| Backend | Rust, Axum, SQLx | ADR 0002 |
| Database | PostgreSQL only, no dialect abstraction. **Installed and lifecycle-managed by the operating system; Datum never owns a database process, on any OS, in any version.** | ADR 0003 as amended, `research/decisions/install-story.md` |
| Quantities and money | `Quantity<D>` with a sealed **dimension** as the type parameter and a runtime `UnitId`; `Money` is a separate type with a runtime `CurrencyId`; core never rounds | `research/decisions/core-quantity.md` (D1) |
| Ledger | Append-only postings. **Conservation holds per balance slice** — quantity per `(group, item, unit)`, value per `(group, currency)` — never as a scalar sum over a group. Five group kinds. Every withdrawal names its source postings and the allocation must reproduce both engines' numbers. | ADR 0004 as amended, `research/decisions/ledger-invariant.md` (D2) |
| Audit trail and signature | Kernel, not module. **Written by a row trigger attached automatically at `CREATE TABLE`, through a security-definer function.** The application role holds SELECT only on the audit table. Per-transaction hash chain anchored off the server; the honest claim is tamper *evidence*, never tamper *proof*. | ADR 0005 as amended, `research/decisions/audit-persistence.md` (D3, D4) |
| Tenancy | Single tenant, self-hosted; residency is an installation property | ADR 0008 |
| Interface | TypeScript, React, TanStack; one application, three interaction modes; archival documents server-rendered to PDF; Tauri is optional and never required | ADR 0009 |
| License | **Open — owner decision pending.** Recommendation AGPL-3.0-or-later with a Developer Certificate of Origin. Workspace carries `license = "UNLICENSED"` until it closes. | ADR 0006 |

### 1a. One kernel, many kinds of shop

The owner's goal is two things at once: a replacement for the ERP their own company runs,
and an open source system another company in another trade can configure for itself.
`docs/03-module-system.md` is the mechanism, and its first requirement is the one that
matters here: *a shop can run only what it needs; a bracket shop never sees a CAPA
screen.* What v2 adds is an acceptance target, so the claim is tested rather than assumed:

- **Two installation profiles ship from the first release**, expressed purely as the set
  of enabled modules and declarative configuration: `regulated-device` (the beachhead)
  and `plain-shop` (no regulated module enabled, no signature requirement on any
  transition, no validation manifest surfaced). The kernel's record properties are
  identical in both; they are invisible and cost nothing when unused.
- Wave 3's phase-end test runs the whole-program API test under **both** profiles.
- **No customer-specific code, ever** (invariant 18). Anything the owner's own company
  needs that another shop would not is configuration or a module, never a branch.

## 2. Current state (2026-09-12)

Written and complete: `docs/00` through `docs/04`; `docs/adr/0001` through `0009`
(`0003` Accepted as amended; `0006` open; the rest Proposed); `docs/01` and `docs/02`
reconciled against the decisions on 2026-09-11 (`_team/reports/doc-reconcile.md`); four
decision records and ten audit slices promoted into `research/`; the four mockups in
`design/`; `DESIGN.md`; `HANDOFF.md`.

The research spikes have **landed**: `spike-landscape`, `spike-regulatory`,
`spike-regulatory-udi`, `spike-governance`, `spike-probe`. Their reports are in
`_team/reports/` and their substance in `research/background/`. The two doc lanes gated
on them (`doc-regulatory`, `doc-landscape`) are no longer gated.

Not yet written: `docs/05` data model, `docs/06` regulatory, `docs/07` roadmap, `docs/08`
competitive landscape, `docs/10` API conventions, the repository files (README,
CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, LICENSE, .gitignore), and **all code**.

Repository: local `main`, three commits, no remote yet. Author identity is the owner's
Gmail, which is correct and stays. `_team/` is excluded via `.git/info/exclude`.

## 3. Wave structure

Four waves plus a slice. The split is dictated by real dependencies: every crate
depends on the workspace and on `datum-core`; lanes work in isolated worktrees where
they cannot see each other's output.

### Wave 1 — foundation and documentation (parallel, nine lanes)

`datum-core` is not a stub. It ships complete in this wave, built to the frozen D1
contract, because every Wave 2 lane imports it and a lane cannot write a meaningful test
against `todo!()`. Every other crate ships as a compiling stub whose public signatures are
real (`research/audits/slice-wave1-stubs.md` §4 and §5 are normative for those stubs).

| Lane | Owns (exclusive) | Kind | Audit |
|---|---|---|---|
| `ws-skeleton` | `Cargo.toml` (root), `rust-toolchain.toml`, `rustfmt.toml`, `.cargo/config.toml`, `justfile`, `.github/workflows/ci.yml`, `dev/compose.yml`, and `crates/<every crate except datum-core and datum-test>/**` as compiling stubs | build, long | deep |
| `core-r1`, `core-r2` | `crates/datum-core/**` — the complete primitive crate | **cross-family blind race, two attempts** (below the production cap of four; declared here). Grok master adjudicates on the acceptance criteria; the winner gets the deep audit. | deep |
| `harness` | `crates/datum-test/**`, `dev/sql/*.sql` (roles, grants), `.env.example` | build, short | deep |
| `doc-datamodel` | `docs/05-data-model.md` | doc | standard |
| `doc-api` | `docs/10-api-conventions.md` | doc | standard |
| `doc-repo` | `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, **`.gitignore` (whole file, including the Rust block)** | doc | standard |
| `doc-regulatory` | `docs/06-regulatory.md` | doc | deep |
| `doc-landscape` | `docs/08-competitive-landscape.md` | doc | standard |
| `doc-roadmap` | `docs/07-roadmap.md` | doc | standard |

Ownership rules that resolve the collisions the stub audit found:

- **`.gitignore` has one writer: `doc-repo`.** Its spec contains the Rust and SQLx
  entries verbatim (`/target`, `**/*.rs.bk`, `.env`, `*.pdb`; never `.sqlx/`, never
  `migrations/`). `ws-skeleton` does not write it.
- **The root `Cargo.toml` has one writer: `ws-skeleton`.** The canonical text is in
  `_team/specs/CONTRACT-workspace.md`. `core` and `harness` carry a *throwaway* root in
  their worktrees so they compile in isolation; the throwaway is discarded at
  integration and their crate manifests must inherit only keys the canonical root
  defines.
- `dev/compose.yml` belongs to `ws-skeleton`; `dev/sql/` belongs to `harness`.
- Every crate manifest uses `[lints] workspace = true` and `.workspace = true` for
  version, edition, rust-version, license, publish.
- Not one lane touches `_team/`, `docs/adr/`, or another lane's files. A lane that
  believes it must raises an escalation in its report and stops.

### Wave 2 — kernel, slice-first (batched; compile-parallel against stubs, done in order)

A crate cannot be finished against a sibling that still returns `Unimplemented`, so the
batches are dependency order, not caution. Each batch is one manifest; lanes within a
batch run in parallel.

| Batch | Crates | Why here |
|---|---|---|
| 2.1 | `datum-db` | Everything persists through it: pool, sealed transaction with transaction-local actor, migration runner, role model, version stamping, no-cascade and no-hard-delete lints. |
| 2.2 | `datum-audit` | Nothing else may create a table before the trigger attachment exists, or its writes go unaudited. Includes the hash chain. |
| 2.3 | `datum-identity`, `datum-numbering`, `datum-uom`, `datum-events` | Independent of each other. Identity reserves the separable signing credential (invariant 14) now, so Wave 2b adds no column to a table with history. |
| 2.4 | `datum-ledger` | **The gate.** Deep audit, doubled, rework race pre-declared. Wave 2 does not close until the property suite in §7 passes in commit mode with the canary armed. |
| 2.5 | `datum-statemachine`, `datum-jobs` | The work order needs states; background work needs a named service principal. Statemachine depends on `SignatureGate`, not on `datum-esign`. |
| 2.6 | `datum-module` (minimal composition root) | Composes every crate above; never a parallel lane. |

### Wave 2s — the vertical slice (the milestone that matters)

Items, locations, lots, the inventory ledger, one minimal work order, and a genealogy
trace, running end to end on real data through the HTTP API. This exercises the riskiest
architecture and it is the screen that demonstrates the product.

| Lane | Delivers |
|---|---|
| `mod-items` | Part master: number, revision, description, type, stocking unit, lifecycle status. |
| `mod-locations` | Warehouses, areas, bins, and the ledger's virtual locations (supplier, customer, scrap, adjustment, WIP). |
| `mod-lots` | Lot and serial identity with the generation constraint (invariant 9), tracked-entity indirection (10), package hierarchy (11), expiry with precision (12). |
| `mod-inventory` | Receipts, issues, moves, adjustments over the ledger; on-hand / allocated / available as rebuildable projections; status (available, quarantined, rejected, hold). |
| `mod-production-min` | Work order: create, release, issue material, complete, receive finished lot. A `TRANSFORMATION` group with allocation edges. **Not** the Phase 3 `production` module; a minimal surface the full module later subsumes. |
| `mod-genealogy` | Forward and backward trace over the ledger's consumption edges. Read-only. Returns the tree the mockup draws. |
| `server-slice` | `datum-server`: HTTP API + OpenAPI for exactly the slice, both installation profiles, headless test script. |

Acceptance for the slice, run headlessly against the API: receive a bar-stock lot under a
supplier lot; release a work order; issue the bar; complete and receive a finished lot;
trace forward from the mill heat and backward from the finished lot and get the same tree;
rebuild every projection from scratch and get the same balances; the ledger property suite
green with its canary case failing as designed.

### Wave 2b — the remaining kernel crates (after the slice runs)

`datum-esign` (implements `SignatureGate`), `datum-documents`, `datum-print`,
`datum-customfields`. Batched as 2b.1 `esign` + `customfields`, 2b.2 `documents`, 2b.3
`print`. Each is additive: new tables, new mechanisms, no change to any column that
already carries history.

### Wave 3 — interface (gated on the visual approval)

Shell and tokens, item master, shop floor terminal, genealogy trace, all against the
slice API, all under both installation profiles. Tauri is deferred per ADR 0009 until a
shop needs a serial gage or a direct label printer. Backend lanes never wait on this gate.

## 4. The UI gate

Four mockups are in `design/` and need a yes or a revise from the owner before any
interface lane starts. The planner's read, for the owner's convenience:

- **Shop floor terminal** — right, and the most important screen in the product. Large
  targets, one task, status words not just colours, monospace identifiers. Approve.
- **Genealogy trace** — the right idea and the right shape. The side-panel text is a
  generation artifact rather than a design and should be treated as placeholder.
  Approve the concept.
- **Item master** — clean, but far too sparse for a real office screen: a two-row BOM in
  a screen built for density. Push harder before anyone builds it: revision effectivity,
  where-used counts, inventory by location, supplier and cost panels.
- **Icon** — the datum symbol on a dark tile. Approve.

The approved look is recorded in `DESIGN.md` §10 once the owner signs off.

## 5. Crate contract (v2)

This is the integration contract. A lane may not rename a crate, change a public type
name, or add a dependency edge not listed here without an escalation.

```
crates/
  datum-core            no kernel dependencies (thiserror serde uuid rust_decimal)
  datum-test            no kernel dependencies (sqlx tokio)            Wave 1 harness; owned by Wave 1 for the life of the build
  datum-db              core
  datum-audit           core db
  datum-identity        core db audit
  datum-numbering       core db
  datum-uom             core db audit
  datum-events          core db
  datum-jobs            core db events
  datum-ledger          core db audit uom                              implements core::PostingSink
  datum-statemachine    core db audit identity                         depends on core::PostingSink + core::SignatureGate, never on ledger or esign
  datum-esign           core db audit identity                         implements core::SignatureGate            (Wave 2b)
  datum-customfields    core db audit                                                                             (Wave 2b)
  datum-documents       core db audit identity numbering statemachine                                             (Wave 2b)
  datum-print           core db audit documents esign                                                             (Wave 2b)
  datum-module          core db + all of the above (composition root; wires PostingSink and SignatureGate)
  datum-server          everything
modules/                (Wave 2s onward; each depends on datum-module's published interfaces only)
```

The graph is acyclic and that property is load-bearing. Two traits in `datum-core` keep it
that way:

- **`PostingSink`** — the interface through which a state transition contributes postings
  to the ledger group of the transaction it runs in. `datum-ledger` implements it.
  Events cannot substitute, because they are asynchronous and land in a different
  transaction, where the group could not balance.
- **`SignatureGate`** — the interface through which a transition declared as requiring
  a signature obtains one. `datum-esign` implements it in Wave 2b. Core ships
  `NoSignatures`, which refuses every signature-requiring transition with a typed error,
  so the slice can run work orders whose transitions do not require signatures while
  nothing that does can slip through unsigned.

`datum-core` holds primitives with no database dependency: identifier newtypes, `Actor`,
`Money`, `Quantity<D>`, `UnitRef<D>`, `AnyQuantity`, the residual types, the
`UnitConverter` trait, the two traits above, and the shared error types. The quantity
contract is `research/decisions/core-quantity.md` §2, frozen, reproduced by reference in
`_team/specs/SPEC-core.md`. Everything else depends on this crate, which is why it is
raced and deep-audited.

`datum-test` is the test harness: connects from `DATABASE_URL`, runs migrators in graph
order, provides **commit-mode** fixtures with per-test schema isolation and cleanup, and a
`postgres_available()` skip helper that becomes a hard failure under `DATUM_REQUIRE_PG=1`.
It is the thing that makes the ledger property tests real (§7).

## 6. Non-negotiable invariants

Every lane is accountable to these. An audit that finds a violation is a fail verdict
regardless of whether the lane's own acceptance criteria passed.

1. **No stored balances.** No column anywhere holds a running quantity or value that
   application code updates. Projections are explicitly named as caches and are
   rebuildable.
2. **Conservation holds per balance slice, not per group.** Quantity is conserved per
   `(group, item, unit)`; value is conserved per `(group, currency)`. A scalar sum over a
   group is meaningless because a group spans unlike items and unlike units. Quantity may
   cross an identity boundary only inside a group whose declared kind is
   `TRANSFORMATION`, and every quantity that crosses carries a value posting that does
   not, so the seam where matter may change identity is the seam where value may not
   disappear. Every withdrawal names the source postings it came from, and that
   allocation must independently reproduce both the quantity the movement engine stated
   and the money the valuation engine stated. Enforced by deferred constraint triggers,
   not by application code. The five group kinds are `MOVEMENT`, `ADJUSTMENT`,
   `TRANSFORMATION`, `VALUATION`, `REVERSAL`. See
   `research/decisions/ledger-invariant.md`.

   A group-level rule cannot catch an operator who scans 550 when the count is 500,
   because every derived number descends from the 550. That check belongs against the
   source document as a tolerance, and it is deliberately not a ledger invariant, because
   a genuine over-receipt must remain recordable.
3. **The audit trail is written by the database, and the application cannot write to
   it.** Entries come from a row trigger attached automatically at `CREATE TABLE`, in
   the same transaction as the change, never from application code. The application role
   holds select on the audit table and holds no insert, update, delete, or truncate;
   entries reach it only through the security-definer trigger function. Against a
   database superuser this is not a guarantee and is never described as one: tamper
   evidence is the per-transaction hash chain plus an anchor published off the server,
   verified off the box. See `research/decisions/audit-persistence.md`.
4. **Time is server-side**, read inside the trigger. No client-supplied timestamp is ever
   stored as the time of record. Every row of one transaction carries the same time of
   record; intra-transaction order is a separate column. The host clock is the source and
   the product says so; "server time" is never sold as trusted time.
5. **Every mutation has an attributable actor, or it does not happen.** The actor is
   supplied transaction-locally, never as session state, and is checked against the
   current transaction id so that a pooled connection cannot attribute a write to the
   previous operator. A write the database cannot attribute is refused, not recorded as
   unknown. Background work runs as a named service principal.
6. **No module reads another module's tables.** Published interfaces only.
7. **No `unsafe`** in any crate without a written justification in the escalation log.
8. **Every migration has a tested reverse.**

### 6a. The kernel rule, restated

The original test was that anything which cannot be retrofitted is kernel. The
regulatory research (`research/background/regulatory.md` section 1) produced a sharper
and more useful version, and it is adopted as the governing rule:

> **Regulated workflows are modules. Regulated record properties are kernel.**
> A module can add a process. A module cannot add a property to history.

Corrective action, supplier scorecards, internal audits, complaint handling, calibration
and barcode identification genuinely bolt on later. Anything that changes what a record
*is* does not, because a module cannot reach backward into records the kernel already
wrote.

### 6b. Further invariants, from the regulatory research

These cost almost nothing now and cannot be bought at any price later. Each is binding on
Wave 1 and Wave 2.

9. **Lot and serial identifiers are constrained at generation.** Uppercase letters,
   digits and hyphen only, twenty characters or fewer. Barcode standards cap these
   fields at twenty characters, 21 CFR 830.20(c) restricts the character set, and one
   issuing agency permits only letters and digits. Lots already etched on product in the
   field cannot be renumbered. The research calls this the single highest-value item it
   found, and it is one validation rule.
10. **The tracked entity is a lot or a unit within a lot, from the first posting.**
    Lot-only inventory with serialization added later changes the primary key of every
    downstream table. This costs one indirection now.
11. **Package hierarchy is kernel, not a barcode-module concern.** Each, inner, case,
    pallet, contained quantity and parent link are core inventory structure. Without it
    the ledger cannot express receiving two cases as forty-eight pieces, and adding it
    later changes the effective unit on every historical posting.
12. **Expiry carries a precision, never a bare date.** Barcode standards permit an
    unspecified day meaning end of month. A `DATE` column invents a day at write time
    that can never be recovered.
13. **Identities are never deleted and never reused.** Users deactivate. Usernames are
    never recycled. A user record outlives every record it signed, because reassigning an
    identifier retroactively makes old signatures ambiguous.
14. **The signing credential is separable from the login credential**, or single sign-on
    later leaves nothing to re-prompt for at signing time.
15. **A signature snapshots the signer's printed name as of signing.** Rendering it by
    joining to a live user table is wrong, because people change their names and the
    regulation asks for the name at the time of signing.
16. **No hard deletes anywhere, and no cascade deletes.** One `ON DELETE CASCADE`
    permanently removes the history of those rows, and it is found at inspection.
17. **Every record carries the application version and configuration version that
    produced it**, because the customer's change assessment depends on knowing which
    build wrote what.
18. **Customer-specific behaviour lives in declarative configuration, never in bespoke
    code shipped to one customer.** Shipping custom code moves that customer into a
    stricter validation category permanently, raising their cost on every future
    release. This looks like a product decision and is an architectural one.

Items 11, 12 and 18 add scope to the kernel that the original plan did not have. That is
the cost of having asked the question before building rather than after.

## 7. Testing obligation

Per lane, not negotiable, and audited. The original version of this section was a
slogan; the audit said so, and these are the acceptance criteria that replace it.

- Unit tests for the crate's own logic.
- Migration tests that run forward and backward against seeded data.
- No lane declares done on code that does not compile and does not pass its own tests.

**The ledger property tests, stated as criteria rather than as an aspiration.** A
generator produces sequences of whole business transactions, not random rows, drawn from
the twelve cases enumerated in `research/decisions/ledger-invariant.md`. The suite
must demonstrate all of the following, and a suite that cannot fail is not a suite:

1. Every legitimate sequence commits.
2. A transposed digit in any single posting is rejected, and the test names which
   predicate rejected it.
3. A dropped counterpart posting is rejected.
4. A group whose declared kind does not permit an identity boundary is rejected when it
   crosses one.
5. An allocation that does not reproduce the movement engine's quantity is rejected.
6. An allocation that does not reproduce the valuation engine's money is rejected.
7. Projections equal the ledger fold after every sequence, and after a rebuild from
   scratch.
8. Any balance is reconstructible at any past instant in the sequence.
9. A reversal restores the prior state without deleting anything.

**The trap that would make all of this silently pass.** The conventional Rust test
harness wraps each test in a transaction and rolls it back. Deferred constraint triggers
fire at commit. A rolled-back test therefore never fires the constraint under test, and
every one of the criteria above would report success while testing nothing. The ledger
suite commits against a real database and cleans up afterward, and one deliberately
failing case is kept permanently in the suite as a canary that the constraints are armed.

**The harness that makes this real** is `crates/datum-test` (Wave 1, `harness` lane): commit-mode fixtures, one schema per test, cleanup after commit, `postgres_available()` that skips locally and fails hard under `DATUM_REQUIRE_PG=1`, and a permanently failing canary that proves the deferred constraints are armed. No lane writes its own harness.

**This suite is also a deliverable to customers**, not only an internal artifact. Test
names are stable from the first release, because a regulated customer attaches this
output to their own installation qualification. Renaming a test later breaks their
evidence.

## 8. Design guidelines for the next agent

Recorded in `DESIGN.md` and binding on every lane that touches the interface: dark mode
by default, no emojis anywhere in the interface, consistent button alignment and
spacing, animation only where it is seamless, and a touch target floor for shop floor
screens. Real compute, meaning MRP, scheduling, and genealogy traversal, is
multi-threaded.

## 9. Risks

| Risk | Response |
|---|---|
| The ledger design is wrong and everything depends on it | This is an **engineering** risk, retired by Phase 1 inventory and lots running on real data, not by Wave 2 closing. Ledger built as the gate of Wave 2, deep and doubled audit, commit-mode property tests with a canary; then the slice. |
| Lanes invent divergent conventions in isolated worktrees | Wave 1 stubs fix names, edges, error types and lint configuration before any fan-out; the crate contract is a file, and the stub audit's normative sections are cited by path. |
| Scope explodes across the module catalog | Waves 1 and 2 are kernel only; the slice is six named modules with a named acceptance script; Phase 2 and later are out of scope for this build. |
| Doc lanes contradict the ADRs | Every doc lane names the ADRs and decisions it must conform to; audits check conformance, not prose quality alone. |
| The four deferred crates get retrofitted badly | Each is additive by construction (§3, Wave 2b). Identity reserves the signing credential now. `SignatureGate` refuses rather than skips until esign exists. |
| Rust contributor pool is thin (ADR 0002) | Not resolvable in this build. The public API and generated client are a Wave 2s and Wave 3 deliverable. |
| The build node changed mid-project | Everything is pinned in files (`rust-toolchain.toml`, compose Postgres version, roles SQL). No lane may depend on a tool that is not in the pin. |
| No shop has said it wants this | Not retirable by architecture. The slice is what gets put in front of one. |

## 10. Out of scope for this build

Everything in `docs/04-module-catalog.md` from Phase 2 onward, except the minimal work
order surface named in Wave 2s. The general ledger, per ADR 0007. Multi-tenancy, per ADR
0008. Runtime plugin loading, deferred per `docs/03-module-system.md` §5. **A bundled or
Datum-managed PostgreSQL lifecycle, on any OS**, per ADR 0003 as amended. Catch-weight
items. The Tauri shell, until ADR 0009's revisit condition fires.

## 11. Build environment and nodes

| Node | Role | State 2026-09-12 |
|---|---|---|
| MacBook (Apple Silicon) | **Primary build node.** All lanes, all worktrees, integration. | rustc 1.98.1 (pinned), cargo, PostgreSQL 17 (Homebrew, `/opt/homebrew/opt/postgresql@17`), `just`, `sqlx-cli`, node 26. |
| NUC (Linux, `ssh cnc`) | Linux CI node, single tenant. | No PostgreSQL, no docker. Provision before the first Linux CI round. |
| Shop PC (Windows) | Windows CI node, single tenant. | Rust present from the v1 run; PostgreSQL unknown. Provision before the first Windows CI round. |

Conventions every lane must follow:

- `DATABASE_URL=postgres://datum_migrate:datum@127.0.0.1:5432/datum_test` is the local
  default documented in `.env.example`; tests read it from the environment, never from a
  committed `.env`. `SQLX_OFFLINE=true` is set in `.cargo/config.toml`, never in `.env`.
- `dev/compose.yml` is the portable path for machines with a container runtime; on this
  MacBook the Homebrew service plays the same role and `dev/sql/` is applied to it. Both
  paths must produce the same roles and grants.
- **LOCAL CI FIRST.** GitHub Actions stay **off** on every repository until the workflow
  has passed on the MacBook, the NUC and the shop PC. The workflow file is written in
  Wave 1 and exercised locally; it is enabled remotely only by the orchestrator after a
  three-node green round.

## 12. Repository and publication

- The owner authorised a public GitHub repository on 2026-09-12.
- **Before the first public push:** the license decision (ADR 0006) and the repository
  name are the owner's, and are being asked for. A public repository is never renamed,
  made private, deleted, transferred or force-pushed afterwards, so the name is chosen
  once.
- Until then the tree is mirrored to a **private** repository, Actions off, as a backup
  remote (`sync-dev` pattern). Pushes are plain fast-forward only.
- `_team/` never enters git. The substantive research is already promoted into
  `research/`; anything else worth keeping is promoted the same way at integration.
- Once the license lands: `LICENSE`, `CONTRIBUTING.md` with the DCO or CLA choice, the
  header convention, and the copyright holder, all written by `doc-repo` on the amended
  ADR.

## 13. Lane conventions for this repository

- Worktrees live at `/Users/jroberts/Desktop/Internal Development/Tools/ERP-wt/wt-<lane>`;
  rework worktrees at `.../wt-<lane>-rw1`, `-rw2`. The exec-master creates them and copies
  the lane's spec in as `<worktree>/SPEC.md` (untracked).
- Specs are authored by the planner in `_team/specs/SPEC-<lane>.md`. The workspace
  contract every code lane must match is `_team/specs/CONTRACT-workspace.md`.
- A lane writes its report to `<worktree>/_team/reports/<lane>.md` before it finishes;
  the dispatcher mirrors it into the main tree.
- Commit budget is three per build lane; commits carry the `Lane:` trailer the git guards
  add. Read-only lane classes cannot commit. Nobody but the orchestrator pushes.
- Absolute paths only, in every prompt and every report.
