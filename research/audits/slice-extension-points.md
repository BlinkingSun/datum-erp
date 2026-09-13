# Plan audit slice 9 — Module extension points

**Scope:** `docs/03-module-system.md` §3 (five extension points), `docs/01` §§2,5,6, `docs/04-module-catalog.md`, `PLAN.md` §3 Wave 3 / §10 (runtime plugins deferred).  
**Date:** 2026-09-11  
**Verdict on “five is enough”:** Five is enough for *integration* and *adjacent* features if Phase 0 kernel + first-party modules own domain algorithms and print. Five is **not** enough for **requirement 2** (“third party ships without forking”) for the beachhead’s highest-frequency customization: **regulated document and label output**. That gap is architectural (kernel vs module), not fixable by “use hooks harder.”

---

## 1. The five points (reminder)

| # | Point | Power | Hard limits (`docs/03` §4) |
|---|--------|--------|---------------------------|
| 3.1 | Events | Async, at-least-once, idempotent subscribers | Cannot participate in originating transaction; cannot veto |
| 3.2 | Hooks | Sync in-tx; read; veto; **contribute postings to same ledger group** | No network I/O; time budget; cannot write outside tx; cannot suppress kernel/module core behavior |
| 3.3 | Custom fields | Typed, validated, audited on *another* module’s entity | Stored fields only (manifest `type`, `validate`) — not layout, not graphs, not formulas |
| 3.4 | Routes / API | Own namespace; OpenAPI | Cannot replace host module’s internal logic unless host calls out |
| 3.5 | UI | Pages, **declared slots**, nav, widgets | No DOM patch; no template inheritance |

**Not extension points:** modify another module’s tables/behavior, suppress audit/signature/ledger balance, raw DB.

**Phase 1 distribution:** compiled-in Rust only; extensibility in practice = **public HTTP API + event stream** (`docs/03` §5). Runtime WASM plugins deferred (PLAN §10).

---

## 2. Customization matrix (discrete / medical manufacturers)

Legend: **OK** = expressible by a third party without forking core *given* a cooperating first-party host module; **KERNEL** = belongs in kernel or first-party module config, not third-party extension; **WORKAROUND** = external service / duplicate UX; **FAIL** = no faithful expression → fork or abandon req 2.

| Customization | Typical buyer | Map to extension point(s) | Result |
|---------------|---------------|-----------------------------|--------|
| Custom ATP / allocation (include POs, WIP, rules per plant) | Contract mfg, medical OEM | Events (`sales.*`, `inventory.*`, `purchasing.*`) + **Routes/API** (external ATP service); optional **Custom fields** for flags | **WORKAROUND** for substitute engine (`docs/03` §5). **KERNEL** if ATP stays in `mps`/`inventory` projections. Third party cannot replace in-tx allocation without forking projection logic. |
| Contract price + metal surcharge + qty breaks | Job shops, implant suppliers | **Custom fields** (surcharge %, contract id); pricing math in `sales` | **KERNEL / first-party** for formula. No `before_price` hook documented → exotic **FAIL** for third-party pricing module without `sales` API callbacks. |
| Costing: burden per machine-hr vs labor-hr vs per piece | Any shop with job costing | **Hooks** on completion *might* add ledger postings; **Events** for async analytics | **FAIL** to *replace* standard costing posting set (`§4` no override). Shop config + `costing` module, not extension. |
| Numbering: check digit, site prefix, separate WO vs traveler series | Regulated shops (820 traceability) | — | **KERNEL** (`wicket-numbering`, `docs/02` §2). Not events/hooks/UI. Pluggable strategy **not** one of five → **FAIL** for third-party numbering pack without kernel hook or fork. |
| Insert `FirstArticleHold` between `Released` and `InProcess` on WO | AS9102 / medical contract mfg | **Hooks** can veto `Released→InProcess` | **FAIL** for true intermediate *state* (audit trail, signatures, dispatch lists). `§4` forbids overriding host state machine; host must declare states (`states.rs`). |
| Change completion postings (backflush vs pull, scrap account) | High-mix machining | **Hooks** add postings; shop policy in `production` | **FAIL** to remove/replace core completion postings; only additive adjustments → reconciliation pain. First-party configuration. |
| **Traveler / packing list / CoC PDF layout** | **Every beachhead shop** | UI slots ≠ PDF; **API** cannot intercept kernel render path | **FAIL** — see §3. |
| ZPL / UDI label templates | Medical device (Phase 6 `udi`, Phase 7 `barcode`) | **Routes/API** if server exposes print; else kernel print | **FAIL** as third-party template pack until print is extensible; catalog defers `barcode` to Phase 7. |
| Approval routing graph varies by $ threshold or customer | `change-control`, NCR MRB | **Custom fields** on doc insufficient (graph, not field) | **FAIL** — workflow graph is host module logic; needs declared workflow extension or first-party rules engine. |
| Computed / formula fields (rollup, not stored) | Estimating, compliance dashboards | **Custom fields** are stored | **FAIL** — needs computed-field primitive or read-model module with **API** only (stale / duplicate UX). |
| Row-level security (buyer → own suppliers only) | Mid-size OEM | **Permissions** in manifest | **KERNEL** (`wicket-identity`). ABAC / row scopes **not** an extension point → policy in kernel or **FAIL** for module-only RLS. |
| Substitute finite scheduler | Shops outgrowing dispatch lists | **Events** + **Routes/API** external optimizer | **WORKAROUND** (`docs/03` §5) — acceptable defer. |
| EDI mapping (850/855/856/810) | Larger OEM (explicit non-target `docs/01` §3) | **Routes/API** + **Events**; Phase 7 `edi` | **OK** as external or compiled module with own tables; not a counterexample for beachhead. |
| Customer Excel import, per-customer column maps | Contract manufacturers | **Routes/API** + **jobs** | **OK** — integration module pattern. |
| Multi-step receiving wizard + extra inspection gates | Medical incoming | **UI** slots on receive screen + **Hooks** `before_transition` on receipt | **Partial OK** if `receiving` declares slots and hook points; **FAIL** if host wizard is monolithic (no step injection contract). |
| Training veto bypass via **signed deviation** (documented) | FDA shops with deviation process | **Hooks** veto; deviation as signed record + hook reads it | **Widen hooks** / kernel **deviation** primitive — not necessarily sixth point. Hook ABI must expose “active approved deviations for (operator, operation, WO)” and audit the bypass. |

---

## 3. Single best counterexample (fork driver)

### **Regulated print / report templates (traveler, packing list, CoC, DHR excerpts, AS9102 FAI)**

**Why this beats state-machine insertion or approval graphs**

1. **Universality:** `docs/01` §7 success criterion 2 (scan traveler on floor) and `docs/04` shipping/DHR/inspection all assume paper/PDF artifacts. Contract manufacturers win work on *how* the CoC reads, not only data correctness.
2. **Paid customization:** Shops pay consultants for Bartender, NiceLabel, Crystal reports, or ERP-specific report writers. This is recurring revenue adjacent to validation (IQ/OQ often includes sample printouts).
3. **None of the five apply:**
   - **Events:** fire after facts; cannot define layout of archived PDF tied to transaction.
   - **Hooks:** veto/postings only; no render pipeline.
   - **Custom fields:** data on entity, not presentation.
   - **Routes/API:** can serve *alternate* PDF only if UI and archival path call the module; core flows still invoke kernel print → dual artifacts or fork.
   - **UI slots:** React panels; not server-side PDF identical on desktop/Tauri (`docs/02` §5: server-rendered PDF, must archive).
4. **Docs trap:** `docs/04` Phase 0 lists **“Reporting and print”** as kernel infrastructure; `docs/03` §3 lists exactly five module extension points and does **not** include templates. `docs/03` §4 forbids overriding behavior — replacing the kernel’s render for `production.traveler` is override.
5. **Fork path:** Team patches kernel print templates or maintains a private `wicket-server` branch that hardcodes layouts — classic open-source ERP fork for “our traveler.” That directly violates `docs/03` §1 requirement 2.

**Runner-up (medical-specific, still fork):** inserting a host state (`FirstArticleHold`) in `production`’s machine — quality module can only **hook-veto**, not model hold queues, partial release, or signature on “exit hold.” Shops will pay; expression requires host-declared **optional states** contract (not documented).

---

## 4. Recommendation

| Option | Assessment |
|--------|------------|
| **Add sixth extension point: Report / print templates** | **Recommend ADD (document + crate), defer implementation detail to Wave 2/3 boundary.** Define: versioned template packs (HTML/CSS → PDF or dedicated DSL), bound to document type + revision, signatures hash **rendered output**, modules register templates via `wicket-module` manifest capability. Keeps Odoo-style inheritance forbidden while allowing third-party *packs*. |
| **Widen hooks only** | Insufficient for print. Useful for **deviation-aware veto** (training clock-on) — add to hook context + audit reason codes, not a new point. |
| **Keep five + external service** | Honest for **scheduler/EDI** (`docs/03` §5). **Refuse** for **archived regulated PDFs** — external print breaks single validated artifact and Part 11 “record integrity” story unless kernel delegates rendering with same audit chain. |
| **PLAN text to add** | Under Wave 2: new lane or sub-crate **`wicket-report`** (or fold into `wicket-documents` with explicit template ABI) + **`wicket-module` acceptance:** register/events/hooks/customfields/routes/ui-slot/**report-template** ABIs frozen at 1.0. Under Wave 2 gate: “hook invocation order, ledger contribution rules, veto payload schema, template binding to record version” are **semver kernel contracts.** Under §10: “Runtime plugins deferred; **template packs compiled-in** in Phase 1.” |

**Do not** add infinite configurability (`docs/01` §6): one bounded template system, not arbitrary code in templates (sandbox/WASM Phase 3 if ever).

---

## 5. Hook ABI and PLAN gap

`docs/03` §3.2: hooks are **synchronous, in-transaction**, may **veto** and **contribute postings to the same ledger group**. That ties:

- `wicket-statemachine` (transition identity, before/after),
- `wicket-ledger` (group_id, balance-to-zero, dimensions),
- `wicket-audit` / `wicket-esign` (veto and posting attribution),
- `wicket-module` (registration, ordering, budgets).

**PLAN.md gap:** Wave 2 lists thirteen parallel kernel lanes but **no** task for:

- Hook ABI types (veto reason schema, posting contribution envelope, timeout),
- Registration surface (which transitions expose hooks — host module manifest),
- Ordering semantics (deterministic hook order; failure = tx rollback).

Without this, Wave 3 modules (`training` veto, `calibration` veto) will invent ad hoc traits → breaking change later. **This is a freeze item for Wave 2 close**, owned by **`wicket-module`** with dependencies on real (not stub) `wicket-statemachine` + `wicket-ledger`.

**Events ABI** is lower risk (append-only event schema per `docs/03` §7) but still needs `wicket-events` + `wicket-module` registration contract.

---

## 6. Wave 2 acceptance criteria (missing today)

PLAN §7 gives generic testing obligations; **no Wave 2 per-crate SPECs.** For extension points to be real at Wave 3:

**`wicket-module` acceptance criteria should require:**

1. **Events:** register publisher/subscriber types; idempotency key contract; at-least-once test harness.
2. **Hooks:** register on named transition; veto propagates; contributor postings share `group_id` and preserve zero-sum (integration test with `wicket-ledger`).
3. **Custom fields:** manifest fragment merged; audit parity test with native column.
4. **Routes:** mount under `/api/{module}`; appears in OpenAPI aggregate.
5. **UI slots:** host declares slot id; guest panel metadata only (no implementation in kernel crate).
6. **Report templates (proposed):** register `template_id` for `document_type`; render fixture record → stable PDF hash; revision bump invalidates prior template in validation manifest.

Other crates: `wicket-statemachine` exports hook anchor types; `wicket-ledger` exports `HookPosting` builder; optional **`wicket-report`** exports template registry.

---

## 7. PLAN vs catalog: missing print crate

| Source | Says |
|--------|------|
| `docs/04` Phase 0 | **Reporting and print** — kernel, size M |
| `PLAN.md` §5 crate graph | **No** `wicket-report` / `wicket-print` crate |
| `docs/03` §3 | Five extension points — **print not included** |

**Gap:** Print is simultaneously **kernel infrastructure** and **per-shop customization surface**. Resolving that split requires either a dedicated Wave 2 crate or an explicit sub-scope of `wicket-documents` plus a sixth extension point. Leaving it implicit guarantees Wave 3 `production`/`shipping` embed hardcoded templates and later extraction is a breaking migration.

---

## 8. EXECUTOR / SPLIT / TIER — `wicket-module`

| Field | Recommendation |
|-------|----------------|
| **EXECUTOR** | **cursor** (integration-heavy Rust, trait design, many crate edges) — not a blind race; Opus/Fable decision on hook ordering and template sandbox before implementation lands. |
| **SPLIT** | **Serial after** `wicket-events`, `wicket-statemachine`, `wicket-ledger`, `wicket-customfields`, `wicket-identity` (and **`wicket-report`** if added) have non-stub public types. Wave 1 stubs allow compile-only fan-out; **acceptance tests for `wicket-module` cannot run until hook/posting integration exists** — same structural issue as slice 4 (stub contract). Sub-split: (A) registry/manifest/lifecycle, (B) hook dispatcher, (C) route/UI metadata — still one lane, sequential milestones inside lane. |
| **AUDIT TIER** | **deep** — carries `docs/03` §1 requirements 2–4; mistake becomes Odoo or unvalidated plugin surface. |

---

## 9. Summary table: five points vs beachhead pain

| Pain tier | Examples | Extension fit |
|-----------|----------|----------------|
| Low | Excel import, EDI bridge, external scheduler | API + events (§5) |
| Medium | Receiving wizard panels, calibration/training veto | UI slots + hooks (if hosts declare) |
| High | ATP, pricing, costing, numbering, RLS | Kernel / first-party modules |
| **Critical (fork)** | **PDF traveler/CoC/DHR layout; ZPL labels** | **Missing 6th point + missing `wicket-report` crate** |

---

## 10. Answers to charter questions (explicit)

1. **Are five enough?** Enough to avoid Odoo; **not** enough for third-party print or host state-machine extension without new contracts.
2. **Best counterexample:** Regulated **server PDF / print templates** (traveler, CoC, packing list).
3. **Add / refuse / defer:** **Add** sixth point (report templates) + **defer** WASM template code execution; **widen hooks** for deviation context; **defer** external-only for planning/EDI; **document refuse** of arbitrary workflow/override (`§4` stands).
4. **Hook ABI in PLAN?** **Yes — gap.** Freeze in Wave 2 via `wicket-module` + `wicket-statemachine` + `wicket-ledger` integration criteria.
5. **Wave 2 ABIs as acceptance criteria?** **Yes** — especially `wicket-module`; today PLAN has none.
6. **Print sixth point + crate?** **Yes** — align `docs/03`, `docs/04`, and PLAN §5 crate graph.

---

*End of slice 9 report.*
