# Build Roadmap

*What gets built in what order, what each step proves, and what a shop can actually
use at each point. Nothing in this document is scheduled with dates: the build is
parallel lanes on a single critical path, and the honest unit of progress is a gate that
passes, not a calendar week.*

**Conforms to:** ADR 0001, 0004, 0005, 0006, 0007, 0008, 0009; `PLAN.md` v2 §1a and §3;
DECISION D-W1-5 (installation profiles).

---

## 1. Current state

As of the foundation build (`PLAN.md` §2): `docs/00` through `docs/04`, the ADRs, the
decision records in `research/decisions/`, and the design mockups exist. **No application
code ships yet.** Wave 1 is in flight: workspace stubs, a complete `datum-core`, the test
harness, and the remaining foundation documentation. Until Wave 1 integrates and Wave 2
batch 2.4 (the ledger) passes its property suite, there is nothing to install and
nothing to demo beyond documents and contracts.

---

## 2. Waves

Wave names and contents match `PLAN.md` v2 §3.

### 2.1 Wave 1 — foundation and documentation

| | |
|---|---|
| **What lands** | Root workspace (`CONTRACT-workspace.md`), compiling kernel stubs for every crate except `datum-core` and `datum-test`, complete `datum-core` (D1), `datum-test` plus `dev/sql` roles and grants, CI recipes, and parallel doc lanes (`docs/05`, `06`, `07`, `08`, `10`, repository files). Real pieces in the `datum-db` stub: pool, transaction-local actor, `Tx::begin`. |
| **What it proves** | Every Wave 2 lane can compile against frozen public types, dependency edges, and lint policy without inventing its own session protocol or money types. Documentation and code contracts agree on names, invariants, and the slice example set. |
| **What a shop can do** | Nothing yet. This wave produces no runnable product. |
| **Gate** | Integrated tree: placeholders absent, `just ci` green, `just ci-db` green with `DATUM_REQUIRE_PG=1`, dependency graph matches the contract (`CONTRACT-workspace.md` §10). Nothing in Wave 2 starts until this gate passes. |

### 2.2 Wave 2 — kernel, slice-first (batches 2.1–2.6)

| | |
|---|---|
| **What lands** | Batches in dependency order: `datum-db` (DDL, migrations, roles, version stamping); `datum-audit` (trigger attachment, hash chain); `datum-identity`, `datum-numbering`, `datum-uom`, `datum-events`; **`datum-ledger` (the gate)**; `datum-statemachine`, `datum-jobs`; minimal `datum-module` with profile configuration frozen per D-W1-5. |
| **What it proves** | Postings conserve per balance slice; withdrawals allocate; audit rows attach to every mutating write; actors are mandatory; the deferred constraint canary fails when armed; state transitions can require signatures by declaration (refused under `NoSignatures` until Wave 2b). |
| **What a shop can do** | Nothing yet. Still no HTTP API and no inventory screens. |
| **Gate** | Wave 2 does not close until `PLAN.md` §7 ledger property suite passes in **commit mode** with the canary armed. Batch 2.6 consumes `_team/specs/SPEC-profiles.md` (eleven keys from D-W1-5). |

### 2.3 Wave 2s — the vertical slice

| | |
|---|---|
| **What lands** | Modules `mod-items`, `mod-locations`, `mod-lots`, `mod-inventory`, `mod-production-min`, `mod-genealogy`, plus `server-slice` (HTTP API, OpenAPI, headless acceptance script). Canonical example set (`MDS-450-M4x12`, heat `HT-ATI-24-8831`, work order `WO-2026-1847`, and the rest in `PLAN.md` §3). |
| **What it proves** | The riskiest architecture choices work on real PostgreSQL data end to end: lot entities from the first posting, quarantine as postings, priced `TRANSFORMATION` with P3 consumption edges, genealogy forward/backward agreement, projections equal to ledger fold, audit attribution, both installation profiles with the same slice script (signature declarations differ only where the plan allows). |
| **What a shop can do** | Run the slice through the API only: receive bar stock by heat and lot, hold quarantine, issue to a minimal work order, complete to a finished lot, trace genealogy. Not a daily operations UI; not Phase 1 catalog completeness (valuation methods, full item master polish, and office workflows come later). |
| **Gate** | Headless script against the API asserting all thirteen items in `PLAN.md` §3 (Wave 2s acceptance), including green ledger properties and pass under **both** `regulated-device` and `plain-shop` profiles. |

### 2.4 Wave 2b — remaining kernel crates

| | |
|---|---|
| **What lands** | `datum-esign` (implements `SignatureGate`), `datum-customfields`, `datum-documents`, `datum-print`, in batches 2b.1–2b.3. Additive tables only. |
| **What it proves** | Regulated transitions can verify real signatures; controlled documents and archival print exist without retro-fitting columns that already carry history. Identity's signing credential reserved in batch 2.3 is consumed here. |
| **What a shop can do** | Still no full quality or doc-control modules. Kernel capabilities exist for later Phase 4 modules; the slice remains the operational demo until Wave 3 and later catalog phases. |
| **Gate** | Each batch's spec acceptance; no migration may alter historical columns. Release builds with a `Required` signature edge cannot bind `NoSignatures` (startup failure per D-W1-4). |

### 2.5 Wave 3 — interface

| | |
|---|---|
| **What lands** | UI shell and tokens, item master, shop floor terminal, genealogy trace — all against the slice API, under both profiles. Tauri deferred per ADR 0009. |
| **What a shop can do** | Touch-first shop floor and office screens for the **slice** scope only. Still not Phase 3 "run the shop on" breadth. |
| **Gate** | Owner visual approval on mockups (`PLAN.md` §4) before interface lanes start; phase-end API test under both profiles (`PLAN.md` §1a). |

---

## 3. Catalog phases mapped to waves

Copied from `PLAN.md` §3 ("Waves and phases"). Do not read a different wave assignment
into this table than PLAN states.

| Catalog phase | Waves | Ends with |
|---|---|---|
| Phase 0 — Kernel | Wave 1, Wave 2, Wave 2b | nothing user-visible; everything depends on it |
| Phase 1 — We know what we have | Wave 2s (the slice) grows into it; the rest after Wave 3 | stock with lot and serial traceability |
| Phases 2–7 | after this build | per the catalog |

The catalog's own **Ends with** column (`docs/04-module-catalog.md`, build phases table)
for every phase name:

| Catalog phase | Ends with |
|---|---|
| Phase 0 — Kernel | Nothing user-visible. Everything depends on it. |
| Phase 1 — We know what we have | Stock tracking with full lot and serial traceability |
| Phase 2 — We know what we buy and sell | Purchase-to-receive and quote-to-ship |
| Phase 3 — We know what we build and what it cost | **First release worth running a shop on** |
| Phase 4 — We can prove it | **First release worth switching to from a commercial suite** |
| Phase 5 — We can plan it | Material and capacity planning |
| Phase 6 — Full regulated coverage | Complete quality system |
| Phase 7 — Connected | Accounting, carriers, machines, CAD, EDI |

Phases 2–7 are **out of scope for the foundation build** (`PLAN.md` §10).

---

## 4. Installation profiles (first-release commitment)

Two profiles ship from the first release (`PLAN.md` §1a; DECISION D-W1-5). Both use one
binary, one license, one schema, and runtime enablement of compiled-in modules
(`docs/03-module-system.md` §5–§6).

| Profile | Enablement | What differs |
|---|---|---|
| **`regulated-device`** | Enables modules marked `regulated = true` (CAPA, complaints, calibration, training gates, e-signature workflows, validation navigation, and the rest per catalog). | Signature-bearing transitions are declared on regulated modules; `datum-esign` binds after Wave 2b. Validation/IQ navigation visible. |
| **`plain-shop`** | Enables **no** module with `regulated = true`. | No signature-bearing edges because no regulated module is enabled — not because the profile overrides module declarations. Hides Validation/IQ and regulated module navigation. |

**Shared and not configurable:** audit trigger and hash chain, server-side time, actor on
every write, no hard deletes of records (schema `app` vs `transient`, D-W1-2), version
stamping, constrained lot and serial identifiers, audit export. A plain shop does not see
these in daily screens; it still pays storage for them.

**Module-owned signatures:** profiles may not add, remove, or downgrade signature
requirements. `mod-genealogy` is `regulated = false`; slice acceptance runs unchanged on
both profiles.

Profile keys are frozen in `_team/specs/SPEC-profiles.md` before Wave 2 batch 2.6.

---

## 5. Milestones that matter to an outsider

Ordered by what they **prove**, not by feature checklists alone.

### 5.1 Slice runs end to end on real data through the API

**Proves:** The ledger, audit, identity, lots, inventory postings, minimal production,
and genealogy read model are coherent under PostgreSQL constraints and real concurrency —
the engineering bet behind Phase 1 inventory retires here, not when Wave 2 closes.

**Contains:** Wave 2s modules and `server-slice`, acceptance script per `PLAN.md` §3.

**Shop utility:** API-level demo and integration testing only until Wave 3 UI lands.

### 5.2 First release worth running a shop on (catalog Phase 3)

**Proves:** A discrete manufacturer can run production, costing, and shop floor against
the same ledger and audit kernel — BOM, routing, full work orders, labor, and job costing
tie movements to money without a parallel inventory truth.

**Contains:** Catalog Phase 3 modules (`bom`, `work-centers`, `routing`, `production`,
`shop-floor`, `costing`, `labor`) on top of Phase 1–2 foundations. This build's plan
**does not** schedule those modules; the milestone names the catalog's phase end state.

### 5.3 First release worth switching from a commercial suite (catalog Phase 4)

**Proves:** Regulated record properties and quality workflows (doc control, inspection,
NCR, CAPA, genealogy as product, calibration, training, change control, DHR/DMR) sit on
the same database and ledger the shop already runs — the wedge commercial suites sell as
a separate quality stack.

**Contains:** Catalog Phase 4 modules. Wave 2b kernel pieces (signatures, documents,
print) are prerequisites; full Phase 4 surface is later work.

### 5.4 Why there are no dates

Lanes run in parallel with mechanical gates (integration, ledger properties, slice
script, visual approval). Calendar estimates would imply a serial plan the repository does
not enforce and would invite treating Phase 2+ modules as committed schedule. Progress is
reported as **gates passed** and **phases ended** per `docs/04-module-catalog.md`, not
quarters.

---

## 6. Dependencies

### 6.1 Cannot be reordered

| Dependency | Reason |
|---|---|
| Audit trigger before any application table | Tables created without the trigger are permanently unaudited (`PLAN.md` Wave 2 batch 2.2; `research/decisions/audit-persistence.md`). |
| `datum-db` session protocol before module DDL | Actor and transaction id must exist before any mutating write (invariant 5). |
| Identity (and signing credential reservation) before electronic signatures | Wave 2b must not add columns to tables that already carry history (`PLAN.md` batch 2.3, invariant 14). |
| Ledger before inventory and production modules | Inventory **is** postings; modules call `PostingSink`, not a parallel quantity store (`docs/04-module-catalog.md` Phase 1). |
| Ledger property suite before Wave 2 close | Everything downstream assumes conservation, allocation, and canary-armed constraints (`PLAN.md` §7). |
| Slice modules before `server-slice` | API exposes only what modules register (`PLAN.md` Wave 2s order 2s.1–2s.4). |
| Wave 2s before Wave 2b | Signatures and controlled documents gate on a running unsigned slice (`PLAN.md` §0 item 3). |
| Visual approval before Wave 3 UI lanes | `PLAN.md` §4. |

### 6.2 Can be parallelized or reordered within a band

| Area | Flexibility |
|---|---|
| Wave 1 doc lanes | Any order; must merge before Wave 2. |
| Batch 2.3 crates | `datum-identity`, `datum-numbering`, `datum-uom`, `datum-events` are independent of each other (`PLAN.md` batch 2.3). |
| Wave 2s.1 | `mod-items`, `mod-locations`, `mod-lots` in parallel. |
| Wave 2s.3 | `mod-production-min` and `mod-genealogy` in parallel once inventory exists. |
| Wave 2b batches | 2b.1 (`esign`, `customfields`) before 2b.2–2b.3, but esign and customfields do not depend on each other within 2b.1. |
| Catalog Phases 2–7 after the foundation build | Order follows module dependencies in `docs/04-module-catalog.md`; not fixed by this wave plan beyond "after this build". |

---

## 7. How to help now

1. **Kernel crates** — Wave 1 owns `datum-core` and stubs; Wave 2 lanes own real
   implementations per `CONTRACT-workspace.md` §4. Pick a crate whose spec exists for the
   current batch; do not add workspace dependencies or kernel edges without escalation.

2. **Documentation** — Read `docs/00` through `docs/04` and the ADRs before code. New
   contributors should align examples with the canonical set in `PLAN.md` §3 (titanium
   bone screw `MDS-450-M4x12`, heat `HT-ATI-24-8831`, work order `WO-2026-1847`).

3. **Scarcest input** — A real shop's requirements (tolerances, quarantine rules, how
   lots are named on the receiving dock, what "complete" means on a work order) matter
   more than another abstract layer diagram. If you run manufacturing operations, recording
   one honest workflow against the catalog modules list is high leverage.

4. **Repository** — Public tree `github.com/BlinkingSun/datum-erp`; license
   AGPL-3.0-or-later with DCO (ADR 0006). Do not push from lane worktrees until
   integration.

---

## 8. What this roadmap does not do

It does not assign dates or durations. It does not promise catalog Phase 2+ modules as
part of the foundation build schedule. Feature lists for later phases live in
`docs/04-module-catalog.md`; delivery order after Wave 3 follows that catalog and future
plans, not implied timelines here.
