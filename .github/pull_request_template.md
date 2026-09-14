<!--
Read CONTRIBUTING.md and GOALS.md before filling this in.

The four goal headings below are REQUIRED. Each one takes either a real answer
or "N/A:" followed by a reason. An empty heading, or "N/A" with no reason, fails
review.

The CI check that enforces this automatically is ABSENT (promised: TODO.md T-06).
Until it lands the maintainer reads for it, so a green build is not proof that you
answered.

If this change touches a public route, an event payload, the manifest schema, the
module contract, the canonical order, or the dependency allowlist, it needs an
accepted request for comment FIRST. See CONTRIBUTING.md section 3.
-->

## What this changes

<!-- One paragraph. What was true before, what is true after. -->

## Why

<!-- Link the issue or request for comment. If neither exists, say why not. -->

---

## Goal 1

<!--
Agent-legible. For each new or changed mutation:
 - Which stable error token does a caller branch on? A bare VALIDATION is not an answer.
 - Is it idempotent, and how does a retry behave?
 - If it adds a state-machine edge, does the legal-next-actions query return it?
 - If it adds a module capability, does module.toml declare it?
Write "N/A: <reason>" if this change adds no mutation and no capability.
-->

## Goal 2

<!--
Every function has an API. If this adds a capability:
 - Which HTTP operation reaches it?
 - If none, which allowlist entry exempts it, and what is the stated reason?
A new capability reachable only from Rust, only from the CLI, or only from the UI
does not merge. Write "N/A: <reason>" if this adds no capability.
-->

## Goal 3

<!--
Migration impact. If this changes an entity, an identifier rule, a unit of measure,
a lot or serial law, or anything a shop would carry over from an incumbent ERP,
say what it means for an existing mapping. Write "N/A: no mapping impact" if not.
-->

## Goal 4

<!--
Followable. If this changes user-visible behaviour, installation, configuration, or
an operator procedure, name the document updated in this same change.
Write "N/A: kernel-internal" if nothing an operator touches changed.
-->

---

## Checks

- [ ] `just ci` is green locally.
- [ ] `just ci-db` is green, or this change touches no SQL, kernel, or schema.
- [ ] Every commit is signed off (`git commit -s`). Required by ADR 0006. The CI check is ABSENT (promised: TODO.md T-04), so this one is on you.
- [ ] New source files carry `SPDX-License-Identifier: AGPL-3.0-or-later`. No file in the tree complies yet; the lint and backfill are TODO.md T-13.
- [ ] Migrations have a tested reverse.
- [ ] No `unsafe`. No `todo!()`.
- [ ] This is a pull request, not a disguised architecture decision. Decision records go in their own change.

## Change class

<!-- Delete the ones that do not apply. CONTRIBUTING.md section 3 defines these. -->

- Pull request only: touches no public surface listed in CONTRIBUTING.md section 3.
- Implements accepted request for comment: <link>
- Architecture decision record: this change is the record itself.
