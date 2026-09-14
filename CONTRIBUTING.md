# Contributing to Wicket

Conforms to: [ADR 0006](docs/adr/0006-license.md) (Accepted); [ADR 0010](docs/adr/0010-one-registry.md) (Proposed); crate graph as frozen in `PLAN.md` §5; the four goals in [GOALS.md](GOALS.md).

Read [GOALS.md](GOALS.md) before you read the rest of this file. It states the four things that must be true of every capability this project grows, and a change that violates one does not merge no matter how good it is otherwise. Contributors directing an AI agent at this repository must read [AGENTS.md](AGENTS.md).

## 1. Contribution agreement

Contributions are accepted under the **Developer Certificate of Origin, version 1.1**. There is no contributor license agreement, and none will be asked for. Contributors keep their copyright, so the project cannot relicense without every contributor's consent (`docs/adr/0006-license.md`).

Every commit must carry a `Signed-off-by:` line. `git commit -s` adds it from `user.name` and `user.email`. The line certifies DCO 1.1 for that commit:

```
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```

The code is licensed **AGPL-3.0-or-later**. See `LICENSE` and `docs/adr/0006-license.md`.

## 2. What every change must answer

Four goals govern this project ([GOALS.md](GOALS.md)). The pull request template asks about each one. The CI check that fails an empty goal heading is ABSENT (promised: `TODO.md` T-06); until it lands the maintainer enforces it by reading. Answer honestly; "not applicable" with a reason is a complete answer, and it is the right answer most of the time.

**Goal 1, agent-legible.** A new mutation carries a stable error token a caller can branch on, and a retry story. A bare `VALIDATION` for every failure is not an answer, because an agent cannot tell a held lot from a malformed part number by parsing English. A new state-machine edge appears in the legal-next-actions query.

**Goal 2, every function has an API.** A capability reachable only from Rust, only from the command line, or only from the user interface does not merge. Either it has an HTTP operation, or it has an allowlist entry with a stated reason. The allowlist is how this goal stays honest; if it becomes a junk drawer the goal is dead, so an addition to it is reviewed as a change to `GOALS.md`.

**Goal 3, migration.** If you change an entity, an identifier rule, a unit of measure, or a lot or serial law, say what it means for a shop carrying data over from an incumbent ERP. Usually the answer is "no mapping impact" and that is fine.

**Goal 4, followable.** If an operator would notice, a document changes in the same pull request. A sentence naming a file, subcommand or route is false unless that path exists or is tagged absent.

Every sentence in project documentation is `is`, `must`, or `ABSENT (promised: …)`. There is no fourth mood. A mechanism that does not exist is never described in the present tense. Precedent: `.github/pull_request_template.md:8-10` (T-06 goal-heading check) and `.github/pull_request_template.md:70` (T-04 sign-off check).

## 3. Change classes

The class is decided by the diff, not by the author.

**Pull request.** The default. Use it when the diff touches none of the surfaces listed under request for comment below.

**Request for comment, accepted before the pull request opens.** Required when the diff touches a public HTTP path, an event payload, the module manifest schema, the module contract or hook interface, the canonical module order, or the workspace dependency allowlist. Open the request-for-comment issue form, and wait for the maintainer to accept it. A pull request that changes one of these surfaces without an accepted request is closed, not reviewed.

**Architecture decision record.** Required when the decision would be expensive to *reverse*, which is a different test from whether it is public. The format, status values and index are in `docs/adr/README.md`.

Decisions that would be expensive to reverse live in `docs/adr/`. The format, status values, and index are in `docs/adr/README.md`.

A pull request that changes an accepted or proposed ADR is a different kind of change from a code patch. It must record what was decided, why, what it costs, what lost, and what would reopen it. Do not bury an architecture change inside a feature PR.

## 4. Crate contract

The crate contract in `PLAN.md` §5 is not negotiable in a pull request. Crate names, public type names, and dependency edges are frozen there. To change any of them, propose an ADR.

## 5. Module contributions

Modules are compiled-in workspace crates, not runtime plugins (`docs/03-module-system.md` §7). A module is therefore a crate pull request, and it ships the whole set or it does not merge:

- Crate named `wicket-mod-<name>`, module identifier `mod-<name>`, schema named `<name>`. All three agree (`docs/11-module-common-rules.md`).
- A `module.toml` that is the single source of truth for the module's identity, permissions, machines, jobs, subscriptions and routes. The composition root reads that file; it does not carry a second copy.
- Forward and reverse migrations, with the reverse tested (`PLAN.md` §6 invariant 8).
- A `docs/validation.md` naming the tests that prove it, including that writes go through a transaction with an actor.
- Routes under the module's own namespace, declared with their method.
- A total signature declaration on every machine edge. Under the regulated profile, a required edge with no signature gate fails at startup, by design.
- An entry in the canonical module order and in both profile files.
- No new workspace dependency without a request for comment and an allowlist edit in the same change.

Propose the module first with the module-proposal issue form. Writing it before the identity is agreed wastes your time, not the maintainer's.

## 6. Code rules

- `just ci` is green before a pull request is opened (`PLAN.md` §11).
- No `unsafe`.
- No `todo!()` on `main`.
- Every migration has a tested reverse (`PLAN.md` §6 invariant 8).
- No stored balances: no column holds a running quantity or value that application code updates (`PLAN.md` §6 invariant 1).
- Tests that need Postgres skip locally and are required in CI (`PLAN.md` §7; `WICKET_REQUIRE_PG=1` makes a missing database a failure).
- Every source file carries `SPDX-License-Identifier: AGPL-3.0-or-later` (§9).
- No capability that only Rust can reach (§2, Goal 2).

## 7. How a change reaches main

`main` must move by fast-forward only. Squash-merge must be disabled, because squashing rewrites the signed commits that ADR 0006 requires. Enforcement of that GitHub setting is ABSENT (promised: `TODO.md` T-17): the repository currently permits squash, rebase, and merge commits. Closing those options is a repository Settings change the maintainer makes; a contributor cannot fix it in a pull request. Never force-push.

Within a wave or a concurrent batch of work, one writer owns a file. A second contributor who needs the same file must escalate and wait; they must not edit it in parallel and merge afterward. Three-way merges of concurrently edited files are how this project has repeatedly lost work: 39 of 41 `integrate: merge` commits in this repository are tagged overlapping ownership. A merge is not an integration strategy.

Hosted CI green is necessary and not sufficient. This project validates locally on three machines across three operating systems before a change lands, and hosted CI does not replace that gate for kernel, SQL, schema, lint-fence or contract changes. A documentation-only change needs only the local gate and hosted CI.

Expect the maintainer to land your change rather than pressing the button themselves; that is a consequence of the fast-forward and mirror rules, not a comment on your work.

## 8. Commit messages

Imperative mood, scoped prefix, one subject line. Examples already in this repository: `docs:`, `PLAN:`. For crate work, use the crate's short name (`core:`, `ledger:`, `db:`).

Every commit is signed off (`git commit -s`). The DCO line is required; a commit without it is not accepted.

## 9. License headers

New source files carry:

```
SPDX-License-Identifier: AGPL-3.0-or-later
```

Copyright of the original work is held by the project owner (Josh, as recorded in `git log`) as an individual (`docs/adr/0006-license.md`). Contributors retain copyright in their own contributions.

## 10. Issues and labels

Open an issue before a large change. The forms are bug, feature, module proposal, and request for comment; a blank issue is disabled on purpose, because every one of those four needs different information.

Labels come from a closed set: `type:*` for the kind of work, `area:kernel|module|docs|gov|ci` for where it lands, and `goal:agent|api|migrate|gov` for which goal it serves.

`good-first-issue` is only ever applied to documentation, licence headers, test names, validation documents, and lint scripts. It is never applied to the ledger, the audit chain, or the database session protocol. Those three are where a well-meaning first contribution does real damage, and no newcomer should be steered into them.

## 11. Security and conduct

Do not open a public issue for a vulnerability; follow `SECURITY.md`. Conduct concerns go to the contact in `CODE_OF_CONDUCT.md`.
