# 0004. All quantities and values are derived from an append-only ledger

Status:   Proposed
Date:     2026-09-11
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

Postings are grouped, and **every group must sum to zero**. This is enforced by a
database constraint, not by application convention.

To make the zero-sum rule hold universally, quantities always move between locations,
and virtual locations exist for every case where goods appear to enter or leave the
world. A receipt is a transfer from `SUPPLIER`. A shipment is a transfer to `CUSTOMER`.
Scrap goes to `SCRAP`. A cycle count correction moves to or from `ADJUSTMENT` and
requires a reason code. Material issued to a job moves into that work order's `WIP`,
and what remains in `WIP` after completion is the variance.

Materialized balance projections exist for query performance. They are explicitly a
cache. They are rebuildable from the ledger with one command, and a scheduled job
verifies agreement. If the cache and the ledger disagree, the ledger is correct.

## Consequences

**What this buys.**

- The audit trail is the data. There is no second structure to keep in sync and no
  possibility of a mutation that failed to log.
- Any balance at any past instant is reconstructible, which is exactly the question an
  investigator asks.
- Genealogy is a graph traversal over postings that already exist.
- Inventory cannot drift silently, because an unbalanced group is rejected at write
  time.
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
single violation anywhere silently destroys the guarantee everywhere.

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
- The zero-sum constraint proves unworkable for some genuine business case not
  anticipated here. Document the case before weakening the constraint, because
  weakening it is close to irreversible.
