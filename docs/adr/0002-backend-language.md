# 0002. Rust for the backend

Status:   Proposed
Date:     2026-09-11
Decider:  project lead

## Context

The system must run on macOS, Windows, and Linux, and must install in under ten minutes
on a machine with no developer tooling and no systems administrator.

It computes quantities, money, and unit conversions, where a silent error corrupts data
that a regulated customer must later defend to an investigator.

It is an open source project that needs outside contributors, which argues for a
popular language with a large pool.

These pull in opposite directions, and the second and third are genuinely in tension.

## Decision

Rust, with Axum for HTTP and SQLx for database access.

## Consequences

**What this buys.**

- **One static binary per platform with no runtime to install.** This is most of the
  cross-platform requirement solved for free, and it is the single largest factor. A
  Node or Python backend means shipping and managing an interpreter on three operating
  systems, and it means the customer's antivirus has opinions about a directory full of
  scripts.
- A type system strong enough to make units of measure, currency, and state transitions
  checkable at compile time. A quantity in inches and a quantity in millimetres can be
  different types that will not add. In this domain that is worth a great deal.
- No garbage collector pause during a long MRP run.
- Memory safety, which matters less for a business application than for a parser, but
  is not nothing when the application will be network-exposed on a shop network.
- Excellent cross-compilation, so all platform builds come off one CI machine.

**What this costs.**

- **The contributor pool is smaller than TypeScript or Python.** This is the real price
  and it should not be minimized. An open source ERP lives or dies on contribution.
- Slower to write, especially for UI-adjacent and glue code. Compile times on a large
  workspace are a daily tax.
- Fewer existing libraries for business-domain problems. There is no Rust equivalent of
  the Python or Java ecosystems for tax tables, EDI parsing, or accounting primitives.
- Hiring, if this ever becomes a company, is harder.

**The mitigation, and it is the important part.** What determines whether outsiders can
build on this is the **extension surface**, not the core language. A complete, stable,
documented HTTP API and event stream means a scheduling optimizer, a customer portal, a
machine-monitoring bridge, or an industry-specific module can be written in any
language, by anyone, without touching the core. The public API is therefore not a
secondary deliverable. It is the contributor story, and it must be treated as a
first-class product from the first release.

## Alternatives considered

**Go.** The closest call. Also produces a single static binary, has excellent database
and HTTP libraries, and is far easier for contributors to pick up. Lost on type system
strength: units of measure, money, and exhaustive state handling are things Rust checks
at compile time and Go checks in tests. In a domain where a unit conversion bug corrupts
inventory silently for months, and where the customer may have to defend the result to
an FDA investigator, that difference is worth the friction. A reasonable person could
decide this differently, and if contributor velocity turns out to be the binding
constraint, this is the alternative to switch to.

**TypeScript on Node.** One language across the stack, the largest contributor pool, and
the fastest path to UI-heavy features. Rejected on packaging, since shipping a Node
runtime for three platforms undermines the install story, and on the single-threaded
model fighting long-running MRP computation.

**Python with FastAPI.** Fastest to prototype, and the same ecosystem as ERPNext and
Odoo so domain code is easy to learn from. Rejected on packaging, which is the worst of
any option for three-platform desktop distribution, and on computational throughput for
planning runs.

**C# on .NET.** Genuinely good fit: strong types, single-file publish, good
cross-platform story, strong business-application ecosystem. Rejected mainly on
open source community fit for this kind of project, which is a softer reason than the
others and worth revisiting if a contributor base materializes there.

## Revisit if

- Contribution stalls and interviews with people who looked at the project and walked
  away point at the language rather than at documentation or scope.
- This happens **before** substantial module code exists. After phase 3, switching is no
  longer a decision, it is a rewrite.
