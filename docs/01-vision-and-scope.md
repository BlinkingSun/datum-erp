# Vision and Scope

Conforms to: [ADR 0003](adr/0003-database.md) (Accepted, as amended), [ADR 0006](adr/0006-license.md) (Accepted), [ADR 0007](adr/0007-defer-general-ledger.md) (Proposed), [ADR 0008](adr/0008-single-tenant.md) (Proposed); `09-workspace-contract.md`.

Audience: contributor, quality. Status: partial.
Living product vision. Kernel crates exist. Beachhead modules and the ten-minute install
are unbuilt. Settled decisions live in `adr/`. Open questions are in §8.

---

## 1. The name

Chosen name: **Wicket**.

Wicket ERP is the product name (chosen 2026-09-13). A wicket is a small gate — the
passage into the shop's system of record. The word is uncommon enough to search for.

An earlier claim that the name lived in exactly one constant, so it stayed cheap to
change, is false since the rename. Crate names, schema names, and documentation all
say Wicket.

Alternatives considered:

| Name | For | Against |
|---|---|---|
| **Wicket** | Short, searchable, gate metaphor | Cricket association outside the US — chosen 2026-09-13 |
| **Mandrel** | The shaft everything is built around, matches a core-plus-modules design | Less obvious meaning outside machining |
| **Tracewright** | Traceability plus maker, unique and searchable | Invented, slightly precious |
| **Arbor** | Machining term, suggests a tree of modules | Several existing projects use it |

## 2. What this is

An open source ERP for discrete manufacturing, built so that regulated manufacturers
can use it without buying a second system, and built so that unregulated
manufacturers never have to see the regulated parts.

Three claims, in priority order.

**It is modular in a way that matters.** The core knows about items, inventory,
orders, and work. It does not know what a medical device is. Regulated workflows, and
eventually anything specific to any industry, are modules that can be switched off.
Regulated record properties are kernel, and they stay on. A job shop making motorcycle
brackets and a contract manufacturer making bone screws run the same core with
different modules enabled.

**It treats compliance as architecture rather than as a feature.** Audit trails,
electronic signatures, record immutability, and lot genealogy are properties of the
kernel, present from the first commit, applying automatically to every module
including ones nobody has written yet. This is the one thing that cannot be retrofitted.
No open source ERP has a compliant electronic signature. The closest comparable
project's change tracking is opt-in per record type, and its history rows are
deletable by an administrator. See `08-competitive-landscape.md` §2.1 (ERPNext
`Version` is opt-in per DocType and Administrator may delete those rows).

**It installs next to a PostgreSQL the operating system already owns.** One binary per
platform, on macOS, Windows, and Linux. On a machine that already runs PostgreSQL 17,
a shop installs Wicket and is entering data in under ten minutes, with no database
administrator and no container runtime. On a bare machine the honest number is thirty
minutes, because PostgreSQL is installed first from its own platform installer. An
evaluator who wants to see the product before installing anything runs one downloaded
file and is looking at seeded data in under five minutes, on a throwaway database that
cannot become a production one.

## 3. Who it is for

**Beachhead: the 10 to 100 person medical device manufacturer or contract
manufacturer.** Machined implants and instruments, single-use disposables, small
electromechanical devices. Almost certainly ISO 13485 certified, probably FDA
registered, probably making Class I and Class II devices.

This shop today is doing one of four things, all of them bad.

1. Running a general ERP that has no quality module worth the name, plus a separate
   electronic quality system, plus spreadsheets to reconcile them.
2. Running a paper quality system alongside an ERP, which means the audit trail lives
   in binders.
3. Paying for an integrated commercial suite. Counted contracts put Arena at about
   $48,700 a year, Greenlight Guru about $44,000, and MasterControl about $115,700.
   See `08-competitive-landscape.md` §3.1 (Vendr medians in
   `research/background/competitive-landscape.md` §7.1).
4. Running entirely on spreadsheets and hoping the next audit goes well.

**Why this beachhead.** It is the hardest target, and hard targets are defensible. A
system that satisfies an FDA investigator satisfies an aerospace auditor, an
automotive customer, and a job shop that just wants to know what things cost. The
reverse is not true. Building the easy case first and hardening it later means
rewriting the foundation, because audit trails and immutability are not features you
can add.

It is also the segment with the worst options. General open source ERPs are weak here
by design, and the commercial systems that do serve it are priced for companies three
times this size.

**Adjacent targets, reachable with the same core and different modules:** aerospace
and defense shops under AS9100, general job shops and contract machining, and small
regulated food or cosmetics operations.

**Explicitly not the target, at least at first:** process and batch manufacturing
based on recipes and yields rather than bills of material, high-volume automotive with
EDI-heavy supply chains, and enterprises above roughly 500 people, where the
requirement is multi-plant, multi-currency, and multi-entity consolidation.

## 4. The central bet

> A thirty-person medical device shop should not have to buy an ERP and an eQMS and
> then reconcile them by hand.

The seam between those two systems is where compliance work actually goes to die. The
ERP knows a work order was completed. The quality system knows an operator was
trained. Neither knows whether the operator who ran that work order was trained on
that operation on that date, because that question spans both systems and the answer
lives in a spreadsheet somebody maintains.

Every question an auditor asks spans the seam. Closing it is the product.

## 5. What makes this different from what already exists

A fuller competitive assessment lives in `08-competitive-landscape.md`. The short
version of the design consequences:

**Against general open source ERP.** Quality is deeper than what already ships, not
absent from it. ERPNext ships quality inspection and non-conformance in core. Odoo
Enterprise has a quality app. Tryton has one. Ours is meant to go further: CAPA, a
Device History Record assembled from the same postings, training veto, calibration
impact. That depth is a Phase 4 promise rather than a present fact. Audit trail and
electronic signature are kernel properties rather than add-ons. Lot and serial
genealogy is a graph over the ledger rather than a report you write yourself. ERPNext
already ships a serial and batch traceability report. The graph is a better design,
not a missing capability.

**Against commercial regulated suites.** Open source, self-hosted, no per-seat pricing,
and no vendor holding a shop's quality records hostage. The validation package ships
with the release rather than costing a five-figure consulting engagement.

**Against building it yourself in spreadsheets.** It survives the audit.

**CAD-native estimating is a genuine later-phase capability, not an unfair advantage.**
This project is being started by someone who already builds CAD tooling: mesh to B-Rep
conversion, STEP to DXF profile extraction, and a CAD application. A CAD-native ERP that
can ingest a STEP file, pull features and material volume out of it, and use that to
seed an estimate remains a genuine capability for an open source, self-hosted system.
It is a Phase 7 module. It is not an unfair advantage. Paperless Parts already owns
CAD-to-estimate for job shops and medical device contract manufacturers, and it already
integrates with JobBOSS, ProShop, and Epicor. The core should not make the module hard.
See the `cad` module in `04-module-catalog.md`.

## 6. Non-goals

Stating these plainly now saves arguments later.

- **Not a full accounting system, at least not first.** Export to QuickBooks, Xero, and
  Sage instead. See `adr/0007-defer-general-ledger.md`.
- **Not a PLM or design control system.** The Design History File belongs somewhere
  else. Integrate.
- **Not an MES.** Second-by-second machine control is a different product with
  different latency requirements. Consume machine data, do not try to be the machine
  controller.
- **Not multi-tenant SaaS.** See `adr/0008-single-tenant.md`. Regulated
  customers self-host, and rolling updates actively conflict with a validated
  installation.
- **Not a framework.** It is an application that happens to be extensible. Projects
  that set out to be platforms first never ship the application.
- **Not configurable in every dimension.** Infinite configurability is how ERP
  implementations turn into two-year consulting engagements. Strong opinionated
  defaults, extension where it genuinely varies, and a clear line between them.

## 7. What success looks like

Concrete tests, in rough order of when they become answerable.

1. **Install.** On a machine that already runs PostgreSQL 17 (the minimum; 18 is
   tested — `09-workspace-contract.md`), a shop installs Wicket and is entering
   data in under ten minutes, on any of three operating systems,
   with no database administrator and no container runtime. On a bare machine the honest
   number is thirty minutes, because PostgreSQL is installed first from its own platform
   installer — one administrator prompt on Windows, one package manager command on
   Linux, one Homebrew formula on macOS — and the shop is told exactly which version,
   which download, and what to click. Wicket's first-run wizard does every remaining step
   itself: it finds the cluster, creates the database, the roles and the grants, runs the
   migrations, and verifies them. Nobody writes a connection string and nobody runs
   `psql`. An evaluator who wants to see the product before installing anything runs one
   downloaded file and is looking at seeded data in under five minutes, on a throwaway
   database that is built so it cannot become a production one.
2. A machinist can log in at a work order, scan a traveler, log time, and report scrap
   in under fifteen seconds, on a tablet, with gloves on.
3. Given a finished device serial number, the system produces the complete genealogy,
   including every material lot, every operator, every gage, and every inspection
   result, in under one minute.
4. The system passes a mock FDA inspection of its audit trail and electronic signature
   controls conducted by someone who does this professionally.
5. A third party writes and ships a module the core team has never seen, without
   forking.
6. A shop that is not regulated at all runs it happily and never encounters a quality
   screen.

## 8. Open questions

Things that genuinely are not decided. Tracked here rather than pretended away.

- **Where the eQMS boundary actually sits.** Document control and training records are
  clearly in. Design controls and the Design History File are probably out. Internal
  audit management is genuinely unclear.
- **How modules are distributed.** Compiled in, or loaded at runtime. See
  `03-module-system.md`.

Settled, and not reopened here: the product name is Wicket (§1). The license is
AGPL-3.0-or-later. Contributions are under the Developer Certificate of Origin,
version 1.1. There is no contributor license agreement. See `adr/0006-license.md`
(Accepted).

---

*Next: `02-architecture.md`.*
