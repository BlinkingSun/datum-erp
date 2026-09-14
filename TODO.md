# TODO

**Conforms to:** [GOALS.md](GOALS.md); [ADR 0010](docs/adr/0010-one-registry.md) (Proposed);
`docs/07-roadmap.md` (wave structure); `PLAN.md` §5, §6, §11.

*No dates. No durations. Progress is a gate that passes (`docs/07-roadmap.md` §5.4).
Sizes are S, M, L and describe scope, not calendar time.*

This backlog exists because the four goals in `GOALS.md` were added to a tree that did
not have them. It was produced by five independent analyses of the repository, each on a
separate slice, and then deduplicated. Where several analyses reached the same work from
different directions, the item says so, because that convergence is the evidence that
the item is real.

---

## How to use this file

**Stage 0 is the contribution product, and it comes first.** It needs no decision
from the owner. It stops documents from lying to contributors now. Do it first.

**Stage 1 is the keystone, and it is blocked on accepting
[ADR 0010](docs/adr/0010-one-registry.md).** Seven separate items from four analyses
collapse into it. Until `module.toml` is the single source of truth and its routes
carry a method, Goal 1 and Goal 2 cannot be worked on independently, and both are
done twice. Stages 2 and 3 must not start before Stage 1 lands.

Stages 2 through 5 are ordered by dependency, not by importance. Stage 5 is the largest
and can only begin once Stage 2 gives it a catalogue to map onto.

---

## Stage 0 — Truth and intake

No dependencies. Every item here is small, and several fix statements in the repository
that are currently false.

| ID | Work | Size | Done when |
|---|---|---|---|
| T-01 | Fill the conduct and security contacts | S | `CODE_OF_CONDUCT.md:39` and `SECURITY.md:9` hold real addresses instead of placeholders; SECURITY states whether GitHub private vulnerability reporting is enabled |
| T-02 | Pull request template carrying the four-goal gate | S | `.github/pull_request_template.md` requires an answer or a reasoned "not applicable" for each goal, plus sign-off and local gate confirmation |
| T-03 | Issue templates and a published label set | S | Bug, feature, module proposal and request-for-comment forms exist; the label set is documented in `CONTRIBUTING.md` |
| T-04 | Sign-off check in CI | S | The workflow fails when any commit in a pull request lacks a `Signed-off-by` line. Required by [ADR 0006](docs/adr/0006-license.md), currently unenforced |
| T-05 | Request-for-comment path | S | A form and an index exist; `CONTRIBUTING.md` states which diffs require one before a pull request |
| T-06 | Goal-heading lint on pull request bodies | S | CI fails a pull request whose body omits a goal heading |
| T-07 | **Repair dangling `_team/` citations in tracked files** | M | No tracked file cites a path under `_team/`. Twenty-plus tracked files currently do, including `PLAN.md` (16 citations) and `docs/01-vision-and-scope.md`, which sources its competitive claim to a file that exists in neither published repository. Either publish the evidence under `research/` or restate the claim inline |
| T-08 | Move the `_team` exclusion into the repository | S | The exclusion lives in a tracked ignore file rather than only in `.git/info/exclude`, so a second contributor cannot accidentally commit their own working directory |
| T-09 | Correct the supported PostgreSQL floor | S | `docs/01-vision-and-scope.md:162` no longer says "16 or later"; the contract's 17 minimum and 18 tested (`docs/09-workspace-contract.md:23-24`) is stated once and consistently |
| T-10 | Close the settled open questions in the vision | S | `docs/01-vision-and-scope.md` §8 no longer lists licence and contributor agreement as undecided; [ADR 0006](docs/adr/0006-license.md) settled both. Section 1's claim that the product name lives in one constant is removed, having been false since the rename |
| T-11 | Stamp audience and status on every document | S | Each document declares operator, contributor or quality, and shipped, partial, absent or historical. `HANDOFF.md:14-16` says no code has been written while `HANDOFF.md:23` says a wave is integrated; both cannot be current |
| T-12 | Correct the stale hosted-CI wording | S | The workflow comment and `HANDOFF.md:28` agree with `PLAN.md:591-593`. Actions are on and outside pull requests already get Linux CI |
| T-13 | Licence identifier lint and backfill | S | Every source file carries its identifier and a lint enforces it. `CONTRIBUTING.md` §9 requires this today and no file complies. Backfill in one maintainer change before enabling the lint |
| T-14 | Unimplemented-macro scanner | S | CI fails on a placeholder macro outside compile-fail fixtures, as `CONTRIBUTING.md` §6 already requires in prose |
| T-15 | The acceptance suite is outside the default gate | S | Either `just ci` (`justfile:392`) gains an integration-test recipe that does not need a database, or `README.md` §5 and `CONTRIBUTING.md` §6 stop calling `ci` the gate. Today `ci` ends in `test-lib`, which passes `--lib` (`justfile:313-314`) and therefore excludes every integration test under `crates/*/tests/`, including `crates/wicket-server/tests/slice.rs` (twenty tests, the Wave 2s acceptance). A contributor running the documented gate never exercises the product's own acceptance script |
| T-16 | The build file describes a passing recipe as expected to fail | S | `justfile` no longer says ci-db is "expected RED until harness lands" (DONE in this change set). Public CI's `just ci-db` is green. The file is owned by another change in this same set; this item is the record, not the edit |
| T-17 | Repository settings contradict the merge policy | S | GitHub `allow_squash_merge` is false, matching `CONTRIBUTING.md` §7 and `GOALS.md` GOV-5. Branch protection on `main` either enforces fast-forward or is recorded as ABSENT. Private vulnerability reporting is on, or `SECURITY.md:9-11` states it is off (see T-01). **Owner action, not a file change:** the repository has `allow_squash_merge`, `allow_rebase_merge` and `allow_merge_commit` all true. The documents describe a rule the platform does not enforce |
| T-18 | One writer per file per wave | S | `CONTRIBUTING.md` and the agent standing orders state one writer per file per wave, and a subsequent wave produces no `integrate: merge` tagged "overlapping ownership". 39 of 41 such merges in this repository's history carry that tag. Concurrent lanes sharing files is the single most frequent integration defect |

---

## Stage 1 — The keystone: one registry

**Depends on:** nothing. **Blocks:** Stages 2, 3 and 5.

Governed by [ADR 0010](docs/adr/0010-one-registry.md). Three analyses found the manifest
drift independently; three found the dropped route method independently.

| ID | Work | Size | Depends | Done when |
|---|---|---|---|---|
| T-20 | Single manifest source. `compiled_in()` reads each `module.toml` rather than embedding a copy | M | — | No inline manifest strings remain in `crates/wicket-module/src/manifest.rs`; a test fails on any drift between the compiled-in catalogue and the file on disk. **Merges three items that three separate analyses raised as the same work** |
| T-21 | Routes carry their method | S | T-20 | `[[routes]]` requires `method`; `ManifestRoute` and `ModuleRoute` store it; the local re-parse in `modules/items/src/lib.rs:158-180` is deleted. **Merges two items raised independently** |
| T-22 | Machines survive registration | M | T-20 | Declared machines are the machines the engine freezes, including not-required reasons; `register()` no longer clears them |
| T-23 | Mount reverse-diff lint | S | — | A lint extracts mounts from the router and compares them to the document table, failing on any difference. This is the interim step [ADR 0010](docs/adr/0010-one-registry.md) explicitly accepts: it makes existing drift visible while the registry is built. It also replaces the circular parity test at `crates/wicket-server/tests/slice.rs:1543-1568`, which compares a table to itself |
| T-24 | The capability table, and a router generated from it | L | T-21, T-23 | One table is the only route source; the three catalogues collapse into it; a hand-written mount fails the lint |
| T-25 | Document generated from the table; parity test reads the router | M | T-24 | The served document's method and path set equals the table, and the test derives one side from the router |
| T-26 | Collapse the duplicate work-order namespace | S | T-23 | One prefix remains; the other is an explicit redirect or gone. `crates/wicket-server/src/http.rs:48-72` currently violates the one-namespace-per-module rule in `docs/10-api-conventions.md:19` |
| T-27 | Module manifest lint | M | T-20 | Fails when a module's crate name, identifier and schema disagree, or when it is missing from the canonical order or either profile |

---

## Stage 2 — Coverage

**Depends on:** Stage 1. Serves Goal 2.

Everything here is a capability that already exists in Rust and cannot be reached over
the wire. None of it is new functionality.

| ID | Work | Size | Depends | Done when |
|---|---|---|---|---|
| T-30 | Mount the module reads and writes that already have handlers | M | T-24 | Item list and patch, location list, tree, patch and deactivate, lot list, serial creation, work-order list, and the genealogy impact and job routes all respond |
| T-31 | Inventory writes | L | T-24 | Issues, moves, adjustments, document read and void are mounted with tests, or their declarations are removed in the same change |
| T-32 | Remaining machine edges | M | T-24 | Item obsolete, work-order cancel, and the document lifecycle edges each have an operation with optimistic concurrency |
| T-33 | Job status, enqueue and cancel | M | T-24 | A job identifier returned by a trace resolves. Today `modules/genealogy` returns one into a route that does not exist |
| T-34 | Kernel administration over HTTP | L | T-24 | Module registry, principals and roles, numbering, units of measure, and audit verification and export are reachable without linking the crate |
| T-35 | Document schemas and per-edge signature meaning | L | T-25 | Every operation carries request and response schemas; the signature meaning equals the edge's own meaning rather than the literal constant stamped on every transition today (`crates/wicket-server/src/openapi.rs:465-470`) |
| T-36 | Cursor pagination and filtering on list operations | M | T-30 | Limit and cursor are honoured; no handler hard-codes a null cursor on a non-empty page |
| T-37 | Rate limit and method-not-allowed envelope | S | — | The documented burst limit returns its error code, and an unsupported method returns the envelope. Both are specified in `docs/10-api-conventions.md` and neither is implemented |
| T-38 | CLI twin table and allowlist | M | T-34 | Every subcommand except process lifecycle has an operation or an allowlist row with a reason |
| T-39 | Custom fields for every entity that has definitions | M | T-24 | No longer restricted to items |
| T-40 | Document attach, link, history and legal hold | M | T-32 | Each public function has an operation or an allowlist row |
| T-41 | Print log and template version bump | S | T-24 | Routed, or made crate-private |
| T-42 | Ledger reversal, balance and projection verification | M | T-31 | No public ledger operation remains without a row or an allowlist entry |
| T-43 | **The Goal 2 gate: capability coverage test** | M | T-24, T-38 | Boots both profiles, walks engine edges, job kinds, module routes and CLI subcommands against the table, and fails on any capability with no row and no allowlist entry |
| T-44 | Golden OpenAPI fixture fails the build on path drift | S | T-23 | A committed fixture of OpenAPI paths is compared to the served document's path set, and the build fails on any extra or missing path. Complements T-23, which diffs router mounts against the document table (`crates/wicket-server/src/http.rs:18-128` vs `crates/wicket-server/src/openapi.rs:29-397`) rather than against a fixture. ABSENT today: the parity test at `crates/wicket-server/tests/slice.rs:1543-1568` compares a table to itself |

---

## Stage 3 — Agent affordances

**Depends on:** Stage 1. Serves Goal 1.

| ID | Work | Size | Depends | Done when |
|---|---|---|---|---|
| T-50 | Domain error tokens | M | — | An agent can distinguish a held lot from a malformed identifier without parsing English. Additive as a detail code; the envelope class is unchanged |
| T-51 | Legal next actions for a record | M | T-24 | Given a live record, the response lists exactly the outgoing edges legal from its current state, each with permission and signature requirement |
| T-52 | A named agent principal | L | — | A non-human session stamps an actor kind that is not a user, and the audit trail names the agent. Today every bearer session is stamped as a user (`crates/wicket-server/src/session.rs:219-225`) |
| T-53 | Dry run | L | T-51 | An illegal transition returns a structured refusal without writing; a legal one reports what would happen without persisting. **Open question first:** a dry run that skips hooks lies, and one that runs them may write. Decide the hook phase before building |
| T-54 | Module conformance suite | L | T-21, T-22 | An empty module fails it and an existing module passes it. Today a new module is judged by imitation |
| T-55 | Module scaffold generator | M | T-54 | Generates a module that fails its own conformance suite until filled in, and generates no directory the module contract cannot honour |
| T-56 | Publish and hash the introspection snapshot | M | T-24, T-51 | Modules, machines, edges, events, jobs and permissions are enumerable, and the snapshot is hashed into the configuration manifest |
| T-57 | Tool-call surface, optional | M | T-56 | Every tool name equals a mounted operation identifier. Do not start before T-56 or it will duplicate the document badly |
| T-58 | Extend the session-protocol lint to modules | S | — | The first scan in the justfile covers `modules/`, which it currently skips, letting module code evade the fence |
| T-59 | Reconcile the module dependency rule with reality | S | T-05 | `PLAN.md:344` says a module depends only on the module crate's published interfaces; every first-party module depends on six kernel crates. Enforcing the sentence as written would reject every module contribution. Amend the sentence or the allowlist, and say which won |
| T-60 | Resolve the phantom UI slot in the module contract | S | — | `docs/03-module-system.md:32-50` documents a UI route file that no module has. Implement the slot or strike it, because a scaffolding agent will otherwise emit dead files |

---

## Stage 4 — Operator documentation

**Depends on:** Stage 0 for truth repair. Mostly independent of Stages 1 to 3, so it can
run in parallel. Serves Goal 4a.

| ID | Work | Size | Depends | Done when |
|---|---|---|---|---|
| T-70 | Classify every environment variable the source reads | S | — | A table lists each with its path, classified as required at boot, optional at runtime, test-only or compile-time. Note that the test-harness variable is widely misread as a production setting, and that the blob root is the only hard boot failure |
| T-71 | Operator documentation index | S | T-11 | An index maps the topics below to files, and the readme points operators there before the plan and design documents |
| T-72 | Prerequisites | M | T-71, T-09 | Operating systems, database version, and the contributor-only tools named as such |
| T-73 | Provisioning: database, the five roles, and the grants | M | T-72 | Reproduces the development role split with production credentials, and includes a check proving the application role cannot write the audit tables |
| T-74 | Configuration, and an example file that can actually boot | M | T-70 | Every required variable is present. The current example file omits the one variable whose absence is a hard boot failure |
| T-75 | Choosing a profile | M | T-71 | A table generated from the profile files, stating that a required signature edge under a no-signature gate fails at startup |
| T-76 | First boot | M | T-73, T-74, T-75 | Each step carries a command, an expected result and an independent check. Distinguishes the product's migrate subcommand from the contributor recipe, which are different paths and desynchronise schema history if mixed |
| T-77 | Day one on the shop floor | L | T-76 | Receive, build, complete and trace, using only routes that exist, each with an expected status |
| T-78 | Day two: backup, restore, upgrade | M | T-76 | Uses the tools that exist today and names both the cluster and the blob root. A database-only restore is stated to be incomplete. The promised backup command is tagged absent |
| T-79 | Qualification, without overclaiming | M | T-76 | Maps the boot integrity check and the named tests onto qualification vocabulary while quoting the regulatory document's own refusal to claim validation |
| T-80 | Hardening and observability | M | T-73 | Trust model, listening address, blob permissions; and a troubleshooting table keyed by real symptoms |
| T-81 | Documentation lints | M | T-74, T-77 | CI fails when a variable in the source is missing from the configuration canon, when a procedure names a command that does not exist and is not tagged absent, or when a document claims validation |

---

## Stage 5 — Migration

**Depends on:** Stage 2 for a stable API to load through, and a published catalogue to
map onto. Serves Goal 3. This is the largest stage and the only one starting from
nothing.

| ID | Work | Size | Depends | Done when |
|---|---|---|---|---|
| T-90 | Mapping file schema and an empty first pack | S | — | Files validate against a schema; CI fails when a source field has no stated disposition |
| T-91 | Staging schema | M | T-90 | Raw payload, source key, freeze timestamp and extract run identifier, with forward and reverse migrations |
| T-92 | Incumbent extractor | L | T-91 | Authenticates, pins the endpoint and contract version, pages by key, and loads fixtures into staging without needing a live tenant |
| T-93 | Point-in-time quantity snapshot | M | T-92 | A freeze-window snapshot is captured as expected-balance rows |
| T-94 | Load units, items, sites and locations through the public API | L | T-92, T-43 | A fixture round-trips into the real tables through HTTP |
| T-95 | Load lots and serials with the identifier law enforced | M | T-94 | An identifier that fails a kernel law never lands in the kernel field; the original is preserved in a cross-reference |
| T-96 | Opening balances | L | T-93, T-95 | On-hand read back through the API matches the snapshot |
| T-97 | Open work orders | L | T-96 | Counts and issued quantities match the extract; the numbering counter is advanced past the imported maximum |
| T-98 | Custom field mapping | M | T-94 | Every incumbent attribute appears in the pack, mapped or explicitly dropped |
| T-99 | Imported evidence path | M | T-94 | Incumbent attachments and sign-off records land as evidence documents. **Zero rows are created in the signature table.** This is the line that must not be crossed |
| T-100 | Reconciliation report | M | T-96, T-97 | Quantity, open work orders, lot status and genealogy completeness produce a pass or fail; under the regulated profile an unexplained genealogy gap fails |
| T-101 | Cutover runbook | S | T-100 | Dry run, parallel run with the incumbent still the system of record, freeze, go-live, and a rollback that is simply staying on the incumbent |
| T-102 | Out-of-scope register | S | T-90 | Published in the run manifest up front. Includes the general ledger, credentials, live signatures, and every catalog-later module |
| T-103 | Adapter boundary and a second-incumbent skeleton | M | T-92 | Incumbent-specific strings appear only under that adapter's directory, enforced by a lint; the first incumbent is the only complete implementation |
| T-104 | Optional history replay | L | T-96, T-100 | Behind a flag, off by default except where the regulated profile requires genealogy on stock still held |

---

## Decisions the owner still owes this backlog

These block specific items and cannot be resolved by a contributor.

1. **Accept or reject [ADR 0010](docs/adr/0010-one-registry.md).** All of Stage 1 hangs
   on it. Rejecting it means Goals 1 and 2 are pursued separately and some work is done
   twice; that is a legitimate choice, but it should be a chosen one.
2. **The agent actor model.** A new actor kind is clearer in a regulatory review; reusing
   the service principal with an acting-for field is a smaller change. Blocks T-52.
3. **Which internal functions are exempt from Goal 2.** The allowlist is the honesty
   mechanism. If it becomes a junk drawer, Goal 2 is dead. Blocks T-43.
4. **Whether install-time subcommands belong on the API at all,** given they run before
   the server binds. Blocks T-38.
5. **Opening balances only, or full history replay,** as the migration default. Regulated
   shops need genealogy for stock they still hold, and full replay may be too large to
   post through the API. Blocks T-104.
6. **Whether to wait for a bill-of-materials module** before claiming manufacturing
   migration. Without one, product structure cannot come across at all. This is the
   largest functional hole in Goal 3.
7. **How an outside pull request reaches the private mirror,** given the standing law
   that the mirror receives landings first. Blocks the merge policy in T-05.
8. **Repository merge settings.** `CONTRIBUTING.md` §7 and `GOALS.md` GOV-5 state
   squash-merge is disabled. The GitHub repository has `allow_squash_merge`,
   `allow_rebase_merge` and `allow_merge_commit` all true. The documents describe a
   rule the platform does not enforce. This is an owner Settings change, not a file
   change. Also decide: branch protection on `main` so the fast-forward rule is
   real, and whether private vulnerability reporting is on. Blocks T-17.
9. **Whether `just ci` must run the acceptance suite.** `justfile:392` ends in
   `test-lib`, which passes `--lib` (`justfile:313-314`) and excludes every
   integration test under `crates/*/tests/`, including
   `crates/wicket-server/tests/slice.rs`. Either `ci` gains an integration-test
   recipe that does not need a database, or the documents stop calling `ci` the
   gate. Blocks T-15.
