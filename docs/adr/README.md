# Architecture Decision Records

One file per decision that would be expensive to reverse. Each records what was
decided, why, what it costs, and what would make us change our mind.

The point is not ceremony. It is that in two years someone will ask "why on earth did
they do it this way," and the answer should exist in writing rather than in someone's
memory.

## Format

```
# NNNN. Title

Status:   Proposed | Accepted | Superseded by NNNN | Rejected
Date:     YYYY-MM-DD
Decider:  who

## Context
What forces are at play. What makes this decision necessary.

## Decision
What we are doing. Stated plainly and in the active voice.

## Consequences
What this buys and what it costs. The costs matter more than the benefits,
because the benefits are why it was proposed and the costs are what will
actually be lived with.

## Alternatives considered
What else was on the table and why it lost.

## Revisit if
The specific conditions that should reopen this.
```

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-modular-monolith.md) | Modular monolith, not microservices | Proposed |
| [0002](0002-backend-language.md) | Rust for the backend | Proposed |
| [0003](0003-database.md) | PostgreSQL, bundled with the installer | Proposed |
| [0004](0004-append-only-ledger.md) | All quantities and values are derived from an append-only ledger | Proposed |
| [0005](0005-compliance-in-kernel.md) | Audit trail and electronic signature live in the kernel | Proposed |
| [0006](0006-license.md) | License | **Open — needs a decision** |
| [0007](0007-defer-general-ledger.md) | Do not build a general ledger | Proposed |
| [0008](0008-single-tenant.md) | Single-tenant self-hosted, not multi-tenant SaaS | Proposed |

Nothing here is Accepted yet. Proposed means it is the current recommendation and the
docs are written as though it holds. Moving to Accepted is a deliberate act.
