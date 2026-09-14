# 0004. All quantities and values are derived from an append-only ledger

Audience: contributor. Status: shipped.

Status:   Proposed
Date:     2026-09-11
Amended:  2026-09-11, decision lane, see `research/decisions/ledger-invariant.md`
Decider:  project lead

## Context

Every ERP must answer two questions about every number it shows: what is it now, and
how did it get that way. The conventional implementation answers the first well and the
second badly. A `quantity_on_hand` column is updated in place, and a separate audit log
records that it changed. The two can disagree, and in practice they do, because the
update path and the logging path are different code written by different people at
different times.

For a system that must satisfy 21 CFR 11.10(e), which requires audit trails that record
changes without obscuring previously recorded information, a design where history is a
side effect of correctly remembering to write it is the wrong foundation.

Separately, genealogy, which is the single most important capability for the target
customer, is a question about history. If history is a log rather than the data itself,
genealogy has to be built as a parallel tracking structure that must be kept in sync
with inventory. Two structures that must agree will eventually not agree.

## Decision

There are no stored balances. Every quantity and every monetary value in the system is
the sum of immutable postings in a ledger.

A posting is never updated and never deleted. Corrections are made by posting a
reversing entry.

Postings are grouped. A group is one atomic business fact, it carries a **recorded and
constrained kind**, and it is subject to a conservation law stated per **balance slice**
rather than as one scalar sum. There are two laws, and they are enforced by database
constraints, not by application convention.

**Quantity is conserved per `(group, item, unit of measure)`.** A group's quantity
postings for a given item, in that item's canonical stocking unit, sum to zero. Feet are
never added to each, and quantities are never added to money.

**Value is conserved per `(group, currency)`.** A group's monetary postings sum to zero.
Value is the law that survives transformation, because matter changes identity and money
does not: cost leaves work-in-process and enters finished goods, and what remains in
work-in-process is the variance.

To make the quantity law hold universally, quantities always move between locations, and
virtual locations exist for every case where goods appear to enter or leave the world. A
receipt is a transfer from `SUPPLIER`. A shipment is a transfer to `CUSTOMER`. Scrap goes
to `SCRAP`. A cycle count correction moves to or from `ADJUSTMENT` and requires a reason
code. Material issued to a job moves into that work order's `WIP`.

Manufacturing, however, is not a closed system in quantity: a titanium bar becomes five
hundred screws, and no slicing of a quantity sum survives that. So a group of kind
`TRANSFORMATION` may move quantity across the `CONSUMED` and `PRODUCED` boundaries,
where matter is permitted to change identity. **The seam is not a loophole, because
access to it is the constrained resource.** No other kind of group may touch those
boundaries — a receipt cannot reach for `CONSUMED` to absorb a shortfall, and a cycle
count cannot reach for `PRODUCED` to conjure the bars it cannot find. The dispatch on
kind is monotone: a kind may only add predicates and is never exempted from the base
law. And crossing the seam is priced in money: every quantity that crosses must carry a
value posting that does not, so the seam where matter may change identity is exactly the
seam where value is forbidden to disappear.

One further constraint is what makes the rest load-bearing rather than decorative.
**Matter does not leave a real location unless the costing engine has named the specific
earlier postings it came out of**, in amounts that add up to exactly what left, valued at
exactly what the valuation engine posted. That allocation is recorded as an append-only
edge from the consuming posting to the consumed posting, and it is the one place in the
design where two numbers are produced by two engines that could have disagreed. It is
also, for free, what makes genealogy total: no posting can be an orphan in the lineage
graph, because the database refuses the withdrawal that has no named parents. Cost layers
for FIFO, moving average, and standard cost are three allocation policies over that same
edge table.

Corrections are typed. A group of kind `REVERSAL` names the group it reverses, and must
be the exact row-for-row negation of it. A fact may be reversed at most once, and a
reversal may not itself be reversed.

Materialized balance projections exist for query performance. They are explicitly a
cache. They are rebuildable from the ledger with one command, and a scheduled job
verifies agreement. If the cache and the ledger disagree, the ledger is correct.

**Units and rounding.** Every quantity is stored in the item's canonical stocking unit at
a precision the item declares, and the database rejects a posting that is not exactly
representable at that precision. Conversion happens once, at the API boundary, and the
entered value, entered unit, and conversion factor are recorded as provenance that no
invariant ever sums. An item's stocking unit and precision are immutable while postings
reference them, and lot-scoped conversion factors are pinned at receipt, so the receipt
and the issue can never use different rulers.

Where a conversion does not divide evenly, the residual is not inside any group — it is a
discrepancy between the ledger and the physical world, and no group constraint can
detect it. The database therefore bounds it instead: the `ROUNDING` boundary is reachable
only from a reason-coded adjustment, and a single rounding posting may not exceed the
item's declared dust tolerance, so nobody can write off three bars as a rounding error. A
scheduled report finds accumulated dust and a cycle count confirms it. A *value* residual
is the opposite case — it is a discrepancy between postings, so the database does enforce
it, and an allocation that does not sum exactly to the whole is rejected.

The full derivation, the SQL, and a case-by-case walk through receipt, inspection,
issue, completion, scrap, cycle count, shipment, customer return, rework, outside
processing, inexact conversion, and correction are in
`research/decisions/ledger-invariant.md`. The data model records the normative
version.

## Consequences

**What this buys.**

- The audit trail is the data. There is no second structure to keep in sync and no
  possibility of a mutation that failed to log.
- Any balance at any past instant is reconstructible, which is exactly the question an
  investigator asks.
- Genealogy is a graph traversal over postings that already exist, and it is complete
  rather than best-effort, because the database will not accept a withdrawal that does
  not name its sources.
- Inventory cannot drift silently *within a group*, because a group that loses a
  counterpart, reclassifies a cost element, moves matter without cost, or crosses the
  transformation seam without accounting for the value is rejected at commit. Drift
  *across* groups — accumulated conversion dust, a consistently mistyped magnitude — is
  bounded and reported, not constrained, and the ADR is deliberate about the difference.
- Cost layers for FIFO and average costing fall out of the same structure rather than
  needing their own.

**What this costs.**

- More storage. A shop posting 5,000 transactions a day generates a few million rows a
  year. This is not a real problem at the target scale, but it is real.
- Every read path needs either a projection or a sum, and getting projection
  invalidation right is genuine work.
- Developers who have not worked this way find it unintuitive at first. The mental
  model has to be taught, and the module authoring guide has to teach it.
- Some operations that feel like a single update become several postings, and the
  reason codes that make them meaningful are extra ceremony at the point of entry.

**What it forbids, permanently.** No module may ever update a balance directly. This
has to be enforced at the database grant level rather than by code review, because a
single violation anywhere silently destroys the guarantee everywhere. The rule to teach
is that a dimension on an immutable row is free and a total on a mutable row is
forbidden: the question is never whether something is a number, it is whether anything
ever updates it.

## Superseded reasoning

The original version of this decision said that **every posting group must sum to zero**,
full stop, enforced by a database constraint. That claim was wrong in three ways, and the
mistake is recorded here rather than quietly edited out, because the ADR is the project's
memory and this particular error is the one a reader will make again.

**It was wrong mechanically.** A scalar `SUM(quantity)` over a group adds feet to each
and pounds to dollars. A single work order completion posting four screws, one housing,
and one assembly nets minus four and would have been rejected, and a group carrying
inventory quantity alongside labour hours and money was never summable at all.
Conservation is a statement about a *slice*, and the slice keys have to be written down.

**It was wrong mechanically a second time.** PostgreSQL has no `CREATE ASSERTION`, and a
`CHECK` constraint is single-row and can never be deferred. The only mechanism that
expresses a commit-time multi-row predicate is a `DEFERRABLE INITIALLY DEFERRED`
constraint trigger, and constraint triggers cannot use transition tables, so the check
runs with row semantics. "Enforced by a database constraint" was true in spirit and
unimplementable as written.

**It was wrong physically, and this is the instructive part.** Per-item conservation does
not survive transformation. A bar becomes screws; the bar side shows minus one with
nothing offsetting it and the screw side shows plus five hundred with nothing offsetting
it. The obvious patch is virtual `CONSUMED` and `PRODUCED` locations that absorb each
side — and that patch is worse than the problem, because it makes every group balance by
construction. **An invariant that cannot fail is not an invariant.** A constraint whose
two sides are one number written twice by one line of code proves that a numeric survived
a round trip through the write-ahead log, and nothing else.

The correction was not to weaken the law but to notice where the real conservation lives.
Quantity is conserved for movements. Value is what survives transformation. Access to the
transformation seam is restricted to a group kind that is recorded and constrained rather
than inferred, and crossing it costs the value predicate. And the one genuinely
load-bearing constraint is the coupling between matter and its cost layers, because that
is the only place where two independent computations have to agree.

The general lesson, worth more than the specific fix: **a conservation constraint is
load-bearing exactly to the degree that the two sides of the slice originate from
different computations.** When adding an invariant here, the first question is not what
it asserts. It is what would have to go wrong for it to fail, and whether that is a bug
anyone could actually write.

## Alternatives considered

**Mutable balances with a trigger-based audit log.** Conventional, familiar, and what
most ERPs do. Rejected because the audit log and the data are separate artifacts that
can disagree, and because reconstructing a past state means replaying a log that was
never designed to be replayed.

**Event sourcing with a full CQRS split.** Closely related and shares the good
properties, but adds eventual consistency between the write and read sides. In a system
where a machinist scans a part and immediately needs to see the updated quantity,
eventual consistency is a user-visible defect. The ledger approach keeps one
transactional database and one consistent read.

**Ledger for inventory only, mutable elsewhere.** Tempting, and rejected because the
same argument applies to cost, and mixing the two models means every developer has to
remember which domain they are in.

## Revisit if

- Posting volume at a real customer exceeds roughly 50 million rows a year, at which
  point partitioning and archival strategy needs its own decision rather than this one.
- A conservation law proves unworkable for some genuine business case not anticipated
  here. Document the case before weakening a constraint, because weakening one is close
  to irreversible. Prefer adding a group kind with its own added predicates over
  removing a predicate from an existing one: dispatch on kind is monotone by design, and
  the first time it is not, the whole scheme stops meaning anything.
- A new group kind is proposed that needs access to the `CONSUMED` or `PRODUCED`
  boundary. That is the seam the design deliberately keeps narrow, and widening it is
  the single change most likely to turn these constraints back into bookkeeping theater.
- Catch-weight items, co-products, or by-products enter scope. Each is designable over
  this structure but none was worked through here.
