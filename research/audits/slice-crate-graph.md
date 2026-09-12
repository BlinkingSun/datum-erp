# Slice 3 — Crate dependency graph (PLAN.md §5)

Adversarial review. Workdir read-only except this report. Sources: PLAN.md entire;
`docs/02-architecture.md` kernel list; `docs/03-module-system.md` (hooks, state
machines, documents); `docs/04-module-catalog.md` Phase 0; ADRs 0001–0005, 0007.

**Verdict: REVISE before Wave 2 fan-out.** The declared graph is a DAG. It is not
the graph Wave 2 will be allowed to keep. The first demanded edge is
`datum-documents → datum-statemachine`. The hypothesized `datum-ledger →
datum-identity` is a false alarm if `Actor` stays a core newtype. The hypothesized
`datum-statemachine → datum-ledger` is the first *cycle* risk, not the first
*edge*, and must be inverted with a trait — events cannot substitute, because
events are async and hooks must post in the originating transaction.

---

## 1. Demanded-first edge

**`datum-documents → datum-statemachine`.** Add it now (amend PLAN §5).

Why this one, not the two the charter hypothesized:

1. Kernel documents are defined as carrying **approval workflow**.
   `docs/02-architecture.md` §2: "Controlled documents with revision history,
   approval workflow, and effectivity." That *is* a state machine: Draft → In
   Review → Approved → Obsolete, with per-transition permissions and signature
   requirements — the exact job of the kernel SM engine two paragraphs earlier
   ("Rather than reimplementing that per module, the kernel provides a
   declarative engine, which also means transitions are uniformly audited and
   uniformly signable").
2. `datum-documents` is declared to depend on `core db audit identity esign` and
   **not** on `datum-statemachine`. The documents lane therefore cannot implement
   its own acceptance criteria without either reinventing the SM engine
   (forbidden by the kernel rationale) or escalating for the missing edge.
   That escalation happens on day one of the documents lane, in Wave 2, against
   a stub that has no transition types.
3. Electronic signature is already a documents dependency, and ADR 0005 says
   "Modules declare which **transitions** require one. They do not implement
   signing." Documents already assumed the SM vocabulary; they just forgot the
   crate.

Close second, also missing, also demanded in the same lane:
**`datum-documents → datum-numbering`**. Gap-free sequences exist specifically
for document types (`docs/02` §2, Phase 0 "Numbering"). A documents crate that
inserts numbered revisions will call the allocator inside the same transaction
as the insert. That is a real edge. It is not first because a documents
implementer designs the entity with a `state` field before they wire a
sequence.

---

## 2. The two hypothesized edges, resolved

### 2.1 Will `datum-ledger` need `datum-identity`?

**No. Do not add the edge. Invert it: `Actor` is already in `datum-core`
(PLAN §5).** `posted_by` is attribution, not RBAC.

Evidence:

- Posting shape in `docs/02` §3: `posted_by` = "The actor, always present."
  No permission name, no role, no session.
- ADR 0004 never mentions identity, RBAC, or authorization. It constrains
  *quantities*: no stored balances, groups sum to zero, corrections are
  reversing postings. Cost layers "fall out of the same structure."
- ADR 0005: "Every action has an attributable actor. Background jobs run as a
  named service principal." That is a presence-and-attribution rule. It is
  satisfied by storing a core `Actor` newtype. The same ADR puts audit
  production in the persistence layer, not in an identity lookup at post time.
- `docs/02` §8: "Role-based access control at the level of **permission, not
  table**. Permissions are declared by modules and composed into roles."
  Authorization lives at the command/API boundary, which is a module or
  `datum-server` concern. A hook contributing a posting (`docs/03` §3.2) is
  already inside a trusted transaction; re-checking RBAC inside `ledger.post`
  would be a layering bug and would force every sink call to carry a session.

If an implementer types `posted_by: identity::UserId` and joins users at
write time, the edge appears. That is the implementer being wrong, not the
docs implying it. Freeze in the crate contract: `posted_by: datum_core::Actor`.
Existence of that actor (user vs service principal, enabled vs disabled) is a
database FK enforced at the composition root, or a `trait ActorDirectory` in
core implemented by identity. It is not a Rust dependency from ledger to
identity.

Adding `ledger → identity` would **not even create a cycle** (identity does
not depend on ledger). It would just drag sessions, RBAC, and OIDC into the
crate PLAN §9 says everything else is waiting on. Refuse it.

### 2.2 Will `datum-statemachine` need `datum-ledger`?

**The type dependency will be demanded. Adding the crate edge is how you
get a cycle. Invert with a trait in `datum-core`. Do not use events.**

`docs/03` §3.2: a hook "runs synchronously inside the originating transaction and
may veto" and **"may contribute postings to the same ledger group."**

That sentence forces a posting sink to be visible to the hook invocation
path. If `HookContext` lives in `datum-statemachine` and takes
`datum_ledger::PostingDraft`, the crate edge `statemachine → ledger` is
immediate.

That edge, **by itself**, is still a DAG (ledger does not currently depend on
SM). The **cycle** appears the moment ledger also advances document state
("posting this receipt marks the PO line Received", "source document status
= Posted"). ADR 0001's motivating transaction does both in one graph:
consume material, post cost, **advance a state machine**, write audit, maybe
sign. If SM calls ledger and ledger calls SM, the graph dies.

Correct inversion (keep acyclic, keep hooks transactional):

```
trait PostingSink { fn contribute(&mut self, draft: PostingDraft) -> Result<()>; }
```

lives in `datum-core` (or a tiny `datum-ledger-api` crate that ledger
implements and SM depends on — same idea, extra crate). The **command
handler** (module / server) opens the ledger group **and** the SM
transition, and passes `&mut dyn PostingSink` into the transition. SM does
not open groups. Ledger does not call `transition()`.

**Events are the wrong inversion.** `docs/03` §3.1: events are
"Asynchronous by default, so a subscriber cannot slow down or fail the
originating transaction." A hook that must contribute to **the same ledger
group** cannot wait for an at-least-once bus. Using events here breaks the
zero-sum group (the contributing postings would be a later transaction, so
either the original group is incomplete or the hook's postings are a
second group that cannot see the first). PLAN §6 invariant 2 (groups sum to
zero, deferred constraint at commit) dies with them.

**Decision for this edge: invert with a trait in core. Do not add
`statemachine → ledger`. Do not "keep acyclic via events."**

---

## 3. Kernel crate count, "Thirteen lanes", off-by-one

### Count of crates in PLAN §5

Fifteen directories:

| # | Crate | Declared deps |
|---|---|---|
| 1 | `datum-core` | none |
| 2 | `datum-db` | core |
| 3 | `datum-audit` | core db |
| 4 | `datum-identity` | core db audit |
| 5 | `datum-esign` | core db audit identity |
| 6 | `datum-uom` | core db audit |
| 7 | `datum-numbering` | core db |
| 8 | `datum-events` | core db |
| 9 | `datum-jobs` | core db events |
| 10 | `datum-ledger` | core db audit uom |
| 11 | `datum-statemachine` | core db audit identity esign |
| 12 | `datum-documents` | core db audit identity esign |
| 13 | `datum-customfields` | core db audit |
| 14 | `datum-module` | core db + all of the above |
| 15 | `datum-server` | everything |

Kernel crates (not the binary): **14**. Binary: `datum-server`, owned by
Wave 3 (PLAN §3).

### Wave 2's thirteen lanes, listed

PLAN §3 Wave 2: "Every lane owns exactly one crate directory and nothing else.
Thirteen lanes, all parallel, all branching from a merged Wave 1."

PLAN never names them. The number is the Phase 0 component count, cargo-culted.

**Phase 0 (`docs/04-module-catalog.md`) — thirteen components:**

1. Identity, authentication, RBAC
2. Audit trail
3. Electronic signature
4. Ledger engine
5. State machine engine
6. Documents and revisions
7. Numbering
8. Units of measure
9. Custom fields
10. Event bus
11. Background jobs
12. Reporting and print
13. Module registry

**`docs/02` kernel box — twelve named capabilities**, same list minus the
catalog's explicit "module registry" (module registry is implied by §4 and
by `docs/03` §6, and is a Phase 0 row):

identity + RBAC, audit trail, electronic signature, ledger engine, state
machines, documents + revisions, numbering, units of measure, background
jobs, event bus, custom fields, reporting + print.

### Resolve the off-by-one

| Count | What it is | Problem |
|---|---|---|
| 15 | PLAN §5 crate directories | includes `datum-server` (Wave 3) |
| 14 | kernel crates (§5 minus server) | Wave 2 said 13 |
| 13 | Phase 0 components / Wave 2 claim | does not match §5 |
| 13 charitable | §5 minus server minus `datum-module` | module then has **no wave**; Wave 3 does not name it |
| 12 | `docs/02` kernel box | omits module registry |

**The number 13 is Phase 0's component count, not a crate-lane list.** It
does not include `datum-core` or `datum-db` (substrate, not named as Phase 0
components). It does include **reporting + print**, which §5 does not have as
a crate. It includes **module registry**, which §5 has as `datum-module` but
which cannot be a parallel lane (see §8).

Charitable reading of "13 parallel Wave 2 lanes":

1. `datum-core`
2. `datum-db`
3. `datum-audit`
4. `datum-identity`
5. `datum-esign`
6. `datum-uom`
7. `datum-numbering`
8. `datum-events`
9. `datum-jobs`
10. `datum-ledger`
11. `datum-statemachine`
12. `datum-documents`
13. `datum-customfields`

Then `datum-module` is an unlisted Wave 2.5 serial lane, `datum-server` is
Wave 3, and **reporting + print is simply dropped.**

That is the least-bad parse, and PLAN does not say it. **Amend PLAN §3 to
name every Wave 2 lane.** Until it does, "thirteen lanes, all parallel" is
false in two ways: the count is unreconciled, and they are not all parallel.

---

## 4. Missing kernel crates / capabilities

Present in `docs/02` §1–2 and Phase 0, **absent from PLAN §5:**

| Capability | In kernel list? | In crate graph? | Call |
|---|---|---|---|
| reporting + print | yes (`docs/02` box; Phase 0 size M) | **no crate** | Missing. Kernel-rule in `docs/02` §2: if it cannot be retrofitted, it is kernel. Modules that each roll their own PDF will not archive identically and will not share travelers/labels/certificates. Add `datum-print` (core, db, audit, documents) or explicitly demote it with a new ADR that says print *can* be retrofitted. Silence is a PLAN bug. |
| event bus | yes | `datum-events` | Present |
| background jobs | yes | `datum-jobs` | Present |
| module registry | Phase 0; `docs/03` §6 | `datum-module` | Present as a crate, but placed in the wrong wave shape (depends on all) |

`datum-core` and `datum-db` are extra relative to Phase 0. That is correct.
They are substrate. They still need Wave 2 *implementation* lanes; Wave 1 only
ships stubs (PLAN §3, §9).

No other kernel box item is missing as a name. Print is the hole.

---

## 5. Hunt results for the remaining demanded edges

### 5.1 Numbering inside a posting transaction

Fine as a service crate. **Neither ledger nor SM should depend on it.**

Gap-free + transactional: PostgreSQL `SEQUENCE` does not roll back, so a
failed receipt would leave a hole — exactly the regulated question Phase 0
says you must not have to answer. Numbering must be a **table-backed
allocator** (`SELECT … FOR UPDATE` on `document_type`, increment, same
transaction as the document insert / posting group). That is an API shape:
`Numbering::next(&mut txn, series) -> DocumentNumber`. Callers pass the
sqlx transaction that ledger also uses.

Who calls it:

- Documents crate, on create/revise → **add `documents → numbering` now**.
- Inventory / receiving / production modules (Wave 3), not kernel ledger.
- SM engine should not allocate numbers. A work order number is the
  production module's document, not a property of "being a state machine."

Ledger postings already have `id` and `group_id`. They do not need
gap-free human numbers. Source documents do.

**Decision: add `documents → numbering`. Do not add `ledger → numbering`.
Do not add `statemachine → numbering`.** Composition (module/server) is
what allocates a number *and* posts *and* transitions in one txn.

### 5.2 `datum-jobs` → `datum-identity`?

ADR 0005 / PLAN invariant 5: jobs "run as a named service principal."
Declared deps: `core db events`. No identity.

**Do not add the edge.** A service principal *is* an `Actor`. Identity
provisions the principal at install (an identity mutation, audited). The
jobs runtime is constructed with `Actor` (config / installed-modules seed).
Same inversion as ledger's `posted_by`.

If jobs start looking up users, hashing passwords, or checking RBAC, the
edge will be demanded and should be refused. The job payload may include
"the user who requested this run" as an `Actor` for audit context, still
without importing identity.

### 5.3 `datum-uom` vs items (cycle across the kernel/module line)

`docs/02` §2: "Item-specific conversions with explicit precision and rounding
rules. Kernel-level because a conversion bug in a module silently corrupts
inventory everywhere."

Phase 0: "Item-specific conversion." Phase 1 `items`: the part master carries
units of measure. If kernel UoM tables have `item_id` and know the items
schema, then either UoM is secretly an items module, or items cannot be a
module that depends on kernel.

**Keep acyclic: UoM is an engine plus conversion tables as data.** Universal
conversions (in ↔ mm, and rounding/precision) live in kernel tables keyed by
`(from_unit, to_unit)`, not by item. Item-specific conversions (1 bar of
heat X = Y lb; stocking vs purchasing UoM) are **data supplied by the
items module** through a kernel interface (`ConversionTable` / dimension
key). `datum-uom` never names an item.

Declared `uom` deps (`core db audit`) are fine. **Do not add `uom → items`.
That is a cycle** (items depends on kernel, which includes uom).

If Wave 2 UoM authors put `item_id` on kernel tables, they have not created
a Rust cycle yet, but they have created a disable-the-items-module hole and
a Phase 1 rewrite. Call that out in the UoM lane charter: no item_id in
this crate.

### 5.4 Cost ledger vs inventory ledger

**One crate. Discriminator column/enum `ledger` ∈ {inventory, cost, labor}.**
Not two crates. Not a GL.

- `docs/02` §3 posting field `ledger`: "Which ledger: inventory, cost, labor."
- ADR 0004: "Cost layers for FIFO and average costing fall out of the same
  structure rather than needing their own." Alternative "Ledger for inventory
  only, mutable elsewhere" was **rejected**.
- Phase 1 `valuation`: "Cost layers on the inventory ledger." Module, not
  kernel crate. Same posting machinery, extra dimensions (cost element, layer).
- ADR 0007: do **not** build a general ledger. Operational cost/labor ledgers
  are not a GL. `gl-export` is Phase 7. Do not add `datum-gl`.

Two crates would duplicate group identity, deferred zero-sum, projections,
rebuild, and property tests. Valuation sitting on a second crate would
violate ADR 0004's "same structure" consequence.

### 5.5 Numbering / events / jobs missing `audit`

PLAN invariant 3+5 and ADR 0005: every mutation is attributable and audit is
produced by persistence. Yet `datum-numbering`, `datum-events`, `datum-jobs`
do not depend on `datum-audit`.

Two consistent stories, PLAN tells neither:

1. **`datum-db` always audits** (persistence layer = db crate). Then listing
   `audit` on identity/ledger/uom/SM/documents/customfields is for *query*
   APIs or `Audited` traits, and numbering's omission is fine. Then say so.
2. **Callers must depend on audit to write.** Then numbering (gap-free
   regulated sequences!) omitting audit is a compliance hole. **Add
   `numbering → audit` now.** Jobs queue mutations and event-store appends
   are weaker cases; still probably audited.

This reviewer assumes (1) is the intent of ADR 0005 and (2) is what the
dep list accidentally implies. **Amend PLAN: one sentence on whether audit
is a db-layer effect or a crate you must take a dependency on to mutate.**

---

## 6. Cycle risks (the ones that actually close a loop)

Declared graph: **acyclic**. Topological order:

```
core
  db
    numbering, events          (no audit in the list)
    audit
      customfields, uom, identity
        jobs  (via events, not identity)
        esign, ledger
          statemachine, documents
            module
              server
```

**Practice cycles, ranked:**

| Cycle | How it forms | Prevention |
|---|---|---|
| **SM ↔ ledger** | Hook context takes ledger types **and** posting completion transitions source documents | Trait `PostingSink` in core; handler opens group + transition. **First cycle, if any.** |
| **uom ↔ items** | Kernel conversion rows keyed by item_id; items module depends on kernel | Conversion tables as data; no item_id in `datum-uom` |
| **documents ↔ SM** | Documents depend on SM (needed) **and** SM special-cases document rows | SM stays generic (machine id + record id + Actor). Documents *use* SM, SM does not import documents. Adding `documents → SM` is **not** a cycle. |
| **\* → module → \*** | A kernel crate importing the registry (e.g. ledger asking "is valuation enabled?") | Module registry is a consumer of kernel crates, never a dependency of them. Feature flags at server composition. |
| ledger → identity | **Not a cycle** (no reverse path). Still refuse; see §2.1 | `Actor` in core |

Events as a cycle breaker: **allowed only for reactions that must not see
the originating transaction** (`docs/03` §3.1). Forbidden for hooks,
zero-sum groups, signatures bound to the record version just written, and
audit rows that must commit with the mutation (ADR 0005: "Writing a record
produces its audit entry as part of the same transaction").

---

## 7. `datum-module` is not a Wave 2 parallel lane

Declared: `datum-module` depends on **all** kernel crates. Wave 2: every
lane parallel from Wave 1 stubs.

A registry that compiles against stubs can parse `module.toml`, write
`installed_modules`, and flip enable flags. It **cannot** honestly register
state machines, permissions, routes, events, signature requirements, or
hooks until those crates' real public APIs exist. If the module lane ships
in parallel, it will freeze a facade against stub signatures and then
break when fourteen other crates land. PLAN §9's "stubs fix names" does not
reach "stubs fix the module host."

**Call: `datum-module` is serial Wave 2.5 (after the other kernel crates
merge), or it is split:**

- `datum-module` (thin): manifest schema, installed-modules table, config
  manifest hash, enable/disable. Deps: `core db audit` only. **Can** be
  parallel.
- Host / composition: lives in `datum-server` (Wave 3). That is the thing
  that must depend on everyone.

PLAN §3 Wave 3 names `datum-server`, Phase 1 modules, web shell, Tauri. It
does not name `datum-module`. Today the crate is homeless if you take the
"13 = minus server minus module" reading, and wrongly parallel if you don't.

---

## 8. PLAN amendments (required)

1. **Name the Wave 2 lanes.** Stop saying "Thirteen lanes." List them.
   Recommended: the 13 charitable crates in §3, plus an explicit Wave 2.5
   `datum-module`, plus Wave 3 `datum-server`. If print stays kernel, add
   `datum-print` and say which wave.
2. **Add `datum-documents → datum-statemachine`.** First demanded edge.
3. **Add `datum-documents → datum-numbering`.**
4. **Do not add `datum-ledger → datum-identity`.** Freeze `Actor` in core;
   state that RBAC is not checked inside `ledger.post`.
5. **Do not add `datum-statemachine → datum-ledger`.** Put `PostingSink`
   (and `PostingDraft` with core-only types: `Actor`, `Quantity`, dimension
   newtypes) in `datum-core`. Document that events cannot carry hook
   postings.
6. **Do not add `datum-jobs → datum-identity`.** Construct jobs with `Actor`.
7. **Do not add `datum-uom → items`.** Charter: no `item_id` in kernel UoM.
8. **One ledger crate**, enum `inventory | cost | labor`. Valuation is Phase 1
   module. No `datum-gl` (ADR 0007).
9. **`datum-module`:** split thin registry vs server host, **or** move the
   whole crate to serial after kernel crates. It cannot be parallel-and-real.
10. **Reporting + print:** add `datum-print` to §5 or write the retrofit
    exception. `docs/02` currently forbids the exception.
11. **Audit dependency rule:** one sentence. Either db auto-audits (and
    numbering's missing audit dep is OK) or numbering/events/jobs must take
    `datum-audit`.
12. **Wave 2 is not "all parallel" even after stubs**, except for compilation.
    Real tests on ledger/uom/SM require a non-stub `datum-core` (and ledger
    requires non-stub uom). See serial batches below. Slice 4 owns stub
    depth; this slice owns the lie that fan-out equals independent completion.

---

## 9. EXECUTOR / SPLIT / TIER per crate

Team law: Claude does not occupy a build lane. EXECUTOR is `cursor` or
`grok`. Anything that freezes a type in `datum-core` or a posting invariant
is `cursor` (types/invariants) with an opus **DECISION** if the slice-5
Quantity design or the PostingSink placement is still open when the lane
starts — decision, not a build.

### Deep (as specified, plus documents)

#### `datum-core` — TIER **deep** (PLAN already). EXECUTOR **cursor**. SPLIT **serial**.

Most expensive crate to get wrong (PLAN §5). Holds `Actor`, `Money`,
`Quantity<U>`, ids, errors, and — after this review — `PostingSink` /
`PostingDraft` dimension newtypes.

Do not split into multiple crates. Internal file split is fine (`ids`,
`money`, `qty`, `actor`, `error`, `posting_sink`) but it ships as one
versioned API. **Serial before every other real (non-stub) kernel test.**
Wave 2 "parallel against stubs" may compile; it cannot validate Money scale
or Quantity if those are still `todo!()`.

If slice 5 concludes runtime units kill `Quantity<U>`, that decision lands
**here** before uom and ledger write a line.

#### `datum-db` — TIER **deep**. EXECUTOR **cursor**. SPLIT **serial with audit, after core**.

Pool, transactions, migration runner, application role vs migration role,
grant model that makes audit append-only (ADR 0003, 0005). The persistence
interception story (slice 2: SQLx has no UoW interceptor) lives here or in
audit; do not let both lanes invent it.

Split internally: (1) pool + txn handle modules can hold, (2) migrations
forward/back, (3) roles/grants. One crate, one lane. Serial after real core;
audit cannot be tested without it.

#### `datum-audit` — TIER **deep**. EXECUTOR **cursor**. SPLIT **serial after db**.

Append-only grants, server time, produced-by-persistence. Depends on slice
2's answer (triggers + application context vs repository wrapper vs both).
Query API for investigators is secondary; the write guarantee is the
product.

Do not split from db until slice 2 says the interceptor is not *in* db.

#### `datum-ledger` — TIER **deep**. EXECUTOR **cursor**. SPLIT **internal only; crate is serial after core+db+audit+uom**.

Highest-value tests in the project (PLAN §7). One crate, three ledger
discriminants. Internal split: schema + deferred constraint, posting API,
projections/rebuild, property-test generator (slice 6). **Do not split cost
vs inventory into crates.**

Must not take identity. Must not take SM. Must not take numbering.

Wave 2 does not close until this crate passes (PLAN §9). That already
contradicts "all parallel." Treat ledger as the Wave 2 gate, not as one of
thirteen equal lanes.

#### `datum-uom` — TIER **deep**. EXECUTOR **cursor**. SPLIT **serial after core (Quantity decision), parallel with identity/customfields once core+db+audit exist**.

Engine + rounding + universal conversion tables as data. Boundary: no
items module types. Item-specific tables are a later interface, not a kernel
row. Wrong here silently corrupts every posting.

#### `datum-statemachine` — TIER **deep**. EXECUTOR **cursor**. SPLIT **serial after identity+esign; parallel with documents only after the new edge is in the contract**.

Declarative states, transitions, permission names (strings declared by
modules, not identity types), signature requirements (calls esign, does not
implement it), hook invocation with time budget (`docs/03` §3.2, ADR 0001
consequences).

**Landmine: HookContext.** Must take `dyn PostingSink`, not ledger types.
Internal split: declaration schema vs transition engine vs hook runner.

#### `datum-identity` — TIER **deep**. EXECUTOR **cursor**. SPLIT **authn vs RBAC vs service principals, one crate**.

Users, sessions, Argon2id, optional OIDC, permissions declared by modules,
service principals for jobs. Depends on audit so creating a user is
audited. Does not depend on ledger, SM, or jobs.

Internal split is three files, not three lanes: (1) principals + sessions,
(2) RBAC composition, (3) OIDC optional feature. Serial after audit.

#### `datum-esign` — TIER **deep**. EXECUTOR **cursor**. SPLIT **serial after identity**.

Hash of exact record version, meaning, server time, re-auth (21 CFR
11.200, `docs/02` §8). Modules declare; this crate implements. Bound to
identity for "who signed" (full user, not just Actor) because re-auth
needs credentials. That is why the declared `esign → identity` edge is
correct, unlike ledger.

Do not merge into SM. SM *requires* a signature; esign *performs* it.

#### `datum-module` — TIER **deep**. EXECUTOR **cursor**. SPLIT **serial Wave 2.5, or split thin registry (parallel, standard) vs host in server**.

Configuration manifest is a validated-configuration artifact (`docs/03`
§8). Getting install/enable/disable/upgrade wrong is a change-control event
in the customer's QMS. Deep is justified.

Cannot be a parallel Wave 2 implementation lane against stubs unless the
lane is explicitly the thin registry only.

### Standard (unless noted)

#### `datum-numbering` — TIER **standard**, with a dedicated audit check on gap-free-under-rollback. EXECUTOR **grok**. SPLIT **parallel after core+db**.

Table allocator, not `SEQUENCE`. Transactional `next(&mut txn, series)`.
If audit is not automatic in db, add audit dep (amend). Parallel with
events.

#### `datum-events` — TIER **standard**. EXECUTOR **grok**. SPLIT **parallel after core+db**.

Typed, at-least-once, idempotent subscribers (`docs/03` §3.1). Must not
grow a sync-hook API; that is SM. Parallel with numbering.

#### `datum-jobs` — TIER **standard**. EXECUTOR **grok**. SPLIT **serial after events; parallel with identity (no edge)**.

Durable queue, named `Actor`. Does not import identity. MRP/genealogy
"runs as a job" is Wave 3 callers.

#### `datum-documents` — TIER **deep** (disagree with "standard otherwise"). EXECUTOR **cursor**. SPLIT **serial after SM + numbering + esign**.

Disagree with the default: this crate *is* the controlled-record primitive
BOM/routing/DHR/DMR will sit on (`docs/02` §2, Phase 4 `doc-control` "Built
on the kernel document primitive"). Effectivity ranges (ADR 0003 exclusion
constraints), revision immutability, approval via SM, signatures on
versions. Standard audit would miss the missing SM/numbering edges.

Once PLAN adds the two edges, the lane is serial after those crates' APIs
exist. Against stubs it can only compile.

#### `datum-customfields` — TIER **standard**. EXECUTOR **grok**. SPLIT **parallel after audit**.

Typed, validated, audited like native fields (`docs/03` §3.3). Must use
audit; must not be a JSON blob. Parallel with uom/identity. Bump to deep
only if the persistence-audit interceptor (slice 2) cannot see dynamic
columns — then it is an audit-integrity crate and belongs on cursor.

#### `datum-print` (missing) — TIER **standard** once added. EXECUTOR **grok**. SPLIT **serial after documents; Wave 2.5 or Wave 3 kernel, not a module**.

Server-rendered PDF (`docs/02` §5). Ordinary infrastructure, but listed as
kernel. Do not let Phase 1 inventory invent label PDFs.

#### `datum-server` — TIER **deep** (Wave 3, composition root). EXECUTOR **cursor**. SPLIT **serial after kernel + module host**.

Wires every crate, opens transactions that SM+ledger+numbering share,
registers module routes. Out of Wave 2. Mentioned because this is where
the inverted traits get their impls.

---

## 10. Serial vs parallel after stubs

Wave 1 stubs make **compile-parallel** possible. They do not make
**done-parallel** possible. PLAN §3 "Thirteen lanes, all parallel" is
true only as git-worktree isolation, and false as an integration story.

Recommended batches once Wave 1 has merged. "Parallel" means real
implementation, not stub-compile.

| Batch | Crates | Why serial relative to previous |
|---|---|---|
| **0 stubs** | all §5 crates as compiling stubs | Wave 1 `workspace` lane. Names, deps, error types. Not Quantity semantics. |
| **1** | `datum-core` | Everything's types. One lane. No parallel partner. |
| **2** | `datum-db` | Needs real core. Audit/ledger cannot test without a pool/txn/migration story. |
| **3** | `datum-audit` **and** `datum-numbering` **and** `datum-events` | audit after db; numbering/events only need core+db and can run beside audit if db auto-audits. If numbering must take audit, numbering slips to batch 4. |
| **4** | `datum-identity`, `datum-uom`, `datum-customfields`, `datum-jobs` | identity after audit; uom after core Quantity decision + audit; jobs after events; jobs ∥ identity (no edge). |
| **5** | `datum-esign`, `datum-ledger` | esign after identity; ledger after uom+audit. **Ledger is the Wave 2 gate.** |
| **6** | `datum-statemachine`, then `datum-documents` | SM after identity+esign. Documents **after** SM + numbering (new edges). Not parallel with each other once the edge is honest. Against *stubs* they can compile in parallel; they cannot integrate. |
| **7** | `datum-module` (or thin registry earlier in 4, host here) | Depends on the real APIs of 1–6. |
| **8** | `datum-print` if added | After documents. |
| **Wave 3** | `datum-server`, Phase 1 modules, UI | Composition root. UI gate is separate (PLAN §4). |

**Stub-parallel (what PLAN actually enables):** batches 1–6 all open on
day one of Wave 2, compiling against Wave 1 stubs, **except**
`datum-module` (and documents' approval workflow, which will escalate).
Expect rework when core's `Actor`/`Quantity`/`PostingSink` stop being
stubs. That rework is why core, db, audit, uom, ledger, SM, identity,
esign, module (and documents) are deep.

**Do not** pretend `datum-module` is batch-3-parallel unless it is the thin
registry with deps reduced to `core db audit`.

---

## 11. Answers, one screen

| Question | Answer |
|---|---|
| Graph acyclic as declared? | Yes. |
| Acyclic in practice? | Yes **if** PostingSink lives in core, Actor lives in core, UoM takes conversion tables as data, and module/host does not get imported downward. **No** if SM takes ledger types *and* ledger transitions states. |
| First demanded edge | **`datum-documents → datum-statemachine`** (approval workflow). Add now. |
| First cycle if they slip | **SM → ledger + ledger → SM**. Invert SM→ledger; never add ledger→SM. |
| `ledger → identity`? | **No.** ADR 0004/0005 imply `Actor` attribution, not RBAC-at-post. |
| `statemachine → ledger`? | Demanded as a *type*, not as a crate dep. Trait in core. Events would break hooks. |
| `jobs → identity`? | **No.** `Actor` at construction. |
| Numbering in a posting txn? | Composition root / modules call both; table allocator on shared txn. Add `documents → numbering`. |
| UoM vs items? | Engine + data. Item-specific tables later, in the items **module**. No cycle. |
| Cost vs inventory ledger? | **One crate**, enum inventory/cost/labor. Valuation is Phase 1. No GL (ADR 0007). |
| Kernel crate count | **14** in §5 excluding server; Phase 0 has **13** components; Wave 2 says **13** lanes and names none. |
| Off-by-one | "13" is Phase 0, not §5. §5 has 15 including server. Print is in Phase 0 and not in §5. Module is in §5 and cannot be parallel. |
| Missing from graph | **reporting + print**. Event bus, jobs, module registry are present as crates. |
| `datum-module` parallel? | **No**, unless reduced to a thin registry. Call it out; serial 2.5 or split. |

---

*End of slice 3.*
