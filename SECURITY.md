# Security

Conforms to: [ADR 0005](docs/adr/0005-compliance-in-kernel.md); `research/decisions/audit-persistence.md` §7.

## 1. Reporting a vulnerability

Report vulnerabilities privately. Do not open a public issue.

**[PRIVATE CONTACT TO BE FILLED BY THE PROJECT OWNER]**

If GitHub private vulnerability reporting is enabled on https://github.com/BlinkingSun/datum-erp, use that. Until the owner fills the placeholder above, treat the report as unrouted and do not publish it.

## 2. Supported versions

None yet. Datum is pre-alpha and has no release. There is no supported version to patch.

## 3. What the audit trail actually guarantees

The wording the project uses — in `docs/01`, `docs/02`, `docs/06`, the sales site, and the validation pack — and the wording a customer can repeat to an investigator, taken from `research/decisions/audit-persistence.md` §7:

> **Every change to a regulated record in Datum is written to the audit trail by the
> database itself, inside the same transaction as the change, with the operator's identity,
> the server time, the prior and new values, and the reason where one is required; a write
> that cannot be attributed to an authenticated operator is refused rather than recorded as
> unknown. The application — including any module, and including a defective one — can read
> the audit trail but cannot insert, alter, or delete an entry: that is enforced by database
> privileges, not by application code. Datum does not claim the trail cannot be altered by
> someone with administrative control of the database server itself; instead, each
> transaction is sealed into a hash chain whose head is published off the server on a
> schedule you control, so that any later alteration of stored history is detectable by
> verifying an exported copy against those off-server records on a separate machine — which
> is the check your periodic audit-trail review performs.**

Three sentences, because a true statement of this needs three: what is guaranteed, who cannot break it, and where the boundary is and what covers it.

What it does **not** say, deliberately: that the trail is immutable; that it cannot be altered; that nobody can edit it; that time is traceable to an authenticated source; that the chain is required by 21 CFR Part 11. Each of those would be false or unprovable. "The audit trail cannot be altered" does not appear anywhere.
