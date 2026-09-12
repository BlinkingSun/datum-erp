# 0007. Do not build a general ledger

Status:   Proposed
Date:     2026-09-11
Decider:  project lead

## Context

The E in ERP stands for enterprise, and most people assume an ERP includes accounting.
Commercial suites do. The instinct to build a general ledger, accounts payable,
accounts receivable, and period close is strong, and it is the single most reliable way
to consume two years and ship nothing anyone wanted.

Meanwhile, every target customer already runs QuickBooks, Xero, or Sage, has a
bookkeeper or an outside accountant who knows it, and has no interest whatsoever in
migrating their books to a new open source project.

## Decision

Do not build a general ledger. Build `gl-export` instead: a journal export with a
configurable account mapping and a reconciliation report, targeting QuickBooks, Xero,
and Sage.

Build `ap-ar-lite`, covering invoices in and out, three-way match, and aging, because
purchasing and sales genuinely need to know whether an invoice matched a receipt. That
is an operational question rather than an accounting one, and it stops short of being
a ledger.

## Consequences

**What this buys.**

- Roughly a year of engineering redirected to the parts that actually differentiate:
  quality, traceability, and shop floor usability.
- No exposure to tax jurisdictions, currency regulation, revenue recognition standards,
  or statutory reporting formats, each of which is an unbounded per-country
  commitment.
- No liability for being the system of record for someone's books.
- Faster adoption, because nobody has to migrate their accounting to try the product.

**What this costs.**

- The product is not a complete ERP by the definition a purchasing committee at a
  larger company would use. For the target customer this is not binding. For a customer
  at 300 people it eventually is.
- Two systems means a reconciliation step, and the export mapping is a real
  configuration burden during implementation.
- Inventory valuation for the balance sheet is computed here and posted there, so the
  two must agree. Getting that reconciliation report right is important and not
  trivial.

## Alternatives considered

**Build the full general ledger.** Rejected on opportunity cost. It is the largest
single body of work in an ERP, it is completely undifferentiated, and the market is
saturated with good cheap options.

**Integrate with exactly one accounting package.** Rejected as unnecessarily narrow. The
export format work is mostly shared, and the mapping layer is the same regardless of
target.

**Build a minimal general ledger for shops that want everything in one place.**
Rejected, and worth being firm about. A minimal general ledger is a trap. It is
adequate until a customer's accountant asks for something it cannot do, at which point
the choice is to grow it into a full one or tell the customer to migrate out, and both
are worse than never having built it.

## Revisit if

- A credible contributor wants to own accounting as a module and has the domain
  expertise to do it. In that case it is an optional module built on the existing
  kernel ledger engine, which is the right shape for it, and it is explicitly not a
  core team commitment.
- The customer profile shifts upmarket enough that the absence blocks deals repeatedly,
  with evidence rather than speculation.
