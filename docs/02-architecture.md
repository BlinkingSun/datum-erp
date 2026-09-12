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

> **If it cannot be retrofitted, it is kernel. If it can be added later without
> touching existing modules, it is a module.**

Everything on the kernel list below has the property that adding it in version 3 would
require rewriting every module written before it. That is the test.

**Identity, authentication, and authorization.** Every action has an actor. There is
no such thing as a system-initiated change with no attributable user, because an
FDA audit trail requires one. Background jobs run as a named service principal.

**Audit trail.** Computer-generated, server-timestamped, immutable, independent of the
operator, recording who, what, when, the previous value, the new value, and where
required, why. It is produced by the persistence layer rather than by module code,
which means a module author cannot forget to write one and cannot write a false one.
This is the single strongest argument for putting it in the kernel.

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

**Every posting group sums to zero.** This is the double-entry constraint, applied to
physical goods. It is enforced by the database, not by convention.

Making it sum to zero requires virtual locations, and this is the trick that makes the
whole design work. A receipt is not an increase from nowhere. It is a transfer from a
virtual `SUPPLIER` location into `RECEIVING`. Scrap is a transfer into a virtual
`SCRAP` location. A cycle count adjustment is a transfer to or from `ADJUSTMENT`, and
the reason code is mandatory. Material issued to a job moves from stock into that work
order's `WIP`. Completion moves finished goods out of `WIP` into stock, and the
residual left in `WIP` is your variance.

The consequences are worth spelling out, because they are most of the reason to do it
this way.

- **The audit trail is free and cannot lie**, because the postings *are* the history.
  There is no separate log that could disagree with the data.
- **Nothing is ever deleted or edited.** Corrections are reversing postings. This
  satisfies the FDA requirement that an audit trail never obscure previously recorded
  information.
- **Genealogy is a graph traversal** over postings that already exist, rather than a
  separate tracking system that must be kept in sync.
- **Any balance is reconstructible at any past instant**, which is exactly what an
  auditor asks for.
- **Inventory cannot silently drift**, because a non-zero group is rejected at write
  time.

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
| Backend | **Rust**, Axum, SQLx | One static binary per platform with no runtime for the customer to install, which is most of the cross-platform requirement solved for free. A type system strong enough to make units of measure, money, and state transitions checkable at compile time. See `adr/0002-backend-language.md`. |
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

**The database** is the only real dependency, and it is the thing standing between us
and the ten-minute install. Three options, in `adr/0003-database.md`. The current
recommendation is to bundle a PostgreSQL binary with the desktop installer and manage
its lifecycle, so that a single-shop install has genuinely nothing to configure, while
still allowing a shop with an existing PostgreSQL server to point at it.

**The clients** are a browser for office users, a browser or the Tauri shell on a
tablet for the floor, and the Tauri shell on a desktop for anyone who wants an icon
rather than a URL.

Typical topology for a thirty-person shop: the binary and the database on one machine
in the office, everyone else on the local network in a browser, three or four tablets
at work centers on the shop floor. No internet dependency, which is a requirement
rather than a nicety, because a network outage must not stop production.

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
- Every mutation is audited with the actor, and the audit store is append-only at the
  database grant level, so the application role cannot update or delete from it.
- Electronic signature re-authentication on signing, with configurable inactivity
  timeout, per 21 CFR 11.200.
- Automatic session lock on floor terminals, because shared tablets are the normal
  case and an unattended logged-in session is an audit finding.
- Backups are a first-class feature with a restore drill documented, not an exercise
  left to the customer.

## 9. Testing strategy

Dictated by the domain. Specifics in a later document, but the shape:

- **Property tests on the ledger.** The invariant that every group sums to zero and
  that projections always equal the ledger sum is the highest-value thing to test, and
  it is testable exhaustively with generated transaction sequences.
- **Golden-file tests for costing and MRP.** Known inputs, known outputs, reviewed
  changes.
- **Migration tests that run forward and backward against seeded data.**
- **A published test suite the customer can run as part of their own installation
  qualification**, which turns our test coverage into part of their validation package
  rather than an internal artifact.

---

*Next: `03-module-system.md`.*
