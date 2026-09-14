# Project Goals

**Conforms to:** [ADR 0001](docs/adr/0001-modular-monolith.md), [ADR 0005](docs/adr/0005-compliance-in-kernel.md),
[ADR 0006](docs/adr/0006-license.md) (Accepted), [ADR 0010](docs/adr/0010-one-registry.md);
`docs/01-vision-and-scope.md`; `docs/10-api-conventions.md`; `PLAN.md` §5 and §6.

*Status: these are goals, not claims. Section 6 states what is true today, and it is
mostly "not yet". Progress is reported as gates passed, never as calendar time
(`docs/07-roadmap.md` §5.4).*

---

## 0. Why this document exists

`docs/01-vision-and-scope.md` says who Wicket is for and what it refuses to do. This
document says what must be true of **every capability Wicket ever grows**, no matter
which module grows it or who contributes it. The vision can be revised by argument.
These four goals are enforced by tooling, and a change that violates one does not
merge.

The project's own measured law applies here above all:

> Every rule enforced by a tool held. Every rule enforced by prose failed.

So each goal below ends with the mechanism that enforces it. A goal with no
mechanism is an aspiration, and this file does not keep aspirations.

---

## 1. Goal 1 — Wicket is built to be run and extended by an AI agent

Wicket is built so an AI agent can extend it and run it. Every module, permission,
state-machine edge, event, job, and error is enumerable from a frozen, hashed
description that matches the running binary. An operating agent acts as a named
principal, discovers legal next actions before mutating, retries with idempotency,
and leaves a trail that names the agent. A building agent scaffolds a module against
that same contract and receives PASS or FAIL from a conformance suite, never from
folklore.

Two audiences, both agents, and they need different things.

**The building agent** writes a module. It needs a machine-readable module contract,
a scaffold that starts legal, and a conformance suite that fails loudly. Today it
learns by imitating `modules/items` and copying a test name out of
`docs/11-module-common-rules.md:10-12`. Imitation is not a contract.

**The operating agent** runs the shop. It needs to ask what it may do to a record
before it tries, to distinguish "this lot is on hold" from "that string is not a
valid part number", to retry safely, and to be attributable in the audit trail as
itself and not as a person.

### Binding rules

| | Rule |
|---|---|
| **AG-1** | After `Kernel::build`, `kernel.catalog()` for a module MUST equal `ModuleManifest::parse` of that module's `module.toml` on id, version, permissions, `regulated`, jobs, subscriptions, and routes. |
| **AG-2** | `[[routes]]` MUST carry `method`, and `ManifestRoute` MUST store it. Dropping it silently, as today (`crates/wicket-module/src/manifest.rs:107-114`), is fail-class. |
| **AG-3** | `compiled_in()` MUST `include_str!` each first-party `module.toml`. Inline stub manifests are fail-class. |
| **AG-4** | Machines declared in TOML MUST be the machines the engine freezes, including `NotRequired` reasons. Clearing `manifest.machines` in `register()` is fail-class. |
| **AG-5** | Every error variant a caller can act on MUST carry a stable domain token, additive as `error.detail_code`. The envelope `code` stays the `docs/10` class. `LotNotIssuable` and `InvalidTransition` MUST NOT both surface as bare `VALIDATION`. |
| **AG-6** | For every persisted state-machine instance, a published query MUST return the outgoing edges legal from the current state, each with permission and signature requirement. Adding an edge that does not appear there is fail-class. |
| **AG-7** | A mutating request from a non-human principal MUST stamp an actor kind that is not `User`, and MUST carry the agent's identity. Hardcoding `ActorKind::User` for Bearer sessions (`crates/wicket-server/src/session.rs:219-225`) is fail-class once an agent principal exists. |
| **AG-8** | A new first-party module MUST pass the module conformance suite. A suite that cannot fail an empty module is not a suite. |
| **AG-9** | A rate limit, error code, or convention named in `docs/10-api-conventions.md` MUST be emitted by real code or removed from the document in the same change. `RATE_LIMITED` is currently named and never returned. |
| **AG-10** | If a tool-call surface such as MCP is added, every tool name MUST equal an OpenAPI `operationId` already mounted. A tool with no HTTP twin is fail-class. There is one capability set, not two. |

### Enforced by

A catalog-equality test in `crates/wicket-module/tests`; a module-manifest lint in
`scripts/`; a conformance binary wired into `just ci`; an actor-stamp test asserting
the session GUC; and, when a tool surface exists, a generated tool list diffed
against `openapi.json`.

---

## 2. Goal 2 — Every function has an API

Every capability Wicket can perform — module store operation, state-machine edge,
job kind, identity or admin action, and operator CLI verb except process lifecycle —
is reachable through a versioned HTTP operation under `/api/v1/`, declared in one
registry that also generates the router and the OpenAPI document. No function may
exist only as a Rust `pub fn`, a clap subcommand, or a SQL statement. CI proves this
by walking that registry against mounts, machines, jobs, and the CLI. A missing row
fails the build.

This goal is currently false, and not marginally. The live surface is a hand-written
table in `crates/wicket-server/src/http.rs:18-128`; the OpenAPI document is generated
from a **second** hand-written table in `crates/wicket-server/src/openapi.rs:29-397`;
and the module manifests declare a **third** set that the server never reads. The
existing parity test compares the served document to `mounted_operations()` with both
sides derived from the same table, so it cannot detect drift from the router at all.

Capabilities that exist in Rust and cannot be reached over the wire today include
listing items, patching a location, listing lots, moving or adjusting stock, voiding a
document, obsoleting an item, cancelling a work order, inspecting a job, verifying the
audit chain, and every identity, numbering, unit-of-measure and module-registry
operation.

### Binding rules

| | Rule |
|---|---|
| **API-1** | Every public capability is one row in a single compiled capability table: id, kind, method, path, permission, and the machine edge, job kind or CLI verb it fulfils. |
| **API-2** | The HTTP router is built only from that table. A hand-written route that the table did not generate is fail-class. |
| **API-3** | The served OpenAPI path and method set equals the table. Extra or missing entries fail the parity test, and that test MUST read the router, not a second copy of the same list. |
| **API-4** | Every collection resource exposes a cursor-paginated list per `docs/10-api-conventions.md`. A hard-coded `next_cursor: null` on a non-empty page is fail-class. |
| **API-5** | Every frozen machine edge has an HTTP operation with `If-Match`, and where a signature is required, the signature meaning MUST be the edge's own meaning and not a literal constant. |
| **API-6** | Every job kind is inspectable over HTTP. User-triggerable kinds also have enqueue and cancel. A job id returned into a route that does not exist is fail-class. |
| **API-7** | Every CLI subcommand except process lifecycle has an HTTP twin or an allowlist entry carrying a one-line reason. |
| **API-8** | A module may not declare a route the process does not mount. |
| **API-9** | One namespace per module. A second prefix is either absent or an explicit redirect row. The duplicate work-order prefixes at `crates/wicket-server/src/http.rs:48-72` are fail-class. |
| **API-10** | Kernel-internal functions live on an allowlist file with a stated reason. **The allowlist is the honesty mechanism for this goal: if it becomes a junk drawer, Goal 2 is dead.** Growth of the allowlist is reviewed as a change to this document. |
| **API-11** | OpenAPI request and response bodies are generated from the same Rust types the handler uses. |

### Enforced by

A mount lint in the shape of the existing `scripts/lint-sql-*.sh`; a capability
coverage integration test that boots both profiles and walks engine edges, job kinds,
module routes and CLI subcommands against the table; and a build-time generation step
so the router and the document cannot diverge, because there is only one source.

---

## 3. Goal 3 — A shop can migrate off its incumbent ERP, Acumatica first

A discrete manufacturer running Acumatica must be able to move onto Wicket by
following a versioned, testable migration: extract through the incumbent's API,
review a field-mapping artifact, load only through Wicket's public APIs, and prove
on-hand quantity, open work orders, and lot genealogy against a freeze-window
snapshot. Incumbent identifiers are preserved where they satisfy Wicket's item, lot
and location laws; otherwise they are cross-referenced, never silently coerced.
Electronic signatures and audit rows created in the incumbent are retained as
attested documents, never re-minted as Wicket signatures. The pipeline is
incumbent-agnostic. Acumatica is the first adapter, not the shape of the design.

Nothing of this exists yet. A repository-wide search finds no import code, no
mapping artifacts and no staging schema. This goal is documentation debt on a green
field, which is the cheapest moment to set its rules.

Two facts constrain it permanently. On-hand is a fold over the append-only ledger and
not a stored balance (`modules/inventory/src/store.rs:530-547`), so fidelity is
proven by rebuilding the projection and comparing, not by copying quantity columns.
And the server owns `posted_at` and `signed_at`, so incumbent timestamps are
provenance, never legal time of record.

### Binding rules

| | Rule |
|---|---|
| **MIG-1** | Every mapping lives in a versioned file under `migrate/maps/<incumbent>/`, and every source field resolves to exactly one disposition: mapped, moved to a custom field, dropped, or rejected. A reviewer can grep any field and find its fate. |
| **MIG-2** | Loaders insert only through published HTTP APIs. Direct insert into the ledger, the signature table or the audit table is forbidden to importer code. |
| **MIG-3** | Identifier policy is preserve-human, mint-UUID. Human identifiers are kept when they satisfy the kernel's charset and length laws; surrogate keys are always generated. After a preserving load, the numbering counter is advanced past the imported maximum. |
| **MIG-4** | An incumbent identifier that fails a kernel law is never coerced into the kernel field. The original is retained in a cross-reference field and the kernel identifier is generated. |
| **MIG-5** | Units of measure and their conversions load and reconcile before any quantity posts. |
| **MIG-6** | Incumbent sign-offs MUST NOT be inserted as Wicket signatures. They load as imported evidence documents with a stated reason. A regulated cutover is not complete until a quality lead signs a reconciliation report inside Wicket. |
| **MIG-7** | Every extract pins endpoint, contract version, company, branch and freeze timestamp in a run manifest. Paging is keyset-based. |
| **MIG-8** | Reconciliation is mandatory and mechanical: on-hand by item and location, open work orders, lot status, and genealogy completeness. Under the regulated profile, an unexplained genealogy gap fails the migration. |
| **MIG-9** | Incumbent-specific code lives only under that incumbent's adapter directory. A second incumbent adds an adapter and a map pack, never a fork of the loader. |
| **MIG-10** | What is not migrated is published up front in the run manifest, not discovered at cutover. |
| **MIG-11** | The importer is `wicket import`. It is never `wicket migrate`, which applies schema. A demo installation refuses import. |

### Known ceiling

There is no bill-of-materials module, so master BOM data has nowhere native to land
(`docs/04-module-catalog.md:87-91` places `bom` in Phase 3). Open work orders can
migrate; product structure cannot. Parties, purchasing, sales and shipping are
likewise catalog-only. Goal 3 is capped at inventory, lots, genealogy and open work
orders until those modules exist, and the out-of-scope register says so plainly.

---

## 4. Goal 4 — Implementation and contribution are both followable

Two halves, one goal: a stranger can install this, and a stranger can improve it.

### 4a. Implementation

Wicket is installed and operated from written procedures that a manufacturer, or an
agent acting for one, can follow from a supported operating system and PostgreSQL to
a verified plant, without inventing connection strings, roles or checkpoints. Every
capability the shop uses is reached by a documented command or HTTP call with an
expected result. Documentation states what the binary does today and labels promised
work as absent until it ships. Installing Wicket never completes the customer's
validation, and no document may imply that it does.

| | Rule |
|---|---|
| **DOC-1** | Every operator step carries four fields: the literal command, the expected result, an independent check that must succeed, and where to go on failure. "Configure as appropriate" is fail-class. |
| **DOC-2** | One configuration canon lists every environment variable the source actually reads, each classified as required at boot, optional at runtime, test-only, or compile-time. The example environment file is a strict subset of it. Today that file cannot boot the product. |
| **DOC-3** | Production procedures never use the development password, trust authentication, or a credential-less bootstrap URL. Those appear only under an explicit development-only heading. |
| **DOC-4** | A sentence naming a file, subcommand or route is false unless that path exists or is tagged absent with a pointer to where it was promised. |
| **DOC-5** | Every document declares its audience and its status. Planning-era documents are marked historical rather than left to contradict the tree. |
| **DOC-6** | Qualification documents may not use "validated", "compliant" or "certified" as product claims. The boot integrity check is named as such and never as an installation qualification protocol. |
| **DOC-7** | Backup procedures name both the database cluster and the blob root. A database-only restore is documented as an incomplete restore. |
| **DOC-8** | The supported PostgreSQL floor is stated once and correctly. `docs/01-vision-and-scope.md:162` currently says 16; `docs/09-workspace-contract.md:23-24` says 17 minimum and 18 tested. The contract wins. |

### 4b. Contribution

Wicket accepts outside work only when the change stays true to Goals 1 through 3.
Every change is classified as a pull request, a request for comment, or an
architecture decision record, and that class is enforced by templates and CI rather
than by reviewer memory. Third-party modules enter as compiled-in workspace crates
that pass the same manifest, migration, audit and route bars as first-party modules.
A single maintainer merges only fast-forward history after local validation, and
hosted CI never replaces the three-machine gate for kernel and schema changes.

| | Rule |
|---|---|
| **GOV-1** | Every pull request MUST answer all four goals explicitly, or state why each is not applicable. A missing answer MUST fail the build once T-06 lands; until then the maintainer enforces it. |
| **GOV-2** | Every commit MUST carry a Developer Certificate of Origin sign-off, and CI MUST check it (T-04, not yet wired). Settled by [ADR 0006](docs/adr/0006-license.md) and not reopened by a pull request. No contributor licence agreement will be asked for. |
| **GOV-3** | Every source file MUST carry its licence identifier, and a lint MUST enforce it (T-13). No file complies today, so this is a backfill before it is a gate. |
| **GOV-4** | The change class is decided by the diff, not by the author. Touching a public route, an event payload, the manifest schema, the module contract, the canonical order or the dependency allowlist requires an accepted request for comment first. A decision expensive to reverse requires its own decision record, never buried in a feature change. |
| **GOV-5** | `main` moves by fast-forward only. Squash-merge is disabled, because it rewrites the signed commits. Hosted CI green is necessary and not sufficient: kernel, SQL and contract changes record a three-machine result. |
| **GOV-6** | A module contribution ships the full set or it does not merge: crate and module identifiers matching the schema name, forward and reverse migrations with a tested reverse, a validation document, routes under its own namespace, a total signature declaration, an entry in the canonical order and in both profiles, and a manifest read from its own file. |
| **GOV-7** | A new third-party dependency is a request for comment plus an allowlist edit in the same change. |
| **GOV-8** | Below 1.0 there is no compatibility promise, but the surfaces in GOV-4 still require a request for comment. From 1.0 the published compatibility rules bind. |
| **GOV-9** | Work suitable for a newcomer is never in the ledger, the audit chain, or the database session protocol. |
| **GOV-10** | Automation decides the mechanical rules. The maintainer decides requests for comment, decision records, the reasons behind a "not applicable", security and conduct. No second-reviewer quorum is required of a solo project. |

---

## 5. How the four goals fit together

They are not four projects. Goals 1 and 2 are **one registry with two readers**.

Make the module manifest the single source of truth, give its routes a method, and
generate the router, the OpenAPI document and the agent introspection surface from
that one table. An agent reads the registry to know what it may do. A coverage test
reads the same registry to prove nothing is missing. A migration maps onto the
catalogue the registry publishes. A pull-request gate walks it to check that a new
capability came with its endpoint.

Three of the five analyses reached the drift between the compiled-in catalogue and
the module files independently, from three different directions. That is the
keystone. It is recorded as [ADR 0010](docs/adr/0010-one-registry.md), and `TODO.md`
sequences the work behind it.

---

## 6. What is true today

Blunt, because the rest of this document is aspiration until these change.

| Goal | Status |
|---|---|
| 1. Agent-guided | **Not met.** No unified machine-readable description; no legal-next-action query; no agent principal distinct from a human; no dry-run. Idempotency and optimistic concurrency **are** implemented and real. |
| 2. Every function has an API | **Not met.** Three disagreeing catalogues. Many implemented capabilities have no route. The parity test is circular and cannot detect router drift. |
| 3. Migration | **Not started.** No import code, no maps, no staging. Capped at inventory and open work orders until a BOM module exists. |
| 4a. Implementation docs | **Not met.** No operator documentation tree. The example environment file cannot boot the product. The supported database floor is stated two ways. |
| 4b. Contribution | **Partly met.** Licence, sign-off, decision records, formatting, lint and test gates exist. Sign-off is unchecked, there are no templates, and a fully green change can still violate Goals 1 through 3. |

*Next: `TODO.md` for the sequenced work, `CONTRIBUTING.md` for how to land it.*
