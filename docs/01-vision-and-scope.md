# Vision and Scope

*Status: draft for discussion. Nothing here is settled until it has an ADR.*

---

## 1. The name

Working name: **Datum**.

In geometric dimensioning and tolerancing, a datum is the reference feature every
other measurement is taken from. That is exactly what this system is meant to be for a
shop: the single reference that everything else is measured against. It is also the
singular of *data*. The metaphor is machinist-native and the double meaning is
on-target.

It is a placeholder until someone objects. The name appears in exactly one constant in
the codebase so it stays cheap to change. Alternatives considered:

| Name | For | Against |
|---|---|---|
| **Datum** | GD&T term, double meaning, short | Common English word, harder to search for |
| **Mandrel** | The shaft everything is built around, matches a core-plus-modules design | Less obvious meaning outside machining |
| **Tracewright** | Traceability plus maker, unique and searchable | Invented, slightly precious |
| **Arbor** | Machining term, suggests a tree of modules | Several existing projects use it |

## 2. What this is

An open source ERP for discrete manufacturing, built so that regulated manufacturers
can use it without buying a second system, and built so that unregulated
manufacturers never have to see the regulated parts.

Three claims, in priority order.

**It is modular in a way that matters.** The core knows about items, inventory,
orders, and work. It does not know what a medical device is. Everything specific to a
regulated industry, and eventually everything specific to any industry, is a module
that can be switched off. A job shop making motorcycle brackets and a contract
manufacturer making bone screws run the same core with different modules enabled.

**It treats compliance as architecture rather than as a feature.** Audit trails,
electronic signatures, record immutability, and lot genealogy are properties of the
kernel, present from the first commit, applying automatically to every module
including ones nobody has written yet. This is the one thing that cannot be retrofitted
and the one thing every existing open source ERP got wrong.

**It runs anywhere with nothing to install.** One binary per platform, on macOS,
Windows, and Linux. A shop should be able to download a file, double-click it, and
have a working system on a spare machine within ten minutes.

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
3. Paying $40,000 to $150,000 a year for an integrated commercial suite.
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

**Against general open source ERP.** Quality is a first-class subsystem rather than a
bolt-on. Audit trail and electronic signature are kernel properties rather than
add-ons. Lot and serial genealogy is a designed capability rather than a report you
write yourself.

**Against commercial regulated suites.** Open source, self-hosted, no per-seat pricing,
and no vendor holding a shop's quality records hostage. The validation package ships
with the release rather than costing a five-figure consulting engagement.

**Against building it yourself in spreadsheets.** It survives the audit.

**One unfair advantage worth naming.** This project is being started by someone who
already builds CAD tooling: mesh to B-Rep conversion, STEP to DXF profile extraction,
and a CAD application. A CAD-native ERP that can ingest a STEP file, pull features and
material volume out of it, and use that to seed an estimate is something no ERP on the
market does well. That is a later-phase module, but the core should not make it hard.
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
- **Not multi-tenant SaaS.** See `adr/0008-single-tenant-self-hosted.md`. Regulated
  customers self-host, and rolling updates actively conflict with a validated
  installation.
- **Not a framework.** It is an application that happens to be extensible. Projects
  that set out to be platforms first never ship the application.
- **Not configurable in every dimension.** Infinite configurability is how ERP
  implementations turn into two-year consulting engagements. Strong opinionated
  defaults, extension where it genuinely varies, and a clear line between them.

## 7. What success looks like

Concrete tests, in rough order of when they become answerable.

1. A shop can install it on any of three operating systems in under ten minutes with
   no database administrator and no container runtime.
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

- **Name.** See section 1.
- **License.** Recommendation is AGPL-3.0, in `adr/0006-license.md`. Must be settled
  before the first public commit, because relicensing later requires consent from
  every contributor.
- **Contributor agreement.** Developer Certificate of Origin is friendlier and is the
  default recommendation. A Contributor License Agreement is the only way to preserve
  the option of selling commercial exceptions later. This is a business decision rather
  than a technical one, and it cannot be deferred cheaply.
- **Where the eQMS boundary actually sits.** Document control and training records are
  clearly in. Design controls and the Design History File are probably out. Internal
  audit management is genuinely unclear.
- **Whether to bundle a database or require one.** Affects the ten-minute install test
  directly. See `adr/0003-database.md`.
- **How modules are distributed.** Compiled in, or loaded at runtime. See
  `03-module-system.md`.

---

*Next: `02-architecture.md`.*
