# Research

Audience: contributor. Status: historical+living index.

Roughly 119,000 words of adversarial review and primary-source research, produced
2026-09-11 before any code was written. It exists because four of the project's founding
assumptions turned out to be wrong, and the record of *why* they were wrong is worth more
than the corrections alone.

Read the decisions if you want to know what was settled. Read the audits if you want to
know what nearly went wrong. Read the background if you are about to touch anything
regulated.

## decisions/

Four blocking architecture questions, each answered in full, each having amended the
corresponding record in `docs/adr/`. These are binding.

| File | Settles |
|---|---|
| `ledger-invariant.md` | What conservation actually means when a bar of titanium becomes five hundred screws, and which constraints are load-bearing rather than decorative |
| `core-quantity.md` | The frozen public contract of the core crate: dimensions, units, money, and why nothing in the core ever rounds |
| `audit-persistence.md` | How the audit trail is really guaranteed, and the honest sentence a customer can repeat to an investigator |
| `install-story.md` | How a shop gets a running system, and why bundling a database was buying something it could not deliver |

## audits/

`plan-audit.md` is the consolidated verdict on the original plan. It returned REVISE with
28 gaps. The eleven `slice-*.md` files are the parallel adversarial reviews it was built
from, five run on Cursor and five on Grok, each told to attack one assumption.

The four that changed the project most:

- `slice-ledger.md` proved the original zero-sum rule was meaningless across unlike items.
- `slice-typed-quantity.md` proved units of measure cannot be Rust type parameters.
- `slice-audit-persistence.md` proved there is no interception point in SQLx to hang the
  audit guarantee on.
- `slice-competitive.md` checked the project's own sales pitch against live sources and
  found three of five claims overstated.

## background/

Primary-source research, with URLs and provenance markers. Slower reading, longer shelf
life.

| File | Contains |
|---|---|
| `regulatory.md` | 21 CFR Part 11, QMSR, ISO 13485 clauses, device records, validation. Its section 1 is the kernel-versus-module synthesis that reshaped the architecture. Start there. |
| `regulatory-udi-aidc.md` | Device identification and barcode standards in detail. Mostly relevant to a much later phase, with four exceptions that are kernel and are flagged as such. |
| `competitive-landscape.md` | Every open source and commercial option assessed, with real contract pricing. |
| `open-source-governance.md` | How open source ERP projects have died. Read before settling the license. |

## How to treat this

It is evidence, not scripture. Every file states its own confidence and its open gaps.
Two evidence gaps were never closed and are named in `competitive-landscape.md`, and one
regulatory question about a competing identifier format is named as unresolved in
`regulatory-udi-aidc.md`. Where a document is paraphrasing a standard rather than quoting
it, it says so.

If you are about to contradict something here, that is allowed. Write down why, the way
the decision records do.

Decisions made during the foundation build (2026-09-12), promoted from the run's excluded working directory:

- `decisions/w1-contracts.md` — D-W1-1 (money column `numeric(24,6)`, `unit_cost_applied numeric(24,8)`; AMENDMENT A1 to `ledger-invariant.md` §5.2) and D-W1-2 (`wicket_app` holds DELETE only in schema `transient`; invariant 16 reworded to records).
- `decisions/traits-profiles.md` — D-W1-3 (`PostingSink`, final text), D-W1-4 (`SignatureGate`, final text, total `SignatureDeclaration` on regulated edges), D-W1-5 (installation profiles: runtime enablement, module-owned signature requirements, the eleven `SPEC-profiles` keys).
- `decisions/w2-rulings.md` — Wave 2 (kernel) rulings and adjudications, promoted at the Wave 2 close.
- `decisions/w2s-rulings.md` — Wave 2s (module slices) rulings R-2s-1…
- `decisions/w2b-rulings.md` — Wave 2b rulings D-2b-1…9 (wicket-esign semantics)
