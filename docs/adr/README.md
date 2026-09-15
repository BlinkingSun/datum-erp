# Architecture Decision Records

Audience: contributor. Status: shipped.

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
| [0003](0003-database.md) | PostgreSQL, installed and managed by the operating system | Accepted, amended |
| [0004](0004-append-only-ledger.md) | All quantities and values are derived from an append-only ledger | Proposed |
| [0005](0005-compliance-in-kernel.md) | Audit trail and electronic signature live in the kernel | Proposed |
| [0006](0006-license.md) | License: AGPL-3.0-or-later, contributions under the DCO | Accepted |
| [0007](0007-defer-general-ledger.md) | Do not build a general ledger | Proposed |
| [0008](0008-single-tenant.md) | Single tenant per installation; residency is an installation property | Proposed |
| [0009](0009-ui-stack.md) | TypeScript and React for the UI, Tauri for the desktop shell | **Accepted** (2026-09-15) |
| [0010](0010-one-registry.md) | One capability registry generates the router, the document, and the agent surface | Proposed |

0003 is Accepted, as amended; 0006 is Accepted. Everything else is Proposed. Proposed
means it is the current recommendation and the docs are written as though it holds.
Moving to Accepted is a deliberate act.
