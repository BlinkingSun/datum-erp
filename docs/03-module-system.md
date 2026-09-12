# The Module System

*This is the document that determines whether anyone else can build on this project.
Everything else is recoverable. Getting extensibility wrong is not.*

---

## 1. What has to be true

Four requirements, and they pull against each other.

1. **A shop can run only what it needs.** A motorcycle bracket shop never sees a CAPA
   screen. A bone screw shop cannot function without one.
2. **A third party can ship a module without forking.** If extending the system
   requires patching core files, every deployment becomes a bespoke fork, and upstream
   improvements stop reaching users. This is the failure mode that has killed more open
   source ERPs than any technical problem.
3. **Modules cannot break each other or the kernel's compliance guarantees.** A badly
   written module may produce wrong numbers in its own domain. It must not be able to
   suppress an audit record, forge a signature, or write an unbalanced ledger group.
4. **The set of installed modules is enumerable and versioned**, because in a regulated
   installation it is part of the validated configuration. A customer must be able to
   produce a signed list of exactly what is running.

Requirement 3 is why the kernel is not itself a module, and requirement 4 is why
runtime plugin loading is more complicated here than in an ordinary application.

## 2. Anatomy of a module

A module is a directory with a manifest. Illustrative shape, not final syntax.

```
modules/calibration/
  module.toml          manifest: id, version, dependencies, permissions, capabilities
  migrations/          owns its tables, forward and backward
  src/
    domain.rs          entities and rules, no I/O
    store.rs           persistence, the only code touching its tables
    api.rs             HTTP routes registered under /api/calibration
    events.rs          what it emits, what it subscribes to
    hooks.rs           validation hooks it registers against other modules
    states.rs          state machine declarations
  ui/
    routes.tsx         pages, registered into the shell
    panels.tsx         panels injected into other modules' screens
    nav.ts             navigation entries, gated by permission
  tests/
  docs/
    validation.md      intended use, requirements, test protocols
```

The manifest declares everything the rest of the system needs to know without loading
code.

```toml
[module]
id = "calibration"
version = "1.2.0"
name = "Gage Calibration"
description = "Calibration schedules and out-of-tolerance impact assessment"

[dependencies]
kernel = "^1.0"
items = "^1.0"
inventory = "^1.0"

[optional-dependencies]
ncr = "^1.0"        # if present, out-of-tolerance findings raise a nonconformance

[permissions]
"calibration.view"    = "View calibration records"
"calibration.record"  = "Record a calibration result"
"calibration.approve" = "Approve a calibration certificate"

[capabilities]
requires-signature = ["calibration.approve"]
regulated = true     # appears in the validation manifest
```

## 3. Extension points

Five, and deliberately only five. Every extension goes through one of them.

### 3.1 Events

Modules publish typed domain events and subscribe to others. Asynchronous by default,
so a subscriber cannot slow down or fail the originating transaction. Delivered at
least once, with subscribers required to be idempotent.

```
inventory.lot_received      genealogy records the receipt edge
production.op_completed     costing posts labor, dhr appends a record
quality.ncr_raised          capa evaluates whether escalation is required
calibration.gage_overdue    production blocks operations requiring that gage
```

This is how most modules should integrate. It is loosely coupled and it cannot break
the publisher.

### 3.2 Hooks

Sometimes a module must participate in a transaction rather than react to it. A hook
runs synchronously inside the originating transaction and may veto.

```
before_transition(work_order, Released -> InProcess)
  -> training module vetoes: operator not qualified for this operation
  -> calibration module vetoes: required gage is out of calibration
```

Hooks are powerful and therefore constrained. A hook may read, may veto with a
structured reason, and may contribute postings to the same ledger group. A hook may
not write outside the transaction, may not perform network I/O, and is subject to a
time budget. A hook that exceeds its budget fails the transaction loudly rather than
silently timing out, because a silently skipped compliance check is worse than an
error.

### 3.3 Custom fields on another module's entity

The classic ERP extension problem. Handled by the kernel rather than per-module, which
keeps added fields typed, validated, indexed where needed, and **audited exactly like
native fields**.

```toml
[[custom-fields]]
entity = "items.item"
key    = "udi_device_identifier"
type   = "string"
label  = "UDI Device Identifier"
validate = "gs1-gtin"
audit  = true
```

The alternative that everyone reaches for first, a JSON blob column, is wrong here.
An unaudited field in a regulated record is a finding.

### 3.4 Routes and API

A module registers HTTP routes under its own namespace. They appear in the generated
OpenAPI document automatically, which means a third-party module gets a typed client
for free and gets documented for free.

### 3.5 User interface

A module contributes full pages under its own route namespace, panels injected into
declared slots on other modules' screens, navigation entries gated by permission, and
dashboard widgets. Slots are declared explicitly by the host module, so a module author
knows what is stable. There is no DOM patching and no template inheritance.

## 4. What is deliberately not an extension point

- **No modifying another module's records directly.** Go through its interface.
- **No overriding another module's behavior.** No inheritance, no monkeypatching, no
  method replacement. If two modules disagree about what should happen, that is a
  design problem to resolve explicitly, not one to resolve by load order.
- **No suppressing kernel behavior.** A module cannot disable audit, weaken a
  signature requirement, or write a ledger group that does not balance. These are
  enforced below the module boundary, not by policy.
- **No unsandboxed access to the database.** Modules get a scoped handle, not a
  connection string.

## 5. Distribution, and the hard tradeoff

How does a third-party module actually reach a running system? Three options, with
real tension between adoption and regulatory fitness.

| Approach | Adoption | Fit for regulated use |
|---|---|---|
| **Compiled in.** Modules are Rust crates; a distribution is a build with a chosen set. | Poor for third parties. Requires a toolchain and a rebuild. | Excellent. The binary is the validated artifact. Nothing can change at runtime. |
| **Runtime plugins.** WebAssembly modules loaded from a directory. | Good. Drop in a file, restart. | Workable but harder. The validated configuration now includes the plugin set, and each plugin needs its own qualification. |
| **External services.** Modules are separate processes speaking the public API and consuming the event stream. | Excellent. Any language. | Good, and honest, because the boundary is visible and the integration is testable. |

**Recommendation, phased.**

- **Phase 1.** Compiled-in Rust crates only. First-party modules ship in the binary.
  This is the simplest thing that can work and it is correct for the beachhead
  customer, who wants a validated artifact rather than a plugin ecosystem.
- **Phase 1, simultaneously.** A genuinely complete public HTTP API and event stream
  from day one, so external integration is possible immediately and in any language.
  This is what makes the project extensible in practice long before a plugin loader
  exists.
- **Phase 3.** WebAssembly plugins, once the kernel interfaces have stabilized enough
  that a plugin ABI is worth committing to. Committing early to an ABI that then has to
  change is worse than not having one.

The important insight is that **the public API, not the plugin loader, is what makes a
project extensible.** A stable, complete, well-documented API means someone can build a
scheduling optimizer, a customer portal, or a machine-monitoring bridge tomorrow,
without a plugin system and without asking permission. Plugin loading is a convenience
that comes later.

## 6. Lifecycle

**Install** runs the module's migrations inside a transaction, registers its
permissions, routes, events, and state machines, and records the module and its version
in the installed-modules table. That table is itself audited.

**Enable and disable** flip a flag. Disabling unregisters routes and hides navigation.
It does **not** drop tables or delete records, because in a regulated system records
are retained regardless of whether the feature that created them is still switched on.
Disabling a module that others depend on is refused with an explanation.

**Upgrade** runs forward migrations and re-registers. Every migration must have a
tested reverse, because a failed upgrade in a production shop at 6am is a real scenario
and "restore from backup" is not an adequate answer when the shop has been running for
three hours since.

**Uninstall** is intentionally not offered in regulated mode. Disable is the operation.

## 7. Compatibility promise

Nobody builds on a foundation that moves. The promise, once version 1.0 exists:

- **Kernel interfaces follow semantic versioning.** A breaking change means a major
  version and a documented migration path.
- **Events are append-only.** Fields may be added. Fields are never removed or
  repurposed within a major version.
- **The HTTP API is versioned in the path** and old versions live for at least one
  full major cycle.
- **Database schemas of first-party modules are not a public interface.** Read them and
  you are on your own. This is stated loudly because the moment third parties depend on
  internal tables, internal refactoring stops being possible.
- **Deprecation runs for one full minor cycle with a runtime warning** before removal.

## 8. The regulated wrinkle

Worth stating separately because it constrains everything above.

In a validated installation, **the set of modules and their versions is part of what
was validated**. Consequences:

- The system must produce a **configuration manifest**: every module, every version,
  every enabled state, hashed and exportable. This becomes an attachment to the
  customer's installation qualification.
- Changing the module set is a **change control event** in the customer's own quality
  system. The software should make that easy rather than pretend it is not happening,
  by recording the change, who made it, when, and against what approval.
- Each first-party module ships **its own validation documentation**: intended use,
  requirements, and executable test protocols. A customer validating only the modules
  they use is doing far less work than one validating a monolith, which is a genuine
  and underrated selling point for this architecture.
- A module marked `regulated = true` in its manifest gets stricter treatment:
  mandatory validation docs, mandatory reverse migrations, and inclusion in the
  configuration manifest.

This is the part where modularity stops being an engineering preference and becomes a
commercial advantage. Validation effort scales with what you turned on.

---

*Next: `04-module-catalog.md`.*
