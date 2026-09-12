# 0001. Modular monolith, not microservices

Status:   Proposed
Date:     2026-09-11
Decider:  project lead

## Context

The system must be modular, because a shop should run only the modules it needs and
because outside contributors must be able to extend it. Modularity is frequently
conflated with process separation, and the default modern answer would be a set of
services.

An ERP is a single large transactional graph. Completing an operation on a work order
consumes material, creates work-in-process or finished inventory, posts labor and
burden cost, advances a state machine, writes audit records, may require an electronic
signature, and may be vetoed by a training or calibration check. Those either all
happen or none of them happen.

The deployment target is a thirty-person shop with no systems administrator, on one
machine, on a shop network, possibly with no reliable internet connection.

## Decision

One process, one binary, one database, one transaction boundary. Modules are
compile-time units with enforced boundaries rather than separately deployed services.

Boundaries are enforced by table ownership, published interfaces, and a review rule
that no module reads another module's tables directly. See `03-module-system.md`.

## Consequences

**What this buys.**

- Atomicity across the whole transactional graph, for free, using ordinary database
  transactions.
- The ten-minute install is achievable, because there is nothing to orchestrate.
- Cross-module queries stay possible and fast. Genealogy spanning inventory,
  production, inspection, and calibration is one query rather than a fan-out.
- Debugging is tractable. One log, one stack trace, one profiler.
- No network partition can put the system into a half-completed state.

**What this costs.**

- Scaling is vertical first. At the target size this is not binding, and a shop that
  outgrows one machine has other reasons to be talking to us.
- Module boundaries are a discipline rather than a physical constraint. Nothing stops
  a careless join except review, so the discipline must be real and must be enforced
  mechanically where possible, with schema grants and a lint rule.
- A module that panics can take the process down, which means panics must be caught at
  the request boundary and hooks must be time-budgeted.
- Everything is written in the same language, which narrows the contributor pool. This
  is mitigated by the public API, which lets external integrations be written in
  anything.

## Alternatives considered

**Microservices per domain.** Rejected. Distributed transactions across inventory,
costing, and quality are the exact problem this domain does not need, and every ERP
that has tried it has ended up with a saga framework and an eventual-consistency bug
backlog.

**Monolith with no module structure.** Rejected. Fails the requirement that shops run
only what they need, fails the requirement that outsiders can extend without forking,
and makes the regulated validation story much worse, because a customer would have to
validate the whole thing rather than what they use.

**Plugin-loaded runtime modules from day one.** Deferred rather than rejected. See
`03-module-system.md` section 5. Committing to a plugin ABI before the kernel
interfaces have stabilized means changing it later, which is worse than not having one
yet.

## Revisit if

- A single deployment genuinely cannot be served by one machine.
- A specific module has resource characteristics so different from the rest that it
  needs its own process. Scheduling optimization and CAD processing are the two
  plausible candidates, and both can be split out later as external services over the
  public API without disturbing this decision.
