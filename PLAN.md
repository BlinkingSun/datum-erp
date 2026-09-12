# PLAN — Datum ERP, foundation build

Task slug: `erp` · Workdir: `C:\Users\fireb\Desktop\Shared\ERP` · Profile: production
Roster rev 4 · Plan author: fable · Date: 2026-09-11

---

## 1. What we are building

An open source, modular ERP for discrete manufacturing, with medical device
manufacturing as the first-class target and every regulated capability delivered as an
optional module over a compliance-aware kernel.

Read before doing anything: `docs/00-erp-primer.md` through `docs/04-module-catalog.md`
and every file in `docs/adr/`. Those are the governing design documents. This plan
implements them; it does not restate them.

The decisions that bind every lane:

| Decision | Value | Source |
|---|---|---|
| Shape | Modular monolith, one binary | ADR 0001 |
| Backend | Rust, Axum, SQLx | ADR 0002 |
| Database | PostgreSQL only, no dialect abstraction | ADR 0003 |
| Quantities and values | Derived from an append-only ledger, groups sum to zero | ADR 0004 |
| Audit and signature | Kernel, not module, produced by the persistence layer | ADR 0005 |
| Tenancy | Single tenant, self-hosted | ADR 0008 |

## 2. Current state

Written and complete: `docs/00` through `docs/04`, `docs/adr/README` and ADRs 0001
through 0007.

Not yet written: ADR 0008, `docs/05` data model, `docs/06` regulatory, `docs/07`
roadmap, `docs/08` competitive landscape, `docs/10` API conventions, repository files
(README, CONTRIBUTING, LICENSE, .gitignore), and all code.

Two research spikes are in flight and will land as `_team/reports/spike-landscape.md`
and `_team/reports/spike-regulatory.md`. Two doc lanes consume them and must not be
dispatched until they exist.

## 3. Wave structure

Three waves. The split is dictated by a real dependency, not by caution: every crate
depends on the workspace and on two foundation crates, and lanes work in isolated
worktrees where they cannot see each other's output.

### Wave 1 — foundation and documentation (parallel)

One code lane, because the workspace skeleton is the contract every later lane builds
into. Six documentation lanes that have no dependency on it and run alongside.

**`datum-core` is not a stub.** It ships complete in this wave, built to the frozen
contract in section 5, because every Wave 2 lane imports it and a lane cannot write a
meaningful test against `todo!()`. Every other crate ships as a compiling stub whose
public signatures are real.

| Lane | Owns (exclusive) | Provider | Audit |
|---|---|---|---|
| `workspace` | `Cargo.toml`, `rust-toolchain.toml`, `.cargo/`, `justfile`, `.github/workflows/`, `dev/`, the roles and grants SQL, the test harness, a **complete `datum-core`**, and a compiling stub for every other crate in section 5 | assign | deep, doubled |
| `doc-datamodel` | `docs/05-data-model.md` | assign | standard |
| `doc-roadmap` | `docs/07-roadmap.md` | assign | standard |
| `doc-api` | `docs/10-api-conventions.md` | assign | standard |
| `doc-repo` | `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `.gitignore` | assign | standard |
| `doc-adr` | `docs/adr/0008-single-tenant.md`, `docs/adr/0009-ui-stack.md` | assign | standard |
| `doc-regulatory` | `docs/06-regulatory.md` | assign | deep |
| `doc-landscape` | `docs/08-competitive-landscape.md` | assign | standard |

`doc-regulatory` and `doc-landscape` are **gated on their spike reports landing**.

### Wave 2 — kernel crates (batched, not one wide fan-out)

Every lane owns exactly one crate directory and nothing else. The original plan called
this thirteen parallel lanes. The audit showed that is compile-parallel against stubs
only, and that done-parallel is batched, because a crate cannot be finished against a
sibling that is still `todo!()`.

| Batch | Crates | Why this boundary |
|---|---|---|
| 1 | `datum-db` | Everything persists through it |
| 2 | `datum-audit` | Nothing else may land before the trail exists, or its writes go unaudited |
| 3 | `datum-identity`, `datum-numbering`, `datum-uom`, `datum-events` | Genuinely independent of each other |
| 4 | `datum-esign`, `datum-jobs`, `datum-customfields` | Depend on batch 3 |
| 5 | `datum-ledger` | **The gate.** Wave 2 does not close until it passes. |
| 6 | `datum-statemachine`, `datum-documents`, `datum-print` | Depend on the ledger and on each other's interfaces |
| 7 | `datum-module` | Composes every crate above; never a parallel lane |

Two corrections to the crate contract, both from the audit:

**The declared graph has a cycle in practice.** State machines must contribute postings
when a transition fires, and posting must mark its source document posted. That is
`statemachine → ledger` and `ledger → statemachine` at once. It is inverted with a
`PostingSink` trait defined in `datum-core`, which the ledger implements and the state
machine depends on. Events cannot substitute, because they are asynchronous and land in a
different transaction, where the group could not balance.

**Two missing edges and one missing crate.** `datum-documents` needs
`datum-statemachine` and `datum-numbering`, because an approval workflow is a state
machine. `datum-print` was in the kernel list and absent from the crate graph; it is now
a crate, and it depends on `datum-esign`, because a signature must render on a printout.

### Wave 3 — server, modules, and interface

Gated on the UI approval gate for anything user-facing. Covers `datum-server`, the
Phase 1 modules from `docs/04-module-catalog.md`, the web shell, and the Tauri
desktop shell.

The Tauri shell no longer supervises a database, since the install decision removed
that job. Its remaining purpose is the shop floor kiosk and a desktop launcher, and
ADR 0009 records whether that is worth a shell at all.

### The demoable-wedge problem

The competitive sweep raised an objection the wave structure has to answer. Waves 1 and 2
produce a kernel with nothing a shop can see, and the differentiating capability lives in
Phase 4 of `docs/04-module-catalog.md`, months away. Building a complete kernel before
anything is demonstrable is the failure mode that kills projects like this.

**Resolution: after Wave 2 closes, the next milestone is a vertical slice rather than
breadth.** Items, lots, the inventory ledger, one work order, and a genealogy trace that
runs end to end on real data. It exercises the riskiest architecture, it is the screen
that demonstrates the product, and it can be put in front of an actual shop. Breadth
across the module catalog comes after something works end to end, not before.

## 4. The UI gate

The system has a substantial user interface and the roster requires the Grok Imagine
approval gate before any of it is built. The grok master produces dark-mode concept
mockups and a candidate application icon; the user signs off on the mockup rather than
on shipped code; the approved look is recorded in `DESIGN.md` and Wave 3 builds to it.

Screens to mock, chosen because they cover the three distinct interaction modes in the
product:

1. **Item master detail** — a dense office-desktop record with revision history, the
   reference for every master-data screen.
2. **Shop floor terminal** — the touch-first operator view. Large targets, glove
   usable, high contrast, under fifteen seconds per interaction. The most important
   screen in the product.
3. **Genealogy trace** — the graph view for a lot or serial. The visual proof of the
   central bet, and the screen that sells the product.
4. **Application icon.**

Backend lanes do not wait on this gate.

## 5. Crate contract

This is the integration contract. A lane may not rename a crate, change a public type
name, or add a dependency edge not listed here without an escalation.

```
crates/
  datum-core            no dependencies
  datum-db              core
  datum-audit           core db
  datum-identity        core db audit
  datum-esign           core db audit identity
  datum-uom             core db audit
  datum-numbering       core db
  datum-events          core db
  datum-jobs            core db events
  datum-ledger          core db audit uom
  datum-statemachine    core db audit identity esign
  datum-documents       core db audit identity esign
  datum-customfields    core db audit
  datum-module          core db + all of the above
  datum-server          everything
```

The graph is acyclic and that property is load-bearing. Any lane that believes it needs
an edge not listed raises an escalation rather than adding one.

`datum-core` holds primitives with no database dependency: identifier newtypes, `Money`
with explicit scale, `Quantity<D: Dimension>`, `Actor`, and the shared error and result
types. Everything else depends on it, which means it is the crate most expensive to get
wrong and it gets a deep audit.

The quantity contract is decided in `_team/reports/DECISION-core-quantity.md` (decision
D1) and is frozen. In summary:

- **Dimension is a type parameter; unit of measure is not.** `D` is one of a sealed
  kernel set — `Count`, `Length`, `Mass`, `Time`, `Volume`, `Area` — so adding a length
  to a mass does not compile. Unit identity is a runtime `UnitId`, because units are
  customer-defined rows in a table and an unbounded set of units cannot be an unbounded
  set of Rust types. Adding two lengths in different units is a typed
  `QuantityError::UnitMismatch`, never an implicit conversion.
- A `Quantity<D>` can only be built on a `UnitRef<D>`, which is obtained by checking a
  `UnitId` against the unit master's dimension. A unit that does not belong to its
  dimension is therefore unrepresentable rather than merely discouraged.
- **`Money` is a separate type**, not a `Quantity`. It carries a runtime `CurrencyId`
  and no arithmetic in common with quantity, so adding money to a quantity does not
  compile and adding two currencies is a typed error.
- Numeric representation is `rust_decimal::Decimal`: quantities at scale ≤ 8
  (`numeric(24,8)`), money at scale ≤ 6 (`numeric(24,6)`).
- **Core never rounds.** Conversion returns a `Converted<D>` whose value cannot be read
  without either proving the residual is zero or binding the residual to a name. The
  conversion graph, item- and lot-specific factors, and rounding policy live in
  `datum-uom` behind a `UnitConverter` trait defined in core. An inventory conversion
  residual has exactly one home: a posting to `ADJUSTMENT` with reason `UOM_ROUNDING`,
  in the same group and the same unit.
- `Quantity<D>` is not serializable. `AnyQuantity { amount, unit, dimension }` is the
  single representation that crosses HTTP and is stored, with `Decimal` on the wire as a
  string. `datum-db` owns the `sqlx` mapping; core has no `sqlx` dependency.
- Catch-weight items, tracked in both eaches and mass, are **out of scope for v1**. The
  core types survive their arrival; the item master, allocation, costing, and MRP do not.

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
   `_team/reports/DECISION-ledger-invariant.md`.

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
   verified off the box. See `_team/reports/DECISION-audit-persistence.md`.
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
regulatory research (`_team/reports/spike-regulatory.md` section 1) produced a sharper
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
the twelve cases enumerated in `_team/reports/DECISION-ledger-invariant.md`. The suite
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
| Ledger design is wrong, and everything depends on it | Built first, deep audit, property tests. Wave 2 does not close until the ledger passes. |
| Lanes invent divergent conventions in isolated worktrees | The Wave 1 workspace lane ships compiling stubs that fix names, dependency edges, error types, and lint configuration before any fan-out. |
| Scope explodes across the module catalog | Waves 1 and 2 are kernel only. No module in the catalog is built until the kernel passes its gate. |
| Doc lanes contradict the ADRs | Every doc lane names the ADRs it must conform to; audits check conformance rather than prose quality alone. |
| Rust contributor pool is thin (ADR 0002) | Not resolvable in this build. The public API is the mitigation and it is a Wave 3 deliverable, not an afterthought. |

## 10. Out of scope for this build

Everything in `docs/04-module-catalog.md` from Phase 2 onward. The general ledger, per
ADR 0007. Multi-tenancy, per ADR 0008. Runtime plugin loading, deferred to Phase 3 per
`docs/03-module-system.md`.
