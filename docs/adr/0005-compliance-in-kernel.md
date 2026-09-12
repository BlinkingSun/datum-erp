# 0005. Audit trail and electronic signature live in the kernel

Status:   Proposed
Date:     2026-09-11
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

- **Audit records are produced by the persistence layer**, not by module code. A module
  author cannot forget to write one and cannot write a false one. Writing a record
  produces its audit entry as part of the same transaction.
- **The audit table is append-only at the database grant level.** The application role
  has insert and select. It does not have update or delete. This is enforced below the
  application, so an application bug cannot violate it.
- **Time comes from the server**, always. No client-supplied timestamp is ever trusted
  or stored as the time of record.
- **Every action has an attributable actor.** Background jobs run as a named service
  principal. There is no anonymous system change.
- **Electronic signature is a kernel primitive** bound to a hash of the exact record
  version signed, capturing signer, server time, and the meaning of the signature.
  Modules declare which transitions require one. They do not implement signing.
- **Records are versioned rather than overwritten** wherever a controlled document or a
  signed record is involved.

Unregulated shops still carry this machinery. That is the accepted cost.

## Consequences

**What this buys.**

- The guarantee is structural. It holds for modules nobody has written yet, which is
  the only way it can hold for a plugin ecosystem.
- A third-party module cannot weaken the compliance posture of the system, which is
  what makes it safe to have third-party modules at all in a regulated installation.
- The validation story is dramatically simpler. Audit and signature behavior is
  validated once, in the kernel, rather than re-validated per module.
- Correct by default rather than correct if remembered.

**What this costs.**

- Every write carries audit overhead, including for shops that do not need it. At the
  target transaction volume this is acceptable, and the audit write is in the same
  transaction so it is one round trip rather than two.
- Storage grows with change volume, not just data volume.
- Development ergonomics suffer slightly. Deleting a test record is not a thing you can
  casually do, and test fixtures need to work with the grain of immutability.
- Some genuinely uninteresting changes get audited. A configuration table for UI
  preferences does not need a compliance-grade trail. The kernel allows a table to be
  declared non-audited, and that declaration is itself a reviewed, recorded decision
  rather than a per-write choice.

## Alternatives considered

**Audit as a module.** Rejected for the reasons in Context. The failure mode is a
partial audit trail, which is worse than none because it invites false confidence.

**Audit by database trigger only.** Attractive, and partially adopted. Triggers catch
everything, including changes made outside the application, which is a genuine benefit.
Rejected as the whole answer because a trigger sees the row change but not the
business intent: which user action caused it, what reason the operator gave, and what
document it belongs to. The design uses both, with the application supplying context
and the database enforcing that nothing escapes.

**A configurable compliance mode that can be switched off.** Rejected. A switch that
disables the audit trail is itself an audit finding, and the existence of the code path
undermines the guarantee even when it is off.

## Revisit if

- Audit write volume becomes a measured bottleneck at a real customer, in which case
  the answer is partitioning and archival of audit data, not weakening the guarantee.
