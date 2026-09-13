# 0006. License

Status:   **Accepted** (2026-09-12)
Date:     2026-09-11, accepted 2026-09-12
Decider:  project owner

## Decision

**AGPL-3.0-or-later for the code. Contributions under the Developer Certificate of Origin,
version 1.1. No contributor license agreement.** Copyright is held by the project owner as an
individual until a foundation or company is deliberately chosen; that choice is not blocked
by this record. The public repository is `https://github.com/BlinkingSun/wicket-erp`.

The owner's stated reason is the one the recommendation below rests on: the project exists
so that others can contribute. Declining a CLA gives up the option to sell commercial
exceptions later; that cost is accepted knowingly. While the owner is the only copyright
holder this decision remains cheap to change; from the first outside contribution it does
not.

## Context

This is the one decision in this directory that is genuinely irreversible. Relicensing
later requires the consent of every contributor who holds copyright in the work, and
past projects have failed to reach people who moved on, changed employers, or simply
stopped answering email. Deciding before the first outside contributor arrives costs
nothing. Deciding after is sometimes impossible.

Two related questions have to be answered together.

**What license governs the code.** This determines whether someone can take the work,
host it commercially, and give nothing back.

**What agreement governs contributions.** This determines whether the project retains
the ability to relicense or to sell commercial exceptions later.

There is also a dependency constraint. Adjacent tooling in this space links
OpenCASCADE, which is LGPL-2.1. Dynamic linking keeps that obligation contained, and
any future CAD module must respect it.

## Recommendation

**AGPL-3.0-or-later for the code, with a Developer Certificate of Origin for
contributions.** Presented as a recommendation rather than a decision because the
second half of it forecloses a commercial path that may matter later.

## The license options

**AGPL-3.0.** What ERPNext and Odoo Community use. Anyone may run, modify, and
distribute it. The distinguishing clause is that offering it to users over a network
counts as distribution, so a company hosting a modified version as a service must
publish their modifications.

For this project the network clause barely binds the actual customer. A shop
self-hosting for internal use triggers nothing, which is the entire target market. What
it does is prevent a cloud vendor from building a closed hosted product on the work.
That is the right protection to want here.

The cost is real. Some corporate legal departments refuse AGPL outright, occasionally
without reading it. That friction lands on exactly the kind of larger customer this
project is not targeting first, which makes it a cheap price now and a more expensive
one later.

**Apache-2.0.** Maximum adoption, no legal friction, explicit patent grant. Anyone may
host it commercially and contribute nothing. For infrastructure this is usually the
right answer. For an application with an obvious hosted business model attached, it
means the hosted business belongs to whoever gets there first with a sales team.

**MIT.** Apache without the patent grant. No reason to prefer it here.

**Open core, with a permissive core and proprietary regulated modules.** Rejected. The
regulated modules are the entire point of the project. Paywalling them means the
project's stated purpose is the part you cannot have, which is both dishonest and bad
strategy.

## The contribution agreement question

**Developer Certificate of Origin.** A sign-off line on each commit asserting the
contributor has the right to submit it. No copyright assignment, no paperwork, no
friction, and it is what Linux and most modern projects use. Contributors keep their
copyright, which means the project cannot unilaterally relicense later.

**Contributor License Agreement.** Contributors grant the project a broad license or
assign copyright. This preserves the ability to relicense and to sell commercial
exceptions to companies that cannot accept AGPL. It also measurably reduces
contribution, because a real number of developers will not sign one, and corporate
contributors need legal review to do so.

The tension is direct. A DCO maximizes contribution and forecloses dual licensing. A
CLA preserves the business option and costs contribution.

**The recommendation is DCO**, on the reasoning that this project's scarcest resource
is contributors, not future licensing optionality, and that a project which never
attracts contributors has no licensing options worth preserving. But this is a business
judgment rather than a technical one, and it belongs to whoever intends to live with the
consequences.

## Consequences of the recommendation

- Shops may run, modify, and self-host freely. Nothing is triggered.
- A vendor offering a hosted version must publish their modifications.
- The project can accept contributions with no paperwork.
- Selling commercial exceptions later is off the table, absent renegotiating with every
  contributor.
- Some enterprise evaluations will stop at the license. This is acceptable given the
  target market.

## What must happen before the first public commit

1. Choose a license and put the full text in `LICENSE`.
2. Choose DCO or CLA and document it in `CONTRIBUTING.md`.
3. Add the license header convention to the contributor guide.
4. Decide whether copyright is held by an individual, a company, or a foundation. This
   matters for enforcement and is easiest to arrange before there is anything to
   enforce.

## Revisit if

Do not revisit casually. Reopening a license decision after outside contributions exist
means finding and getting agreement from every one of them.
