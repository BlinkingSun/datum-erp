# Standing orders for agents

Conforms to: [ADR 0001](docs/adr/0001-modular-monolith.md) (Proposed), [ADR 0003](docs/adr/0003-database.md) (Accepted, as amended), [ADR 0004](docs/adr/0004-append-only-ledger.md) (Proposed), [ADR 0005](docs/adr/0005-compliance-in-kernel.md) (Proposed), [ADR 0006](docs/adr/0006-license.md) (Accepted), [ADR 0007](docs/adr/0007-defer-general-ledger.md) (Proposed), [ADR 0008](docs/adr/0008-single-tenant.md) (Proposed), [ADR 0010](docs/adr/0010-one-registry.md) (Proposed); [GOALS.md](GOALS.md); [PLAN.md](PLAN.md) §1a and §6; [docs/01-vision-and-scope.md](docs/01-vision-and-scope.md) §6.

Audience: any AI agent pointed at this repository. Status: standing orders.
Pre-alpha. There is no release (`README.md` §2).

Every sentence below is `is`, `must`, or `ABSENT`. There is no fourth mood.

## 1. Product law

Goal 2 (`GOALS.md` §2) is the product law. A capability is not real until a
stranger reaches it through `/api/v1` under both `regulated-device` and
`plain-shop` (`PLAN.md` §1a) with an expected result written next to the command
(`GOALS.md` DOC-1). A `pub fn` with no mount is inventory.

Goal 2 is false (`GOALS.md` §6). One capability table
(`crates/wicket-server/src/capabilities.rs`) generates the router and the
OpenAPI document (T-24, T-25). `scripts/lint-mounts.sh` fails a string-literal
`.route("` and an unbound capability id. Unmounted `module.toml` routes remain
off the table until T-30.

Kernel work that does not advance Wave 2s slice acceptance (`PLAN.md` §3 Wave 2s,
thirteen assertions; `docs/07-roadmap.md` §2.3) is inventory, not progress. The
slice gate is `just ci-db` (`justfile`), which runs
`crates/wicket-server/tests/slice.rs`. `just ci` is
`fmt-check clippy lint-sql lint-mounts test-lib`. `test-lib` passes
`--lib` and excludes every integration test under `crates/*/tests/`.
Green `just ci` is not slice acceptance.

## 2. What an agent must not do

| Ban | Evidence |
|---|---|
| Must not open a new crate or module while Goal 2 is false. | The capability table is the mount source. Unmounted `module.toml` routes (T-30) are still declared and not mounted. Adding a handler without a table row, or a table row without a handler, is fail-class. |
| Must not write an unbuilt mechanism in the present tense. | Every sentence is `is`, `must`, or `ABSENT (promised: TODO.md T-NN)`. |
| Must not edit a file another lane owns. | 39 of 41 `integrate: merge` commits in this repository are tagged overlapping ownership. A three-way merge is not an integration strategy. One writer per file per wave. The second lane must escalate and stop. |
| Must not grow the Goal 2 allowlist to park an awkward public function. | `GOALS.md` API-10. Allowlist growth is a change to `GOALS.md`, reviewed as one, not a nit. |
| Must not invent a second source of truth, and must not "fix" drift by editing both sides until they match. | An agent must find which source is authoritative and must delete the other (`docs/adr/0010-one-registry.md`; `GOALS.md` §5). |
| Must not build regulated behaviour before the unsigned slice runs end to end. | `PLAN.md` §0 item 3; `docs/07-roadmap.md` §6.1. Wave 2s before Wave 2b. The slice is unsigned by declaration. |

## 3. Ticket card

A lane that cannot fill every row must not start.

| Field | What it holds |
|---|---|
| Outcome | User-visible result in one sentence. |
| Profiles | Both `regulated-device` and `plain-shop` named (`PLAN.md` §1a). |
| Invariant | Which frozen invariant it touches, by number (`PLAN.md` §6). |
| Goal 1 | Answer, or `none` with a reason (`GOALS.md` §1). |
| Goal 2 | Answer, or `none` with a reason (`GOALS.md` §2). |
| Goal 3 | Answer, or `none` with a reason (`GOALS.md` §3). |
| Goal 4 | Answer, or `none` with a reason (`GOALS.md` §4). |
| Owns | Exclusive list of files this lane writes. An empty list is not a start. |
| Does not claim | What this change leaves untrue. |
| Green command | The exact command that must pass. Slice work is `just ci-db`. `just ci` is not that command. |
| Docs | The documentation patch in the same change if an operator or contributor is otherwise misled (`GOALS.md` DOC-4). |

The four-goal CI check on pull-request bodies is `scripts/check-pr-goals.sh`
(wired in `.github/workflows/ci.yml` job `lint-policy`). A pull request whose
body omits a goal heading fails the build (`GOALS.md` GOV-1).

## 4. Frozen decisions

An agent that wants to violate one opens an ADR (`docs/adr/README.md`). It must
not do it "just for now". Proposed means the current recommendation, and the
docs are written as though it holds (`docs/adr/README.md`).

| Decision | Source | Status |
|---|---|---|
| Modular monolith: one process, one binary, one database, one transaction boundary. | `docs/adr/0001-modular-monolith.md` | Proposed |
| One binary, two installation profiles (`regulated-device`, `plain-shop`). A profile is runtime enablement of compiled-in modules, never a second build. | `PLAN.md` §1a; `docs/07-roadmap.md` §4; DECISION D-W1-5 | Frozen in the plan. No ADR. |
| PostgreSQL is the only database. The operating system owns the process lifecycle. Wicket never owns a database process, on any OS, in any version. | `docs/adr/0003-database.md` | Accepted, as amended |
| Append-only ledger. No stored balances. On-hand is a fold over postings. | `docs/adr/0004-append-only-ledger.md`; `PLAN.md` §6 invariant 1 | Proposed |
| Audit trail is a kernel row trigger. The application role holds select on the audit table and holds no insert, update, or delete. | `docs/adr/0005-compliance-in-kernel.md`; `PLAN.md` §6 invariant 3 | Proposed |
| Signature requirements are module-owned. Modules declare; they do not implement signing. A profile must not add, remove, or downgrade a requirement. Enablement is the only lever a profile has. | `docs/adr/0005-compliance-in-kernel.md`; `PLAN.md` §1a; `docs/07-roadmap.md` §4 | ADR Proposed; profile rule frozen in the plan |
| AGPL-3.0-or-later. Contributions under the Developer Certificate of Origin, version 1.1. No contributor licence agreement. | `docs/adr/0006-license.md` | Accepted |
| Not a general ledger. Export, do not bookkeep. | `docs/adr/0007-defer-general-ledger.md`; `docs/01-vision-and-scope.md` §6 | Proposed |
| Not product lifecycle management. The Design History File belongs somewhere else. | `docs/01-vision-and-scope.md` §6 | Vision non-goal. No ADR. |
| Not a manufacturing execution system. Consume machine data. Do not be the machine controller. | `docs/01-vision-and-scope.md` §6 | Vision non-goal. No ADR. |
| Not multi-tenant software as a service. One customer per installation. | `docs/adr/0008-single-tenant.md`; `docs/01-vision-and-scope.md` §6 | Proposed |

PLM and MES have no ADR. An agent that wants to build either opens one. Absence
of an ADR is not permission.

## 5. Already true

An agent proposing to clean any of these has not read the tree.

| Fact | Consequence |
|---|---|
| Zero `datum` matches in tracked files. | The rename is complete. |
| Zero `todo!()` or `unimplemented!()` in tracked Rust. | The placeholder sweep is complete. `scripts/lint-unimplemented.sh` (T-14) is the scanner. |
| Idempotency is implemented. | An agent must not rebuild it. |
| `If-Match` is implemented (`crates/wicket-server/src/extract.rs:105-131`). | An agent must not rebuild it. |

`RATE_LIMITED` is named in `docs/10-api-conventions.md` and is never emitted
(`GOALS.md` AG-9). Emitting it is ABSENT (promised: `TODO.md` T-37). An agent
must not describe the rate limit as live.

An agent must read `README.md`, `GOALS.md`, and `docs/07-roadmap.md` before
writing a word. `TODO.md` Stage 1 (T-20–T-27) has landed. Stage 2 coverage (T-30) is next:
mount the module reads and writes that already have handlers.
