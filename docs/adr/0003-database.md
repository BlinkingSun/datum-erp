# 0003. PostgreSQL, installed and managed by the operating system

Audience: contributor. Status: shipped.

Status:   Accepted, amended 2026-09-11. The bundling half of the original decision is
          withdrawn. The database choice is unchanged.
Date:     2026-09-11
Decider:  project lead
Amends:   the original title was "PostgreSQL, bundled with the installer"
See also: `research/decisions/install-story.md` (decision D5),
          `research/audits/slice-bundled-postgres.md` (the evidence)

## Context

The ledger design in ADR 0004 depends on database-enforced invariants. The zero-sum
constraint on posting groups, effectivity date ranges that must not overlap, and
append-only audit tables enforced by grant rather than by application code are all
things the database must do, because doing them in application code means a bug can
break a compliance guarantee.

At the same time, the install target is a shop with no database administrator, and the
success criterion is a working system in under ten minutes.

These conflict. The most capable database is the least trivial to install.

The original version of this ADR resolved the conflict by bundling a PostgreSQL binary
inside the installer and having the application manage its lifecycle. That half did not
survive contact with Windows, and the amendment below records why.

## Decision

**PostgreSQL, as the only supported database.** No dialect abstraction layer, no second
backend. This is unchanged and is not what the amendment touches.

**Installed and lifecycle-managed by the operating system, not by us.** PostgreSQL is
installed from its own platform installer — the EDB installer on Windows, Homebrew or
Postgres.app on macOS, the distribution package on Linux — and the operating system's
own service manager owns the daemon: a Windows Service, a launchd LaunchDaemon, a
systemd unit. The boundary is permanent and is stated as a rule:

> **We never own a database process lifecycle. On any operating system. In any version.
> We own everything above the socket.**

Everything above the socket is: probing for a reachable cluster, creating the database,
creating the application and migration roles, installing extensions, applying the grants
that make the audit table append-only, running migrations, backup, verification,
restore, and the customer-runnable installation qualification suite. That is where this
product's value lives, and a bundled cluster was never going to do it better.

**The escape hatch is promoted to the production path.** What was the exception for
large deployments is now the only supported configuration for controlled records. A
first-run wizard replaces the installer as the thing that makes this painless: it finds
the cluster, states the supported version range, links the exact download for the
operating system it is running on, and then does every remaining step itself. Nobody
writes a connection string and nobody runs `psql`.

**A throwaway evaluation mode exists and is explicitly not production.** A single
downloadable executable unpacks an embedded PostgreSQL on an ephemeral loopback port
with seeded demo data, requiring no administrator rights and no container runtime. It
refuses to import real data, has no promotion path into a production install, expires
after thirty days, and stamps every screen and document as an uncontrolled record. It
reuses the test harness the kernel crates need anyway, so it is nearly free.

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

- Grants become a real boundary rather than a gesture. The postmaster runs as a service
  account that no shop operator logs in as, so the application role's inability to
  update or delete audit rows is a control against every application user rather than
  only against application bugs. Under bundling the operator owned the data directory
  and could have read the superuser password out of their own profile, so the
  append-only claim would have been much weaker. Losing the bundle strengthened it.

**What this costs.**

- A bare machine needs PostgreSQL installed before Wicket, which is a second download and
  an administrator prompt on Windows. This is the real price and it is paid once.
- Platform-specific packaging work, three times — but for our own service registration,
  not for a database supervisor.
- The customer's IT holds the superuser credential, which is a procedural control their
  validation package must cover. We document it rather than let them find it at
  inspection.
- Some antivirus and endpoint products dislike any server process, but they see a binary
  in a well-known vendor path they already recognise rather than one writing write-ahead
  log out of an application's private directory.

**What it no longer costs**, relative to the withdrawn bundling decision: no `initdb`
policy, no port probing, no cluster supervision, no unattended `pg_upgrade`, no second
set of PostgreSQL binaries shipped for every major version, and no hundreds of megabytes
of installer.

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

**Require the customer to install PostgreSQL.** Originally rejected on the grounds that
it breaks the ten-minute install and is the most common point at which an evaluation is
abandoned. **Now adopted**, for the reasons in the amendment below, with the evaluation
concern answered by a throwaway demo mode rather than by a bundled cluster.

**Embedded PostgreSQL as a library.** Investigated. The options are immature and the
licensing is uncomfortable. Retained for tests and for demo mode, where a per-user
process with an ephemeral port is exactly the right shape. Not a production engine.

## Amendment, 2026-09-11 — the revisit fired

This ADR's Revisit-if section named the bundled-cluster upgrade path as the condition.
That condition is met, and then some. It was fired as a planning decision rather than
after a shop lost a ledger to a half-finished `pg_upgrade`, which is the whole point of
writing revisit triggers down.

**What the evidence found** (`research/audits/slice-bundled-postgres.md`): a user-process
cluster on Windows dies when the operator logs off, which is exactly what a shop floor
does at the end of a shift. Running it as a Windows Service requires administrator
rights. Endpoint protection has quarantined PostgreSQL write-ahead log files and killed
the service. `localhost` resolves IPv6-first on Windows, costing seconds against a
sub-second scan budget. `pg_upgrade` needs both sets of binaries and, by its own
documentation, an administrative account. No shipping desktop application silently
lifecycle-manages PostgreSQL on Windows without administrator rights and without a
container runtime; the closest analog requires elevation and discourages Windows for
production.

**The finding that actually decided it**, which the evidence stopped one step short of:
`wicket-server` has the same problem. A server binary launched by the logged-in operator
dies at logoff exactly as `postgres.exe` does. Surviving a logoff requires a system
service, so the administrator prompt was never a consequence of the database decision at
all — it is the price of having a server. Bundling was being asked to buy something it
could not deliver for any database choice, which is why withdrawing it costs far less
than it appears to.

**What did not change:** PostgreSQL as the only dialect, and every capability argument
above it. Deferrable constraint triggers, exclusion constraints, recursive CTEs,
grant-level table permissions, and point-in-time recovery are why ADR 0004 and ADR 0005
work, and all of them require a real PostgreSQL. SQLite and PGlite remain rejected on
capability. The bundling half was under attack; the database half was not.

## Does bundling return?

**In the form originally decided — our own process tree initializing, starting, stopping
and upgrading a cluster — no. Not in v1, not in v2, not ever.** This is recorded so it is
not re-litigated every six months. The reasoning does not depend on tooling maturity, so
no future library makes it stale: surviving a logoff requires the operating system's
service manager, and once you have that there is no reason to duplicate it badly.

A different thing may return, and should not be confused with the above. A **composite
installer** — our installer, already elevated for its own service, invoking the
platform's PostgreSQL installer unattended and then handing the cluster to the operating
system's service manager and walking away — would turn the bare-machine install back into
ten minutes, and we would still never own the lifecycle. It is a fresh decision, not an
automatic one, and it needs all five of these first:

1. Code-signed installers on all three operating systems, from an established identity.
2. A named packaging owner with a per-OS CI matrix. Not a lane borrowed from a feature
   wave. Packaging a database is a product, not a task.
3. The wizard's manual path shipped and documented, so the composite path can fail over.
   It must never be the only way in.
4. Ten paying production installs, so we know which endpoint-protection products
   actually appear in machine shops.
5. It never owns `pg_upgrade`. Major-version upgrades stay with the platform installer.

## Revisit if

- A shop needs an offline shop floor mode, which would introduce a local cache and a sync
  protocol. That is a new decision, not a modification of this one, and an embedded
  single-connection engine is a plausible answer for a read-only cache specifically.
- All five composite-installer conditions above become true and someone wants to spend
  the packaging wave.
