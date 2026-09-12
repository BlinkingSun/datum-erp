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

| Lane | Owns (exclusive) | Provider | Audit |
|---|---|---|---|
| `workspace` | `Cargo.toml`, `rust-toolchain.toml`, `.cargo/`, `justfile`, `.github/workflows/`, `dev/`, and a **compiling stub** for every crate in section 5 | assign | deep |
| `doc-datamodel` | `docs/05-data-model.md` | assign | standard |
| `doc-roadmap` | `docs/07-roadmap.md` | assign | standard |
| `doc-api` | `docs/10-api-conventions.md` | assign | standard |
| `doc-repo` | `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `.gitignore` | assign | standard |
| `doc-adr` | `docs/adr/0008-single-tenant.md`, `docs/adr/0009-ui-stack.md` | assign | standard |
| `doc-regulatory` | `docs/06-regulatory.md` | assign | deep |
| `doc-landscape` | `docs/08-competitive-landscape.md` | assign | standard |

`doc-regulatory` and `doc-landscape` are **gated on their spike reports landing**.

### Wave 2 — kernel crates (wide fan-out)

Every lane owns exactly one crate directory and nothing else. Thirteen lanes, all
parallel, all branching from a merged Wave 1.

### Wave 3 — server, modules, and interface

Gated on the UI approval gate for anything user-facing. Covers `datum-server`, the
Phase 1 modules from `docs/04-module-catalog.md`, the web shell, and the Tauri
desktop shell.

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
with explicit scale, `Quantity` carrying its unit of measure as a type parameter,
`Actor`, and the shared error and result types. Everything else depends on it, which
means it is the crate most expensive to get wrong and it gets a deep audit.

## 6. Non-negotiable invariants

Every lane is accountable to these. An audit that finds a violation is a fail verdict
regardless of whether the lane's own acceptance criteria passed.

1. **No stored balances.** No column anywhere holds a running quantity or value that
   application code updates. Projections are explicitly named as caches and are
   rebuildable.
2. **Posting groups sum to zero**, enforced by a deferred database constraint, not by
   application code.
3. **The audit table is append-only at the grant level.** The application role holds
   insert and select and does not hold update or delete.
4. **Time is server-side.** No client-supplied timestamp is ever stored as the time of
   record.
5. **Every mutation has an attributable actor.** Background work runs as a named
   service principal.
6. **No module reads another module's tables.** Published interfaces only.
7. **No `unsafe`** in any crate without a written justification in the escalation log.
8. **Every migration has a tested reverse.**

## 7. Testing obligation

Per lane, not negotiable, and audited:

- Unit tests for the crate's own logic.
- **Property tests on `datum-ledger`**, generating arbitrary transaction sequences and
  asserting that groups balance and that projections always equal the ledger sum. This
  is the highest-value test in the project.
- Migration tests that run forward and backward against seeded data.
- No lane declares done on code that does not compile and does not pass its own tests.

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
