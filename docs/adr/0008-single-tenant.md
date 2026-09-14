# 0008. Single tenant per installation; residency is an installation property

Audience: contributor. Status: shipped.

Status:   Proposed
Date:     2026-09-11
Decider:  project lead

## Context

`docs/01-vision-and-scope.md` section 6 and `PLAN.md` already treat this as binding:
Wicket is not multi-tenant SaaS. The reasons were that regulated customers self-host,
that rolling updates conflict with a validated installation, and that a company
identifier on every table plus a filter on every query is a permanent tax for a
capability we would not use.

The regulatory spike contradicts the assumption behind that, not the tenancy choice
itself. Section 1.1 item 13: 21 CFR 821.50(b) requires device tracking records kept at
a centralized point in the United States; European customers will require European
residency; 21 CFR 11.10(b) requires the ability to produce a complete copy of one
customer's records. Its words are that a single global region with a tenant identifier
column cannot satisfy any of this later.

Those are two axes. Conflating them is how the original instinct and the spike ended
up talking past each other.

**Tenancy** is how many customers share an installation. One process and one database
for one customer, or many customers in one process and one database distinguished by
a column.

**Residency** is where data physically lives, and whether a boundary exists that can
be drawn around one customer's records. A single-tenant installation can still lack a
named residency. A multi-tenant system can still have one, if isolation is a database
and a region rather than a column.

A third axis is named here so it is not smuggled into this record. **Civil identity
inside hashed structures.** Part 821 requires patient names, addresses, and identifiers
retained for a device's useful life. European customers owe erasure rights. Erasing a
name that sits in a ledger posting or an audit hash chain breaks the chain and the
genealogy edges. That is a data classification problem, not a tenancy problem.

## Decision

**One customer per installation.** Wicket is self-hosted. It is not a multi-tenant
SaaS. One process, one database, one customer. There is no tenant identifier column,
no row-level tenant filter, and no shared control plane that serves two customers
from one cluster.

**The installation is the residency boundary.** An installation is a kernel fact. It
has a stable identity and a declared legal residency, which is configuration the
customer names and which is versioned and audited like any other controlled setting.
Records live in that installation and nowhere else. A complete copy of one customer's
world is a complete copy of one installation. That is how 11.10(b) is satisfied under
this decision: export the installation, not a filtered slice of a shared store.

A United States manufacturer who must keep tracking records at a centralized point
inside the United States puts the installation there. A European manufacturer who
needs European residency puts the installation there. Those are two customers and two
installations. They are not two rows in one database.

A single company that later needs two residencies runs two installations. That is a
second copy of Wicket, not a tenant. Consolidation across those copies is out of
scope, as `docs/01-vision-and-scope.md` already says for multi-entity customers.

**What Wave 1 must contain, even under single tenancy.**

1. **Installation identity and declared residency** in the kernel, as configuration.
   Software version and configuration version stamps on records (spike section 1.1
   item 14) hang off the installation, not off a tenant.
2. **A complete-copy export of the installation**, in human-readable and electronic
   form, that does not depend on a tenant filter. Backup, audit export, and the
   off-server hash-chain anchor already designed in ADR 0003 and ADR 0005 are scoped
   to this boundary. They stay whole-installation. No table is "global across
   customers," because there are no other customers in the database.
3. **No tenant discriminator in the schema.** Adding one so that hosting is easier
   later is the tax this decision exists to refuse, and it is the mechanism the spike
   says cannot provide residency later anyway.

**What is genuinely deferrable.**

- A vendor operating the software for a customer. Self-host is the product. A
  hosted single-tenant cluster, one database per customer in a region the customer
  chooses, is a commercial mode on top of this decision. It does not change the
  schema. It can wait.
- Multi-tenant shared-database SaaS. It conflicts with pinning a validated version,
  and it is the shape that cannot grow a residency boundary later. It is not a later
  phase of this decision. It is a different product.
- A tokenization vault for patient PII. Not this record. See below.

**Patient PII and erasure.** Single tenancy does not solve this. It only prevents two
customers with opposite legal duties from sharing a row store. One manufacturer who
ships a tracked device in the United States and also sells in Europe still has both
duties. Tokenizing civil identity behind a vault, so that erasure replaces a vault
entry and does not rewrite a posting or an audit hash, is its own decision. It must
be written before any module stores a patient name, address, or national identifier.
Until that record exists, kernel tables that participate in the ledger, the genealogy
graph, or the audit hash chain do not carry civil identity. They may carry an opaque
token. That prohibition is the only PII rule this record is entitled to make.

## Consequences

**What this buys.**

- A validated installation stays the version the customer qualified. There is no
  rolling update across tenants, because there are no tenants. The argument in
  `docs/01-vision-and-scope.md` survives contact with the spike.
- The one-process topology in ADR 0001 and the install story in ADR 0003 stay
  possible. Multi-tenant SaaS would have forced a hosted control plane this project
  is not staffed to operate.
- Per-customer export is the whole database. There is no slice to get wrong and no
  table a filter can forget.
- Residency is a fact about where the machine is, which a shop already controls,
  rather than a routing feature we would have to build and then defend to an
  investigator.
- Per-customer behaviour stays configuration, which is what keeps a customer at
  GAMP Category 4. Shipping per-tenant code would manufacture Category 5 for that
  customer on every future change. Spike section 1.1 item 15 is the source of that
  cost; single tenancy makes it a rule rather than a hope.

**What this costs.**

- We will not be a cloud ERP. A shop that wants zero local machines cannot be a
  customer of the v1 product. That is a real lost segment. It is also the segment
  that cannot pin a version, which is why it is acceptable for the beachhead and
  expensive if the customer profile moves upmarket.
- A customer with plants in two jurisdictions does not get one system. They get two
  installations and no consolidation, and no genealogy that spans them. The
  multi-entity exclusion in the vision document is now load-bearing.
- Every feature a hosted multi-tenant product gets for free (central metering, push
  updates, one operations team watching every shop) we do not get. Support is per
  installation. Migrations are per installation. The version matrix across the fleet
  is ours to live with for the life of every validated copy.
- Installation identity and residency are two more kernel facts that every export
  and every investigator conversation must be consistent with. Getting them wrong is
  a compliance finding, not a display bug.
- The PII collision remains fully open. This decision removes the temptation to
  treat it as a tenancy feature, and it does not spend the problem.

## Alternatives considered

**Multi-tenant SaaS, shared database, tenant identifier on every table.** The default
modern answer. Rejected on both axes. On tenancy: rolling updates and a validated
installation cannot coexist, because the customer cannot pin a version if we push
one. On residency: a tenant identifier in one region is not a boundary. It is a
filter. 821.50(b) wants records kept at a centralized point in the United States,
not tagged as belonging to a United States customer. 11.10(b) wants a complete copy
of one customer's records; a filter that a bug can omit a table is not a complete
copy. The spike is right that this shape cannot grow the missing boundary later.
The original instinct was also right that the column is a permanent tax. Both
reasons stand. They were never in conflict.

**Single tenant, no installation record, residency left as an SOP.** Tempting,
because self-host already puts the bits in one building. Rejected for the Wave 1
kernel surface. An SOP in a binder does not give 11.10(b) an exporter, and it does
not stamp a residency jurisdiction onto the records an investigator is reading.
The installation object is small. Pretending the box is not a thing the kernel
knows about is how a tenant column would get bolted on when the first hosted
customer appeared.

**Add a tenant identifier now and do not use it.** Rejected. It is the tax without
the capability, and it trains every query to have a filter that does not mean
residency. The spike's warning is specifically that this column cannot be upgraded
into a boundary.

**Vendor-hosted single tenant, one cluster per customer, region chosen at
provisioning.** Compatible with this decision and not chosen yet. It preserves
isolation and residency. It gives up the spare-office-machine install story. It is
a commercial mode, not a schema. Revisit when someone is prepared to operate it.

**Multi-tenant with real isolation: one database per tenant, shared application
fleet, region as a placement attribute.** This is what the spike actually requires
if we ever host many customers. It is not a column. It is an orchestration product.
Rejected for v1 as out of character for a thirty-person shop install, and recorded
so that if hosting arrives we do not add a tenant identifier as a stepping stone.
The stepping stone is one database per customer, placed in a region.

**Put PII tokenization in this record.** Rejected. Tenancy does not decide how a
name is stored. A vault has its own threat model, key custody, and erasure
protocol. Folding it in here would let a tenancy decision freeze a privacy
architecture. Named as a required follow-on, with the kernel prohibition above so
the follow-on is not already lost.

## Revisit if

- A paying customer cannot self-host and will not buy without a vendor-operated
  region in a named jurisdiction. That reopens hosted single-tenant, not
  shared-database multi-tenancy.
- A single company in the beachhead needs two residencies and also needs one
  genealogy that spans them. That is a new decision about cross-installation
  trace, and it is not answered by a tenant column.
- We ever operate the system ourselves. ADR 0005 already names this as the moment
  we become the database administrator the trust boundary talks about. It is also
  the moment residency stops being "wherever the customer put the box" and becomes
  a placement we have to prove.
- The PII follow-on record is written. It does not reopen this one. It will
  constrain modules that store device-tracking subjects, and it must exist before
  any of those modules write a name.
