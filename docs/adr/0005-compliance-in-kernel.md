# 0005. Audit trail and electronic signature live in the kernel

Status:   Proposed
Date:     2026-09-11
Amended:  2026-09-11, decision lane, see `_team/reports/DECISION-audit-persistence.md`
Decider:  project lead

## Context

The target customer must satisfy 21 CFR Part 11, which requires computer-generated,
time-stamped audit trails that record the operator, the change, and the prior value
without obscuring it, and electronic signatures bound to their records in a way that
makes later tampering detectable.

The obvious modular instinct is to make compliance a module, so that unregulated shops
do not carry the weight. That instinct is wrong here, and the reason is worth writing
down because it will be proposed again.

If audit is a module, then every other module must remember to call it. Modules written
before the audit module existed will not. Modules written by third parties may not.
Modules written in a hurry will have one code path that does and one that does not. The
result is an audit trail that is mostly complete, and a mostly complete audit trail is
worthless, because its value comes entirely from the guarantee that nothing is missing.

The same argument applies to signatures. A signature system a module can decline to use
provides no assurance.

## Decision

Audit trail, electronic signature, record immutability, and server-authoritative time
are kernel properties, present from the first commit, applying automatically to every
module including ones not yet written.

Specifically:

- **Audit records are produced by the database**, not by module code and not by the Rust
  persistence layer. A row trigger, attached automatically to every audited table when
  the table is created, writes the entry as part of the same transaction as the change.
  A module author cannot forget to write one, because they never write one. The
  application's job is to supply the actor and the business intent, transaction-locally,
  and to fail closed when it cannot: a write that reaches the database without an
  attributable actor is refused, not recorded as unknown.
- **The audit table is append-only at the database grant level, and the application
  cannot write to it at all.** The application role has select. It does not have insert,
  update, or delete. Entries reach the table only through the security-definer trigger
  function, which the application cannot call directly and cannot supply an identity or a
  timestamp to. An application bug cannot violate the trail, and neither can application
  code that is trying to.
- **The trail is tamper-evident beyond the application boundary.** Grants bind the
  application; they do not bind whoever administers the database server, who on a
  self-hosted install is usually the customer. Each transaction is therefore sealed into
  a hash chain, and the head of that chain is published off the server on a schedule the
  customer controls, so that a later alteration of stored history is detectable by
  verifying an exported copy against those off-server records on another machine.
- **Time comes from the server**, always, read inside the trigger. No client-supplied
  timestamp is ever trusted or stored as the time of record. Every row of one atomic
  change carries the same time of record, because it was one act. Server time is not the
  same thing as trusted time — the server reads the host clock, and the host clock is
  administered by the customer — and the product says so rather than implying otherwise.
- **Every action has an attributable actor.** Background jobs run as a named service
  principal. There is no anonymous system change, and no "unknown" actor: a write the
  database cannot attribute is refused.
- **Electronic signature is a kernel primitive** bound to a hash of the exact record
  version signed, capturing signer, server time, and the meaning of the signature.
  Modules declare which transitions require one. They do not implement signing. Every
  signing requires all identification components, every time. The continuous-session
  relaxation the regulation allows is not implemented, because its precondition is a
  component usable only by one individual, and a session on a shared work-centre tablet
  is not that.
- **Records are versioned rather than overwritten** wherever a controlled document or a
  signed record is involved.

Unregulated shops still carry this machinery. That is the accepted cost.

## Consequences

**What this buys.**

- The guarantee is structural. It holds for modules nobody has written yet, which is
  the only way it can hold for a plugin ecosystem.
- A third-party module cannot weaken the compliance posture of the system, which is
  what makes it safe to have third-party modules at all in a regulated installation. It
  cannot skip the trail, edit the trail, or write an entry the trail did not earn. It
  can still name an actor the kernel authenticated, because in-process code runs with the
  application's privileges; that is a software-integrity problem, answered by the
  validated build and the hashed module manifest, and not one this decision claims to
  solve.
- The validation story is dramatically simpler. Audit and signature behavior is
  validated once, in the kernel, rather than re-validated per module.
- Correct by default rather than correct if remembered.

**What this costs.**

- Every write carries audit overhead, including for shops that do not need it. At the
  target transaction volume this is acceptable, and the audit write is in the same
  transaction so it is one round trip rather than two.
- Storage grows with change volume, not just data volume.
- Development ergonomics suffer slightly. Deleting a test record is not a thing you can
  casually do, test fixtures need to work with the grain of immutability, and because the
  application cannot insert audit rows, fixtures must write through the real write path
  rather than staging a trail directly. Tests get their own database rather than their own
  delete privilege.
- A write that loses its actor fails. Forgetting to open a transaction through the kernel
  helper is not a silent degradation to an anonymous entry; it is an aborted transaction
  and a visible bug. That is the intended trade, and it means the helper has to be
  pleasant enough that nobody wants to route around it.
- Kernel events that are not row changes — logins, exports, prints, signatures, security
  events — now need a narrow, reviewed database function to reach the trail, and that
  function runs with elevated rights. It is deliberately kept unable to fabricate a row
  change, and its owner deliberately cannot modify or delete anything.
- Tamper evidence is only evidence if somebody performs it. The hash chain is worthless
  without the off-server anchor and the periodic verification, which are the customer's
  procedure, supported by our tooling. We ship the tool, the SOP, and a nagging health
  check; we cannot ship the habit.
- Some genuinely uninteresting changes get audited. A configuration table for UI
  preferences does not need a compliance-grade trail. The kernel allows a table to be
  declared non-audited, and that declaration is itself a reviewed, recorded decision
  rather than a per-write choice.

## What we say about it

The mechanism is only half of this decision. The other half is the sentence a customer
repeats to an investigator, which must be true, must name its own boundary, and must not
be improved upon by anyone writing marketing copy. It is:

> Every change to a regulated record in Datum is written to the audit trail by the
> database itself, inside the same transaction as the change, with the operator's
> identity, the server time, the prior and new values, and the reason where one is
> required; a write that cannot be attributed to an authenticated operator is refused
> rather than recorded as unknown. The application — including any module, and including
> a defective one — can read the audit trail but cannot insert, alter, or delete an
> entry: that is enforced by database privileges, not by application code. Datum does not
> claim the trail cannot be altered by someone with administrative control of the database
> server itself; instead, each transaction is sealed into a hash chain whose head is
> published off the server on a schedule you control, so that any later alteration of
> stored history is detectable by verifying an exported copy against those off-server
> records on a separate machine — which is the check your periodic audit-trail review
> performs.

Product documentation, the vision document, and sales material quote that paragraph or
say something strictly weaker. "The audit trail cannot be altered" is not a sentence this
project makes. An investigator who asks who the database superuser is finds out in about a
minute, and being the vendor whose claim survives that minute is worth more than the
stronger sentence.

## Alternatives considered

**Audit as a module.** Rejected for the reasons in Context. The failure mode is a
partial audit trail, which is worse than none because it invites false confidence.

**Audit by database trigger only.** Adopted as the mechanism, and not sufficient on its
own. Triggers catch everything, including changes made outside the application, which is
the only way "cannot forget" survives contact with modules nobody has written yet. What a
trigger cannot see is business intent: which user action caused the change, what reason
the operator gave, and what document it belongs to. The application supplies that as
transaction-local context before it writes, and the trigger refuses the write if it is
missing. Trigger plus fail-closed context is the design; trigger alone would produce a
complete trail of changes nobody can interpret.

**Audit written by the application's persistence layer in Rust.** This was the original
decision and it does not survive contact with the library we chose. It would require an
interception point that sees every write with both its old and its new values, and that
point does not exist: there is no unit of work, no entity callback, and no wrapper that
catches a caller who writes directly. What can be built in Rust — a single blessed write
type, plus a lint forbidding everything else — is house style. House style is correct if
remembered, which is the exact property this ADR exists to reject, so it is kept as
ergonomics and defence in depth and is never described as the guarantee.

**Grant-level append-only as the whole integrity story.** Retained as necessary and
rejected as sufficient. Revoking write privileges from the application role is a real
control against a real threat, and it is the honest answer to "can a bug in your software
change the trail." It is not an answer to "can anyone in the building change the trail,"
because on a self-hosted install the person who owns the data directory is the database
superuser and privileges do not bind them. The answer there is evidence, not prevention:
a hash chain whose head leaves the machine.

**Cryptographically signing audit rows with a key held by the application.** Rejected. A
signature whose key lives beside the data it signs proves only that the software was
working, which the software already claims. If a customer wants signed anchors, the key
is theirs and lives off the server.

**A configurable compliance mode that can be switched off.** Rejected. A switch that
disables the audit trail is itself an audit finding, and the existence of the code path
undermines the guarantee even when it is off.

## Superseded reasoning

This ADR is the project's memory, so what it got wrong stays written down.

**It said audit records are produced by the persistence layer, so a module author cannot
forget or falsify one.** The intent was right and the mechanism did not exist. SQLx has no
interceptor, no unit of work, and no callback that sees a row before and after, so there
was no place in Rust to put the guarantee. Anything we could have built there — a sealed
write type, a macro, a lint — would have been bypassable by one ordinary query, which is
the failure mode this ADR was written to prevent. The guarantee moved to a database
trigger, where it is unforgettable by construction, and the Rust side was reduced to
supplying context and failing closed without it. This was found by reading the library's
documentation carefully instead of assuming it behaved like an ORM.

**It said the audit table is append-only at the grant level, with the application role
holding insert and select.** Two errors in one sentence. First, granting the application
insert contradicted the same ADR's claim that a module cannot write a false audit entry:
if the application can insert, any code in it can insert a lie, and no amount of review
prevents that. Insert is now revoked and entries reach the table only through a
security-definer trigger the application cannot hand an identity or a timestamp to.
Second, "append-only at the grant level" was allowed to imply "cannot be altered." Against
an application bug, it is true and worth having. Against the person who administers the
server — on a self-hosted install, the customer's own IT person, who owns the data
directory and is the database superuser — privileges bind nothing. They can update the
table, disable the triggers, switch off trigger firing for the session, or restore a
doctored dump. Telling an investigator the trail cannot be altered when it can is worse
than not claiming it, so the claim is now scoped and the gap is covered by evidence rather
than by assertion: a hash chain, anchored off the server, verified by the customer's own
periodic review.

Neither correction changes what this ADR wants. Both change what it is entitled to say.

## Revisit if

- Audit write volume becomes a measured bottleneck at a real customer, in which case
  the answer is partitioning and archival of audit data, not weakening the guarantee.
- We ever host the system ourselves, in which case we become the database administrator
  the trust boundary names, and the off-server anchor has to leave our infrastructure
  rather than the customer's for the evidence to mean anything.
- A customer's own quality system supplies a stronger integrity control — write-once
  storage, a hardware key they hold, a third-party timestamp authority — in which case we
  feed it rather than duplicate it.
