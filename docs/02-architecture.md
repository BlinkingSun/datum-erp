# Architecture

*Status: draft. Each major decision has a corresponding record in `adr/`.*

---

## 1. Shape

A **modular monolith**. One process, one binary, one database, one transaction
boundary. Modules are compile-time units with enforced boundaries, not separate
services.

This is a deliberate rejection of microservices, and the reason is specific rather
than fashionable. An ERP is a single large transactional graph. Completing an
operation on a work order simultaneously consumes material, creates finished
inventory, posts labor cost, advances a state machine, writes an audit record, and may
require an electronic signature. Those either all happen or none of them happen. Across
a service boundary that guarantee costs a distributed transaction protocol, and every
team that has tried it in this domain has regretted it.

The monolith is also what makes the ten-minute install possible. There is nothing to
orchestrate.

```
+-------------------------------------------------------------+
|  CLIENTS                                                     |
|  Browser (office)    Tablet (shop floor)    Desktop shell    |
+-------------------------------------------------------------+
                    | HTTP + JSON, described by OpenAPI
+-------------------------------------------------------------+
|  API SURFACE                                                 |
|  Routes registered by modules. One generated TypeScript      |
|  client. Same API for the UI and for third parties.          |
+-------------------------------------------------------------+
|  MODULES            enable / disable, own their own tables   |
|                                                              |
|  items   bom   routing   inventory   parties   sales         |
|  purchasing   production   costing   mrp   scheduling        |
|  inspection   ncr   capa   calibration   genealogy   dhr     |
|  training   change-control   udi   shipping   cad   ...      |
+-------------------------------------------------------------+
|  KERNEL             not optional, applies to every module    |
|                                                              |
|  identity + RBAC      audit trail       electronic signature |
|  ledger engine        state machines    documents + revisions|
|  numbering            units of measure  background jobs      |
|  event bus            custom fields     reporting + print    |
+-------------------------------------------------------------+
|  STORAGE            PostgreSQL                               |
+-------------------------------------------------------------+
```

## 2. The kernel and why it is drawn there

The line between kernel and module is the most important decision in this document.
The rule:

> **Regulated workflows are modules; regulated record properties are kernel.** A module
> can add a process. A module cannot add a property to history.

Everything on the kernel list below is a property of records. Adding it in version 3
would mean rewriting history that already exists. That is the test.

**Identity, authentication, and authorization.** Every action has an actor. There is
no such thing as a system-initiated change with no attributable user, because an
FDA audit trail requires one. Background jobs run as a named service principal.

**Audit trail.** Computer-generated, server-timestamped, independent of the
operator, recording who, what, when, the previous value, the new value, and where
required, why. It is produced by a database trigger attached automatically at table
creation, writing through a security-definer function. The application supplies actor
and business intent transaction-locally, and fails closed when it cannot. A module
author cannot forget to write one, because they never write one, and they cannot write
a false one, because the application role cannot insert into the trail. SQLx has no
interception point that could have done this. This is the single strongest argument
for putting it in the kernel.

**Electronic signature.** A signature is a kernel primitive bound to a specific record
version. Any module can declare that a state transition requires one. The signature
captures the signer, the server time, the meaning of the signature, and a hash of
exactly what was signed, so that later modification is detectable.

**Ledger engine.** Described in section 3. The generic append-only posting machinery
that inventory, cost, and labor are all built on.

**State machines.** Nearly every ERP document is a state machine. Quotes, orders,
work orders, nonconformances, and change orders all move through defined states with
defined permissions and defined signature requirements. Rather than reimplementing
that per module, the kernel provides a declarative engine, which also means transitions
are uniformly audited and uniformly signable.

**Documents and revisions.** Controlled documents with revision history, approval
workflow, and effectivity. Bills of material, routings, work instructions, procedures,
and drawings all use it.

**Numbering.** Gap-free, collision-free, configurable sequences per document type.
Sounds trivial and is not, because a gap in a regulated numbering sequence is a
question you have to answer.

**Units of measure.** Item-specific conversions with explicit precision and rounding
rules. Kernel-level because a conversion bug in a module silently corrupts inventory
everywhere.

**Custom fields.** Regulated shops always need fields nobody anticipated. Making this a
kernel feature means custom fields are typed, validated, and audited like any other
field, instead of becoming an unaudited JSON blob.

**Server-side time.** Time of record is read on the server, inside the trigger. No
client-supplied timestamp is stored as the time of record. The host clock is the
source, and the product says so.

**Identity lifecycle.** Users deactivate. They are never deleted. Identifiers are never
reused. The user record outlives every record it signed.

**Signing session.** A signing credential is separable from the login credential, so a
re-prompt is possible. The continuous period of controlled system access is a real
concept in the auth layer, not a comment in a procedure.

**Master-data effectivity.** BOM, routing, spec, packaging, and label artwork carry
effective-from and effective-to plus revision identity, distinct from record
versioning. A work order snapshots the revision set in force at release.

**Deterministic record rendering.** Signed records are rendered by a versioned service
whose output is reproducible. The UI is not a renderer.

**Content-addressed blobs.** Signed records reference attachments by content hash. A
path someone can overwrite is not a reference.

**Retention and legal hold.** A retention clock, a legal hold, and a no-hard-delete
invariant are enforced at the database layer. Expected life is an item-master
attribute that drives the clock.

**Data-residency boundary.** Tenancy includes a residency boundary. A single global
region with a tenant column cannot satisfy it later.

**Build and configuration identity.** Every record is stamped with the software version
and the configuration version that produced it.

**Declarative configuration.** Customer-specific behaviour is configuration with its
own versioning, approval, and audit trail, not per-customer code.

**Package hierarchy.** Each, inner, case, and pallet, with contained quantity and parent
link, are kernel inventory structure. Identifiers per package level are a module.

**Constrained lot and serial generation.** The kernel mints lot and serial numbers in
`[0-9A-Z-]`, at most twenty characters. Unconstrained identifiers cannot be renumbered
once they are printed on product.

**Expiry precision.** Expiry is stored with a precision discriminator. A date column
invents a day and cannot recover "end of month."

**UDI attachment point.** Lot, serial, and shipment records have a stable place for a
module to attach a UDI. Phase 6 must not have to alter the ledger to store one.

**Event bus, background jobs, reporting, and printing** round out the list as ordinary
infrastructure.

## 3. The ledger engine

This is the heart of the system and the idea most worth getting right.

**Every quantity and every value in the system is the sum of immutable postings.**
There is no `quantity_on_hand` column that gets updated. There is a ledger, and
on-hand is a query against it.

A posting carries:

| Field | Purpose |
|---|---|
| `id` | Immutable identity |
| `group_id` | The atomic transaction this belongs to |
| `ledger` | Which ledger: inventory, cost, labor |
| `posted_at` | **Server** time, never client-supplied |
| `posted_by` | The actor, always present |
| `dimensions` | Typed keys: item, location, lot, serial, work order, cost element |
| `quantity` or `amount` | Signed |
| `source` | The document that caused it |
| `reason` | Required for adjustments |

**Conservation is per balance slice, not a scalar sum over the group.** Quantity is
conserved per `(group, item, unit of measure)`. Value is conserved per `(group,
currency)`. A scalar sum over a group would add feet to eaches and pounds to dollars,
and it would reject a completion that posts four screws, one housing, and one
assembly. Both laws are enforced by the database, not by convention.

A group carries a recorded kind. There are five: `MOVEMENT`, `ADJUSTMENT`,
`TRANSFORMATION`, `VALUATION`, and `REVERSAL`. Dispatch on kind is monotone: a kind may
only add predicates and is never exempted from the base law.

Quantities always move between locations. Virtual locations exist for the cases where
goods appear to enter or leave the world. A receipt is a transfer from `SUPPLIER`. A
shipment is a transfer to `CUSTOMER`. Scrap goes to `SCRAP`. A cycle count correction
moves to or from `ADJUSTMENT` and requires a reason code. Material issued to a job moves
into that work order's `WIP`.

Manufacturing is not a closed system in quantity. A titanium bar becomes five hundred
screws. Quantity may cross an identity boundary only inside a group whose declared kind
is `TRANSFORMATION`, through the `CONSUMED` and `PRODUCED` boundaries. No other kind
may touch those boundaries. Value is the law that survives transformation: cost leaves
work-in-process and enters finished goods, and what remains in work-in-process is the
variance. Every quantity that crosses the seam must carry a value posting that does
not, so the seam where matter may change identity is exactly the seam where value is
forbidden to disappear.

One further constraint is what makes the rest load-bearing rather than decorative.
Matter does not leave a real location unless the costing engine has named the specific
earlier postings it came out of, in amounts that add up to exactly what left, valued at
exactly what the valuation engine posted. That allocation is recorded as an
append-only edge. It is the one place in the design where two numbers are produced by two
engines that could have disagreed. It is also what makes genealogy total: the database
refuses a withdrawal that has no named parents.

Group-level rules cannot catch a miscounted scan. An operator who posts 550 when the
physical count is 500, in a posting path where every derived number is derived from 550,
produces a group that conserves perfectly and is still wrong. The redundant source for
that error is the source document, and it is an application check, not a ledger
invariant.

The consequences are worth spelling out, because they are most of the reason to do it
this way.

- **The audit trail is free and cannot lie**, because the postings *are* the history.
  There is no separate log that could disagree with the data.
- **Nothing is ever deleted or edited.** Corrections are reversing postings. This
  satisfies the FDA requirement that an audit trail never obscure previously recorded
  information.
- **Genealogy is a graph traversal** over postings that already exist, rather than a
  separate tracking system that must be kept in sync, and it is complete rather than
  best-effort, because a withdrawal without named parents is refused.
- **Any balance is reconstructible at any past instant**, which is exactly what an
  auditor asks for.
- **Inventory cannot silently drift within a group**, because a group that loses a
  counterpart, moves matter without cost, or crosses the transformation seam without
  accounting for the value is rejected at commit. Drift across groups, including a
  consistently mistyped magnitude, is bounded and reported, not constrained.

Stored balances still exist, as materialized projections, because summing ten million
rows on every screen is not viable. They are explicitly a cache. They are rebuildable
from the ledger by a single command, and a scheduled job verifies that the cache and
the ledger agree. If they ever disagree, the ledger wins and someone gets paged.

## 4. Module boundaries

Modules must be genuinely separable or the design degrades into a monolith with
folders. Four rules.

**A module owns its tables.** Its migrations create them. No other module may read
them directly. Enforced in review, and enforceable by schema separation and grants.

**Modules talk through published interfaces and events.** A module declares a typed
interface for what it offers and depends on interfaces rather than implementations. If
`costing` needs the standard cost of an item, it calls the `items` interface. It does
not join to the items table.

**Extension happens through declared points, never by modification.** A module may
subscribe to events, register a validation hook that can veto a transition, add fields
to another module's entity through the kernel custom-field mechanism, register routes,
and register navigation entries and UI panels. A module may not patch another module's
behavior. This is the rule that keeps the project from becoming Odoo, where
inheritance chains make it impossible to reason about what any given save actually
does.

**Disabling a module hides it, never destroys data.** Records stay. Routes stop
resolving. This matters in a regulated setting, where deleting a record is not a thing
you get to do.

Full details in `03-module-system.md`.

## 5. Stack

| Layer | Choice | Reasoning |
|---|---|---|
| Backend | **Rust**, Axum, SQLx | One static binary per platform with no runtime for the customer to install, which is most of the cross-platform requirement solved for free. A type system strong enough to make dimensions, the separation of money from quantity, and state transitions checkable at compile time. Units of measure are customer-defined rows, not type parameters. See `adr/0002-backend-language.md`. |
| Database | **PostgreSQL** | Real constraints, real transactions, exclusion and deferrable constraints for the ledger invariants, range types for effectivity, and logical replication for backup. See `adr/0003-database.md`. |
| Web UI | **TypeScript**, React, TanStack Query / Table / Router | ERP is data grids, and TanStack Table is the best answer for grids with virtualization, grouping, and inline edit. |
| Shop floor UI | Same app, separate route tree, touch-first | Different ergonomics, not a different codebase. |
| Desktop shell | **Tauri v2** | Small, uses the system webview, and produces native installers for all three platforms. |
| API contract | **OpenAPI**, generated TypeScript client | Third-party integration is mandatory in this domain. The UI eating its own public API keeps that API honest. |
| Search | Postgres full text first | Do not add a search cluster to a ten-minute install. |
| Reports and print | Server-rendered to PDF | Travelers, certificates, labels, and packing lists must render identically everywhere and must archive. |

The backend language is the one choice here that is expensive to reverse and worth
arguing about before any code exists. The tradeoff is contributor friction, since
Rust's pool is smaller than TypeScript's or Python's. The counter-argument is that the
extension story, rather than the core language, is what determines whether outsiders
can build on it, and a stable HTTP and event API means an external module can be
written in anything.

## 6. Running on three operating systems

The requirement is that it works on macOS, Windows, and Linux. That splits into two
different questions.

**The server** is a single Rust binary cross-compiled for macOS on Apple silicon and
Intel, Windows x64, and Linux x64 and ARM64. No runtime, no interpreter, no container
required. A shop runs it on whatever machine it has.

**The database** is the only real dependency, and it is the thing that used to stand
between us and the ten-minute install. It is PostgreSQL, and it is installed and
lifecycle-managed by the operating system rather than by us. See
`adr/0003-database.md`, which was amended on 2026-09-11 to withdraw the bundling half of
the original decision while keeping PostgreSQL itself. The rule is short:

> We never own a database process lifecycle. On any operating system. In any version.
> We own everything above the socket.

**Both processes are system services**, which is the requirement everything else follows
from. A shop floor tablet that comes back at 6am needs the office machine answering with
nobody logged in, and a process launched by an interactive user dies when that user logs
off. That is true of `postgres.exe` and it is equally true of `datum-server`. So the
installer registers a service, which costs one administrator prompt at install time and
none afterwards — and once that prompt is paid for our own service, having the database
be a service too costs nothing. This is why bundling bought less than it looked like it
did: the elevation was never avoidable for any database choice.

Per operating system, concretely:

- **Windows.** PostgreSQL from the EDB installer, which registers it as a Windows
  Service under its own account. Datum from a signed MSI that registers `datum-server`
  as a Windows Service set to Automatic (Delayed Start) and adds one inbound firewall
  rule. Configuration lives in `%PROGRAMDATA%`, not in one user's profile, so a second
  office login does not get a second installation. PostgreSQL listens on loopback only;
  only `datum-server` binds the LAN, so there is one firewall rule and one exposed
  process. `datum-server` connects to `127.0.0.1` and never to `localhost`, because
  Windows resolves `localhost` IPv6-first and the failed `::1` attempt costs seconds
  against a sub-second scan budget.
- **macOS.** For a Mac serving a shop, PostgreSQL from Homebrew started with
  `sudo brew services start`, which writes a LaunchDaemon that survives logout; without
  `sudo` it writes a LaunchAgent that does not, which is the same defect in a different
  hat. Datum from a signed `.pkg` that installs its own LaunchDaemon. Postgres.app is
  fine for a single-user office desktop and is not the answer for a machine that serves
  tablets, because it stops when the app quits.
- **Linux.** The distribution's PostgreSQL package, then our `.deb` or `.rpm` with a
  systemd unit ordered after it. This is the only platform where the install is genuinely
  under ten minutes from a bare machine, and it is what we recommend to a shop that has a
  choice about where the server lives.

**We own everything above the socket**, and that is where the work went instead. A
first-run wizard probes for a cluster, states the supported PostgreSQL version range,
links the exact download for the operating system it is running on, and then does the
rest itself: creates the database, creates the application and migration roles, installs
extensions, runs the migrations, applies and then verifies the grants that make the audit
store append-only, and confirms the cluster's encoding. Nobody writes a connection string
and nobody runs `psql`. A side effect worth naming: because the postmaster now runs as a
service account that no shop operator logs in as, those grants are a control against
every application user rather than only against our own bugs.

**Three ways in, and only one of them is production.** A single downloadable executable
runs a throwaway demo with an embedded PostgreSQL on an ephemeral loopback port and
seeded data, needing no administrator rights and no container runtime; it refuses to
import real data, has no path to becoming a real installation, expires, and marks every
screen and document as an uncontrolled record. A `docker compose` file exists for
developers and for evaluators who already have Docker. Neither is supported for real use,
and the supported path is the one above.

**Backup ownership splits cleanly.** The customer owns the medium, the schedule, and the
retention period. We own the mechanism, the verification, and the proof: a backup command
that writes a dump plus a manifest of schema version, row counts and the ledger head; a
verify command that restores into a scratch database, re-runs the ledger conservation and
audit chain checks against that manifest, and writes a dated verification record into the
audit trail; a restore command that runs both automatically; and a scheduled job that the
installer registers with Task Scheduler, launchd, or a systemd timer. A regulated
installation additionally configures write-ahead log archiving, because a nightly dump
means a recovery point up to a day old and that is a deviation rather than an
inconvenience. Section 8's commitment to a documented restore drill stands, and is easier
to keep against a cluster with ordinary tooling than against one hidden inside an
application.

**The clients** are a browser for office users and a browser in kiosk mode on a tablet
for the floor. Scanners present as keyboards, so the floor terminal needs nothing a
browser cannot do. A native desktop shell is not part of version 1; it becomes worth
building only if a shop needs hardware a browser cannot reach, such as a serial-port gage
or a direct label printer driver.

Typical topology for a thirty-person shop: the server binary and the database on one
machine in the office, both running as services, everyone else on the local network in a
browser, three or four tablets at work centers on the shop floor. No internet dependency
for operation, which is a requirement rather than a nicety, because a network outage must
not stop production. The one thing no architecture fixes: if someone powers the office
machine off at night, the shop starts late. That is an operating procedure, and the
deployment document says so.

## 7. Performance targets

Stated now so they can be designed for rather than discovered.

| Operation | Target | Note |
|---|---|---|
| Any interactive screen | Under 200ms at the 95th percentile | Grids are virtualized and paginated server-side |
| Shop floor scan to confirmation | Under 500ms | Slow scanning is the top cause of abandoned data entry |
| Full MRP run, 10k items, 50k postings | Under 60 seconds | Runs as a background job, never blocks a user |
| Genealogy trace, full tree | Under 10 seconds | Recursive CTE with a materialized closure if needed |
| Ledger rebuild, 10M postings | Under 10 minutes | Offline maintenance operation |

## 8. Security posture

The realistic threat model for a shop floor system, which is mostly insider and
accident rather than nation-state.

- Argon2id password hashing, with optional OIDC for shops that have a directory.
- Role-based access control at the level of permission, not table. Permissions are
  declared by modules and composed into roles.
- Every mutation is audited with the actor. The audit store is append-only at the
  database grant level: the application role can read the trail and cannot insert,
  update, or delete an entry. Grants bind the application. They do not bind the person
  who administers the database, who on a self-hosted install is the customer. Datum does
  not claim the trail cannot be altered by someone with administrative control of the
  database server itself. Instead, each transaction is sealed into a hash chain whose
  head is published off the server on a schedule the customer controls, so that any
  later alteration of stored history is detectable by verifying an exported copy
  against those off-server records on a separate machine.
- Electronic signature re-authentication on signing, with configurable inactivity
  timeout, per 21 CFR 11.200.
- Automatic session lock on floor terminals, because shared tablets are the normal
  case and an unattended logged-in session is an audit finding.
- Backups are a first-class feature with a restore drill documented, not an exercise
  left to the customer.

## 9. Testing strategy

Dictated by the domain. Specifics in a later document, but the shape:

- **Property tests on the ledger.** The invariants that conservation holds per balance
  slice and that projections always equal the ledger sum are the highest-value thing
  to test, and they are testable exhaustively with generated transaction sequences.
- **Golden-file tests for costing and MRP.** Known inputs, known outputs, reviewed
  changes.
- **Migration tests that run forward and backward against seeded data.**
- **A published test suite the customer can run as part of their own installation
  qualification**, which turns our test coverage into part of their validation package
  rather than an internal artifact.

---

*Next: `03-module-system.md`.*
