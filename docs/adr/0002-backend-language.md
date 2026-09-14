# 0002. Rust for the backend

Audience: contributor. Status: shipped.

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
- A type system strong enough to make **dimensions**, the separation of money from
  quantity, and state transitions checkable at compile time. A length and a mass are
  different types that will not add; money and quantity share no arithmetic; and the
  result of a unit conversion is a type whose value cannot be read without either
  proving the residual is zero or naming where the residual goes. State enums are
  matched exhaustively. In this domain that is worth a great deal.

  **Correction, 2026-09-11 (decision D1, `research/decisions/core-quantity.md`).**
  This bullet previously read: *"A quantity in inches and a quantity in millimetres can
  be different types that will not add."* That is **false** for this system. Units of
  measure are customer-defined rows in a database, and an unbounded set of units cannot
  be an unbounded set of Rust types. What is checkable at compile time is the
  **dimension** — Count, Length, Mass, Time, Volume, Area — a sealed kernel set. Unit
  identity is a runtime `UnitId`, and adding two lengths in different units is a typed
  error rather than a compile error. See "Does the correction weaken this decision"
  below; it does, at one joint, and the joint is named.
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

**Does the correction weaken this decision.** Yes, at one joint, and the joint should be
named rather than smoothed over.

The context section above says a silent error in a unit conversion corrupts data a
regulated customer must later defend. That failure mode has two halves. The first is a
**wrong conversion factor** in the unit master — 12.0 entered as 1.2. **No type system
prevents this**, in any language, because the factor is data entered by a shop employee.
It is defended by audited and e-signed changes to the conversion tables, by property
tests, and by per-dimension sanity bounds — not by the compiler. The second half is a
**silently dropped rounding residual**, and that one the type system does prevent: a
conversion returns a value that cannot be consumed without addressing its residual.

So Rust buys half of the thing this ADR cited it for, and the more likely half in
practice is the half it does not buy. The list of what the type system actually delivers
— dimensions, money-versus-quantity, un-ignorable residuals, exhaustive state matching,
newtypes — is real and useful, and shorter than this ADR originally claimed.

The consequence for the alternatives is specific and falls on Go. Go can express distinct
dimension types with named types, and it can make a residual awkward to ignore with a
second return value that lint tooling polices. What Go cannot do is make ignoring it a
compile error rather than a lint finding. That is a narrower gap than "Rust checks this at
compile time and Go checks it in tests." Go loses on the type system by a margin, not by a
mile. The decision therefore now rests primarily on packaging — one static binary, no
runtime to install, already named above as the single largest factor — and on the absence
of GC pauses during MRP. Both of those are untouched by this correction, and Go satisfies
them too, which is precisely why it is recorded below as the closest call. The revisit
trigger at the end of this ADR is **more** live after this correction, not less.

## Alternatives considered

**Go.** The closest call. Also produces a single static binary, has excellent database
and HTTP libraries, and is far easier for contributors to pick up. Lost on type system
strength: dimensional separation, the money-versus-quantity split, un-ignorable rounding
residuals, and exhaustive state handling are things Rust checks at compile time and Go
checks with a linter and tests. Note per the correction above that this margin is
narrower than originally written — Go can express dimension types and multi-return
residuals; it cannot make ignoring them fail the build — and that **neither** language
prevents the likelier corruption, a wrong conversion factor entered as data. In a domain
where the customer may have to defend inventory history to an FDA investigator, the
remaining difference is still judged worth the friction, but it is a margin and not a
chasm. A reasonable person could decide this differently, and if contributor velocity
turns out to be the binding constraint, this is the alternative to switch to.

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
