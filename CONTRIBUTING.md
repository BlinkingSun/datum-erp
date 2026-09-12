# Contributing to Datum

Conforms to: [ADR 0006](docs/adr/0006-license.md) (Accepted); crate graph as frozen in `PLAN.md` §5.

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

## 2. Architecture decisions

Decisions that would be expensive to reverse live in `docs/adr/`. The format, status values, and index are in `docs/adr/README.md`.

A pull request that changes an accepted or proposed ADR is a different kind of change from a code patch. It must record what was decided, why, what it costs, what lost, and what would reopen it. Do not bury an architecture change inside a feature PR.

## 3. Crate contract

The crate contract in `PLAN.md` §5 is not negotiable in a pull request. Crate names, public type names, and dependency edges are frozen there. To change any of them, propose an ADR.

## 4. Code rules

- `just ci` is green before a pull request is opened (`PLAN.md` §11).
- No `unsafe`.
- No `todo!()` on `main`.
- Every migration has a tested reverse (`PLAN.md` §6 invariant 8).
- No stored balances: no column holds a running quantity or value that application code updates (`PLAN.md` §6 invariant 1).
- Tests that need Postgres skip locally and are required in CI (`PLAN.md` §7; `DATUM_REQUIRE_PG=1` makes a missing database a failure).

## 5. Commit messages

Imperative mood, scoped prefix, one subject line. Examples already in this repository: `docs:`, `PLAN:`. For crate work, use the crate's short name (`core:`, `ledger:`, `db:`).

Every commit is signed off (`git commit -s`). The DCO line is required; a commit without it is not accepted.

## 6. License headers

New source files carry:

```
SPDX-License-Identifier: AGPL-3.0-or-later
```

Copyright of the original work is held by the project owner (Josh, as recorded in `git log`) as an individual (`docs/adr/0006-license.md`). Contributors retain copyright in their own contributions.
