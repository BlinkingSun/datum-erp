# Build Roadmap

*What gets built in what order, what each step proves, and what a shop can actually
use at each point. Nothing in this document is scheduled with dates: the build is
parallel lanes on a single critical path, and the honest unit of progress is a gate that
passes, not a calendar week.*

**Conforms to:** ADR 0001, 0004, 0005, 0006, 0007, 0008, 0009, 0010 (Proposed); `PLAN.md` v2 §1a and §3;
DECISION D-W1-5 (installation profiles).

---

## 1. Current state

Pre-alpha. There is no release.

The foundation-build snapshot is stale. `PLAN.md` §2 still says all code is unwritten.
`HANDOFF.md` §2 still says Wave 1 is integrated and Wave 2 batch 2.1 (`wicket-db`) is next.
This tree is neither of those states.

What is on disk, counted in this worktree:

| | |
|---|---|
| Crates | Seventeen under `crates/` (`Cargo.toml` workspace members: `wicket-core` through `wicket-server`). |
| Modules | Six under `modules/`: `items`, `locations`, `lots`, `inventory`, `production_min`, `genealogy` (`modules/README.md`). |
| Rust sources | 312 `.rs` files under `crates/` and `modules/`. |
| HTTP | Slice, esign, custom-fields, documents, and print routes are mounted (`crates/wicket-server/src/http.rs:18-128`). |
| Profiles | `profiles/plain-shop.toml` and `profiles/regulated-device.toml` exist. |
| Slice suite | `crates/wicket-server/tests/slice.rs` (twenty `#[tokio::test]` functions in this tree). |

Those crates are implementations with `src/`, migrations, and tests, not the compiling
stubs Wave 1 named. `wicket-ledger` is an append-only posting engine
(`crates/wicket-ledger/src/lib.rs:1-5`). The six modules each ship a `module.toml`.
`docs/11-module-common-rules.md:3` and `research/decisions/w2s-rulings.md:3` describe a
Wave 2s close; `research/decisions/w2-rulings.md` and `research/decisions/w2b-rulings.md`
exist as ruling records. A formal wave-close gate record is ABSENT: the source those
waves name is present, and HANDOFF.md still contradicts the tree.

Wave 3 interface code is ABSENT (no UI package in the workspace; `PLAN.md` §3 Wave 3;
ADR 0009). A shop cannot install a daily operations product.

The single measure of progress is the slice acceptance suite in §2.3, not a wave label
and not a crate count. Breadth work must wait until that suite is boring (§9).

---

## 2. Waves

Wave names and contents match `PLAN.md` v2 §3.

### 2.1 Wave 1 — foundation and documentation

| | |
|---|---|
| **What lands** | Root workspace (`CONTRACT-workspace.md`), compiling kernel stubs for every crate except `wicket-core` and `wicket-test`, complete `wicket-core` (D1), `wicket-test` plus `dev/sql` roles and grants, CI recipes, and parallel doc lanes (`docs/05`, `06`, `07`, `08`, `10`, repository files). Real pieces in the `wicket-db` stub: pool, transaction-local actor, `Tx::begin`. |
| **What it proves** | Every Wave 2 lane can compile against frozen public types, dependency edges, and lint policy without inventing its own session protocol or money types. Documentation and code contracts agree on names, invariants, and the slice example set. |
| **What a shop can do** | Nothing yet. This wave produces no runnable product. |
| **Gate** | Integrated tree: placeholders absent, `just ci` green, `just ci-db` green with `WICKET_REQUIRE_PG=1`, dependency graph matches the contract (`CONTRACT-workspace.md` §10). Nothing in Wave 2 starts until this gate passes. |

### 2.2 Wave 2 — kernel, slice-first (batches 2.1–2.6)

| | |
|---|---|
| **What lands** | Batches in dependency order: `wicket-db` (DDL, migrations, roles, version stamping); `wicket-audit` (trigger attachment, hash chain); `wicket-identity`, `wicket-numbering`, `wicket-uom`, `wicket-events`; **`wicket-ledger` (the gate)**; `wicket-statemachine`, `wicket-jobs`; minimal `wicket-module` with profile configuration frozen per D-W1-5. |
| **What it proves** | Postings conserve per balance slice; withdrawals allocate; audit rows attach to every mutating write; actors are mandatory; the deferred constraint canary fails when armed; state transitions can require signatures by declaration (refused under `NoSignatures` until Wave 2b). |
| **What a shop can do** | Nothing yet. Still no HTTP API and no inventory screens. |
| **Gate** | Wave 2 does not close until `PLAN.md` §7 ledger property suite passes in **commit mode** with the canary armed. Batch 2.6 consumes `_team/specs/SPEC-profiles.md` (eleven keys from D-W1-5). |

### 2.3 Wave 2s — the vertical slice

| | |
|---|---|
| **What lands** | Modules `mod-items`, `mod-locations`, `mod-lots`, `mod-inventory`, `mod-production-min`, `mod-genealogy`, plus `server-slice` (HTTP API, OpenAPI, headless acceptance script). Canonical example set (`MDS-450-M4x12`, heat `HT-ATI-24-8831`, work order `WO-2026-1847`, and the rest in `PLAN.md` §3). |
| **What it proves** | The riskiest architecture choices work on real PostgreSQL data end to end: lot entities from the first posting, quarantine as postings, priced `TRANSFORMATION` with P3 consumption edges, genealogy forward/backward agreement, projections equal to ledger fold, audit attribution, both installation profiles with the same slice script (signature declarations differ only where the plan allows). |
| **What a shop can do** | Run the slice through the API only: receive bar stock by heat and lot, hold quarantine, issue to a minimal work order, complete to a finished lot, trace genealogy. Not a daily operations UI; not Phase 1 catalog completeness (valuation methods, full item master polish, and office workflows come later). |
| **Gate** | The project's single measure of progress. `crates/wicket-server/tests/slice.rs` must assert all thirteen items in `PLAN.md` §3 (Wave 2s acceptance) under both `regulated-device` and `plain-shop`, including green ledger properties. Presence of module directories is not the gate; the script being boring is. |

That suite is the scoreboard. It is a headless script against the API
(`crates/wicket-server/tests/slice.rs:1`). The two profile tests
(`slice_end_to_end_plain_shop`, `slice_end_to_end_regulated_device`) drive the
canonical example set through the thirteen assertions. Twenty `#[tokio::test]`
functions live in that file in this tree.

`just ci` does **not** run it. `justfile:392` defines `ci` as `fmt-check clippy
lint-sql test-lib`. `justfile:313-314` defines `test-lib` as
`cargo test --workspace --lib --all-features`, which excludes every integration
test under `crates/*/tests/`. Only `just ci-db` (`justfile:397`, which adds
`test-db`) exercises `slice.rs`. Wiring the suite into `just ci` is ABSENT
(promised: `TODO.md` T-15).

Breadth work must not start while that script is not boring. Catalog Phase 4
quality workflows, CAD estimating, incumbent migration adapters, and UI chrome
must wait; see §9 and the dependency rules in §6.

### 2.4 Wave 2b — remaining kernel crates

| | |
|---|---|
| **What lands** | `wicket-esign` (implements `SignatureGate`), `wicket-customfields`, `wicket-documents`, `wicket-print`, in batches 2b.1–2b.3. Additive tables only. |
| **What it proves** | Regulated transitions can verify real signatures; controlled documents and archival print exist without retro-fitting columns that already carry history. Identity's signing credential reserved in batch 2.3 is consumed here. |
| **What a shop can do** | Still no full quality or doc-control modules. Kernel capabilities exist for later Phase 4 modules; the slice remains the operational demo until Wave 3 and later catalog phases. |
| **Gate** | Each batch's spec acceptance; no migration may alter historical columns. Release builds with a `Required` signature edge cannot bind `NoSignatures` (startup failure per D-W1-4). |

### 2.5 Wave 3 — interface

| | |
|---|---|
| **What lands** | UI shell and tokens, item master, shop floor terminal, genealogy trace — all against the slice API, under both profiles. Tauri deferred per ADR 0009. |
| **What it proves** | The office and shop-floor screens run under both `regulated-device` and `plain-shop` against the same Wave 2s slice API. Still not catalog Phase 3 breadth. |
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
| **`regulated-device`** | Enables modules marked `regulated = true` (CAPA, complaints, calibration, training gates, e-signature workflows, validation navigation, and the rest per catalog). | Signature-bearing transitions are declared on regulated modules; `wicket-esign` binds after Wave 2b. Validation/IQ navigation visible. |
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

Ordered by what they **prove**, not by feature checklists alone. Until the slice
suite in §2.3 is boring, later milestones in this list are sequenced, not started.

### 5.1 Slice runs end to end on real data through the API

**Proves:** The ledger, audit, identity, lots, inventory postings, minimal production,
and genealogy read model are coherent under PostgreSQL constraints and real concurrency —
the engineering bet behind Phase 1 inventory retires here, not when Wave 2 closes.

**Contains:** Wave 2s modules and `server-slice`. The acceptance script is
`crates/wicket-server/tests/slice.rs`, asserting the thirteen items in `PLAN.md` §3
under both profiles. That file is the scoreboard (§2.3). `just ci` does not run it;
only `just ci-db` does. Wiring the suite into `just ci` is ABSENT (promised: `TODO.md` T-15).

**Shop utility:** API-level demo and integration testing only. Wave 3 UI chrome is
frozen until this suite is boring (§9).

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
a separate quality stack (`research/background/competitive-landscape.md` §7.2–§7.3).

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
| `wicket-db` session protocol before module DDL | Actor and transaction id must exist before any mutating write (invariant 5). |
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
| Batch 2.3 crates | `wicket-identity`, `wicket-numbering`, `wicket-uom`, `wicket-events` are independent of each other (`PLAN.md` batch 2.3). |
| Wave 2s.1 | `mod-items`, `mod-locations`, `mod-lots` in parallel. |
| Wave 2s.3 | `mod-production-min` and `mod-genealogy` in parallel once inventory exists. |
| Wave 2b batches | 2b.1 (`esign`, `customfields`) before 2b.2–2b.3, but esign and customfields do not depend on each other within 2b.1. |
| Catalog Phases 2–7 after the foundation build | Order follows module dependencies in `docs/04-module-catalog.md`; not fixed by this wave plan beyond "after this build". |

---

## 7. How to help now

1. **Kernel crates** — Wave 1 owns `wicket-core` and stubs; Wave 2 lanes own real
   implementations per `CONTRACT-workspace.md` §4. Pick a crate whose spec exists for the
   current batch; do not add workspace dependencies or kernel edges without escalation.

2. **Documentation** — Read `docs/00` through `docs/04` and the ADRs before code. New
   contributors should align examples with the canonical set in `PLAN.md` §3 (titanium
   bone screw `MDS-450-M4x12`, heat `HT-ATI-24-8831`, work order `WO-2026-1847`).

3. **Scarcest input** — A real shop's requirements (tolerances, quarantine rules, how
   lots are named on the receiving dock, what "complete" means on a work order) matter
   more than another abstract layer diagram. If you run manufacturing operations, recording
   one honest workflow against the catalog modules list is high leverage.

4. **Repository** — Public tree `github.com/BlinkingSun/wicket-erp`; license
   AGPL-3.0-or-later with DCO (ADR 0006). Do not push from lane worktrees until
   integration.

---

## 8. What this roadmap does not do

It does not assign dates or durations. It does not promise catalog Phase 2+ modules as
part of the foundation build schedule. Feature lists for later phases live in
`docs/04-module-catalog.md`; delivery order after Wave 3 follows that catalog and future
plans, not implied timelines here.

---

## 9. Frozen until the slice is boring

These are correct ideas. They are not next. The order is the dependency table in §6,
not a rejection of the work.

Until `crates/wicket-server/tests/slice.rs` is boring — green under both profiles
and asserting the thirteen items in `PLAN.md` §3 — the following must stay frozen:

| Work | Why it waits | §6 rule |
|---|---|---|
| Wave 3 UI chrome (shell, item master, shop floor, genealogy screens) | The screens consume the slice API (`PLAN.md` §3 Wave 3). Visual approval is a further gate (`PLAN.md` §4). | Visual approval before Wave 3 UI lanes. |
| Catalog Phase 4 quality workflows (doc control, inspection, NCR, CAPA, DHR/DMR as product) | Phase 4 sits on a shop already running the slice (`docs/04-module-catalog.md`; this document §3 and §5.3). | Catalog Phases 2–7 after the foundation build. |
| CAD-native estimating | Catalog Phase 7 (`docs/04-module-catalog.md` "Ends with" table). | Catalog Phases 2–7 after the foundation build. |
| Incumbent migration adapters | Goal 3 (`GOALS.md` §3). Loaders must call published HTTP APIs. Those APIs are the slice. | Slice modules before `server-slice`. |

No new phase is named here. No existing phase is renamed. Wave 2b kernel crates
(signatures, documents, print, custom fields) remain sequenced after a running
slice (`PLAN.md` §0 item 3; §6 "Wave 2s before Wave 2b"). That order stands.
Phase 4 modules on top of those crates are frozen with the rest of this table.
