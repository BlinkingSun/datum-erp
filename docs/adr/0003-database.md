# 0003. PostgreSQL, bundled with the installer

Status:   Proposed
Date:     2026-09-11
Decider:  project lead

## Context

The ledger design in ADR 0004 depends on database-enforced invariants. The zero-sum
constraint on posting groups, effectivity date ranges that must not overlap, and
append-only audit tables enforced by grant rather than by application code are all
things the database must do, because doing them in application code means a bug can
break a compliance guarantee.

At the same time, the install target is a shop with no database administrator, and the
success criterion is a working system in under ten minutes.

These conflict. The most capable database is the least trivial to install.

## Decision

**PostgreSQL, as the only supported database.** No dialect abstraction layer, no second
backend.

**Bundled with the desktop installer.** The installer ships a PostgreSQL binary,
initializes a cluster in the application data directory, manages its lifecycle, and
handles upgrades. The user never learns it is there.

**With an escape hatch.** A shop that already runs PostgreSQL can point the application
at it with a connection string, and larger deployments will.

## Consequences

**What this buys.**

- Deferrable constraints, which is what makes the zero-sum ledger check possible at
  commit time rather than at every insert.
- Exclusion constraints and range types, which handle effectivity and calibration
  intervals correctly instead of approximately.
- Recursive common table expressions, which is what genealogy traversal is.
- Table-level and column-level grants, which is how the audit table becomes
  append-only for the application role rather than merely by convention. This is a real
  compliance argument, not a nicety.
- Point-in-time recovery and logical replication, so backup is a solved problem rather
  than a feature to write.
- One dialect, one set of migrations, one set of tests. No abstraction tax forever.

**What this costs.**

- Installer complexity, which is genuine work. Initializing a cluster, choosing a port,
  handling an unclean shutdown, and performing a major-version upgrade of a bundled
  cluster are all real engineering and all must work unattended.
- Installed size, roughly a few hundred megabytes.
- Platform-specific packaging work, three times.
- Some antivirus and endpoint products dislike a bundled server process and will need
  documentation to appease.

## Alternatives considered

**SQLite.** Enormously attractive for the install story, since the database is a file
and there is nothing to manage. Rejected on capability. No deferrable constraints, weak
concurrent write behavior under simultaneous shop floor scanning, limited grant model
so the append-only audit guarantee becomes application-enforced, and no point-in-time
recovery. It would win if the compliance requirements were softer. They are not. It
remains attractive for a future read-only offline shop floor cache, which is a
different problem.

**Support both, with an abstraction layer.** Rejected firmly. Dual-dialect support is a
tax paid on every query, every migration, and every test, forever, and the abstraction
inevitably leaks exactly at the advanced features that motivated choosing PostgreSQL.
Teams that do this end up writing to the lowest common denominator, which means not
using the features at all.

**Require the customer to install PostgreSQL.** Rejected. It breaks the ten-minute
install and it is the most common point at which an evaluation is abandoned. For larger
shops it remains available as the escape hatch.

**Embedded PostgreSQL as a library.** Investigated. The options are immature and the
licensing is uncomfortable. Bundling the real server binary is boring and works.

## Revisit if

- The bundled-cluster upgrade path proves genuinely unreliable in the field, in which
  case requiring an external database for regulated installations becomes the honest
  answer.
- An offline shop floor mode becomes a requirement, which would introduce a local cache
  and a sync protocol. That is a new decision, not a modification of this one.
