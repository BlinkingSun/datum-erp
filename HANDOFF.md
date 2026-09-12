# Handoff — Datum

*Written 2026-09-11, at the end of the planning phase. For whoever picks this up next,
including a future version of the person who started it.*

---

## 1. What this is

An open source ERP for discrete manufacturing, aimed first at medical device
manufacturers of roughly ten to a hundred people, built so that everything regulated is
an optional module over a kernel that is compliance-aware from the first commit.

No code has been written. What exists is a plan that has survived a hostile review, nine
architecture decision records, and about 119,000 words of research. That is the deliverable
of this phase and it is deliberate: four of the founding assumptions turned out to be
wrong, and finding that out before thirteen crates were built on them was the entire point.

## 2. Status

| | |
|---|---|
| Phase | Planning complete. Execution not started. |
| Code | None. |
| Plan audit | Returned REVISE, 28 gaps. All blocking items resolved. |
| Decisions | 9 records. 0003 Accepted as amended. 0006 open. The rest Proposed. |
| Blocked on | Two calls from the project owner. See section 5. |
| Repository | Local only, branch `main`, two commits, no remote. |

## 3. Read these in this order

Half a day of reading. There is no shortcut, and skipping it means re-deriving decisions
that already have written reasoning.

1. **`docs/00-erp-primer.md`** — what an ERP does, taught by following one order of
   titanium bone screws from a customer request through to a recall query eighteen months
   later. Written for someone who has never worked inside one. Start here even if you
   think you know.
2. **`docs/01-vision-and-scope.md`** — who this is for and what it refuses to do.
3. **`docs/02-architecture.md`** — the kernel and module split, and the ledger.
4. **`docs/adr/`** — nine decisions, each with its costs stated. `README.md` indexes them.
5. **`PLAN.md`** — the three-wave build, seventeen invariants, and the crate contract.
6. **`research/README.md`** — where the evidence lives.
7. **`DESIGN.md`** — binding interface rules.

## 4. What is settled

Nine records in `docs/adr/`. The ones that carry the most weight:

**The ledger is the foundation** (0004). No stored balances anywhere. Every quantity and
value is the sum of immutable postings. Conservation holds per balance slice, not across a
whole transaction, because you cannot add feet to each. Quantity may cross an identity
boundary only inside a transaction explicitly declared a transformation, and value must
cross where matter may not. Every withdrawal names the postings it came from, so the
costing engine independently reproduces both the quantity and the money. That last part is
the only genuinely load-bearing check, because it is the only one where two engines that
could have disagreed must agree.

**Compliance is a property of the kernel, not a module** (0005). The governing rule, and
the single most useful sentence produced in this phase:

> Regulated workflows are modules. Regulated record properties are kernel.
> A module can add a process. A module cannot add a property to history.

**The audit trail is written by the database** (0005). A trigger attached automatically at
table creation, through a security-definer function. The application supplies the actor
transaction-locally and fails closed without one. It cannot write to the audit table at
all. Grant-level protection binds the application, not the database administrator, and the
record says so rather than overclaiming.

**Rust, PostgreSQL, single tenant, no general ledger** (0002, 0003, 0007, 0008). Each with
its costs written down.

**Datum never owns a database process lifecycle** (0003). On any operating system, in any
version.

## 5. What is not settled, and why it is blocking

**The visual approval gate.** Four mockups are in `design/`: a shop floor terminal, a
genealogy trace, an item master, and an application icon. They need a yes or a revise from
the project owner. No interface work starts until then, because iterating on a mockup is
cheap and iterating on shipped screens is not.

My own read: the shop floor terminal is right and is the most important screen in the
product. The genealogy trace is the right idea and the text in its side panel is a
generation artifact rather than a design. The item master is clean but far too sparse for a
real office screen and should be pushed harder before anyone builds it.

**The license** (`docs/adr/0006-license.md`). This is the one genuinely irreversible
decision and it must be settled before the first public commit, because relicensing later
needs consent from every contributor and that is sometimes impossible to obtain.

Recommendation: AGPL-3.0, with a developer certificate of origin rather than a contributor
license agreement. A shop self-hosting internally triggers nothing. A vendor offering a
hosted version must publish their changes. Declining a contributor agreement gives up the
ability to sell commercial exceptions later, and `research/background/open-source-governance.md`
is the reason: every confirmed death in this category traces to concentrated copyright plus
a vendor whose interests eventually diverged from the open edition, and the contributor
agreement is the mechanism that makes that legally possible.

You can have the option to change the license later, or the guarantee that nobody can take
it away. Not both.

## 6. What happens next

In order. Nothing here has started.

1. Settle the two items in section 5.
2. Re-audit the revised plan. The original audit ran on the grok master channel and the
   protocol allows two cycles; one has been used.
3. Write the Wave 1 specification. `research/audits/slice-wave1-stubs.md` section 4
   specifies exactly what a compiling stub must contain for a later lane to be able to
   test against it. That detail is not optional and it is easy to underestimate.
4. Partition Wave 1 into a manifest and run it. One workspace lane carrying a complete
   core crate, plus six documentation lanes that have no dependency on it.
5. Wave 2, in seven batches, with the ledger as the gate.
6. Then a vertical slice, not more breadth. See section 8.

## 7. What was rejected, so it is not re-litigated

Each of these was proposed, examined, and turned down for a written reason. If you want to
reopen one, read the reason first.

| Rejected | Where the reasoning lives |
|---|---|
| Microservices | ADR 0001 |
| Units of measure as Rust type parameters | ADR 0002, `research/decisions/core-quantity.md` |
| SQLite, or supporting two databases | ADR 0003 |
| Bundling PostgreSQL in the installer | ADR 0003 amendment, `research/decisions/install-story.md` |
| Every posting group summing to zero | ADR 0004, `research/decisions/ledger-invariant.md` |
| Audit trail as a module | ADR 0005 |
| A compliance mode that can be switched off | ADR 0005 |
| Building a general ledger | ADR 0007 |
| Multi-tenant SaaS, and a tenant identifier column | ADR 0008 |
| Runtime plugin loading in version 1 | `docs/03-module-system.md` section 5 |
| Open core, with regulated modules paywalled | ADR 0006 |

## 8. The four things most likely to go wrong

Stated plainly, because each has already nearly happened once.

**Stored balances will be reintroduced by accident.** It is what every developer's instinct
reaches for the moment a screen feels slow. Projections are a cache, they are rebuildable,
and the ledger is the truth. Any column holding a running total that application code
updates is a defect regardless of how well it performs.

**The ledger property tests will silently pass while testing nothing.** The standard Rust
test harness wraps each test in a transaction and rolls it back. Deferred constraint
triggers fire at commit. A rolled-back test never fires the constraint under test. The
suite must commit against a real database, and a deliberately failing case stays in it
permanently as a canary. This is written into `PLAN.md` section 7 and it is the single
easiest way to waste the entire architecture.

**Lot and serial numbers will be minted unconstrained.** Uppercase, digits, hyphen,
twenty characters or fewer, enforced at generation. Barcode standards cap the field, the
FDA restricts the character set, and one issuing agency allows only letters and digits.
Lots already etched onto implants in the field cannot be renumbered. The regulatory
research calls this the highest-value item it found and it is one validation rule.

**The project will build breadth before anything works end to end.** The competitive
review was blunt about this: the differentiating capability lives four phases out, and a
complete kernel demonstrates nothing to a shop. The answer in `PLAN.md` is a vertical
slice after Wave 2 — items, lots, the ledger, one work order, and a genealogy trace on
real data. Resist adding modules until that runs.

## 9. Practical notes

**Repository.** Local only, no remote, branch `main`, two commits. The author is recorded
as Josh with the email on this machine; that was a guess and should be corrected before
anything is pushed. `_team/` is excluded from version control as process scaffolding; the
substantive output was copied into `research/` and is tracked.

**Before the first public commit**, in addition to the license: a `LICENSE` file, a
`CONTRIBUTING.md` recording the contributor agreement decision, a `.gitignore`, and a
decision about who holds copyright. Those were scoped as a Wave 1 documentation lane and
have not been written.

**The name** is a placeholder. Datum is the reference feature everything else is measured
from in geometric tolerancing, and it is also the singular of data. It appears in one
constant so it stays cheap to change. Three alternatives are listed in
`docs/01-vision-and-scope.md`.

**On the multi-agent run that produced this.** Ten parallel adversarial review slices, four
decision lanes, two research spikes, and two reconciliation lanes, split evenly across two
model families so that no single one was marking its own work. The cross-family split is
what caught the real problems: the reviewer that proved the ledger rule meaningless was not
the one that wrote it. If you resume this work in the same harness, note that the board
reports lanes as DIED within a second of dispatch on Windows while the processes are in
fact alive and working. It is a display defect, it was flagged by the run's own watcher on
its first pass, and re-dispatching on it would double every lane.

---

## 10. The honest summary

The planning is good and it is not finished being wrong. Four foundations were corrected
this round and the record of each says what was wrong and why, because that is more useful
to the next person than a clean document would be.

The wedge is real but narrow. No open source ERP has a compliant electronic signature, and
that is the one thing that cannot be added later. Almost everything else this project would
claim as differentiating is either already available elsewhere or is a promise about a
phase that does not exist yet. The documents now say so.

What has not been tested at all is whether a real shop wants it. That is the next risk, and
no amount of further architecture will retire it.
