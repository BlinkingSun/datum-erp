# Regulatory requirements

Conforms to: [ADR 0005](adr/0005-compliance-in-kernel.md) as amended, and
`research/decisions/audit-persistence.md` (D3/D4).

This document is the map a customer's quality lead can read, and the index a
contributor uses to find the load-bearing line. It does not claim Wicket ships
validated, and it does not claim an installation that has not been validated.
Status in the map is where the requirement lives in the design: **kernel now**
means a Wave 1 / Wave 2 kernel crate or invariant; **Wave 2b** means
`wicket-esign`, `wicket-documents`, `wicket-print`, or `wicket-customfields`; **module
later** means a module that does not yet exist. Wave 1 of this repository ships
stubs. The slice does not already have a Wave 2b crate or a later module.

---

## 1. What applies to the beachhead

The beachhead is a 10-to-100 person medical-device manufacturer or contract
manufacturer — machined implants and instruments, titanium bone screws, small
electromechanical devices — subject to the US device quality system and, where it
keeps electronic records in place of paper, to Part 11. One paragraph per
regime. Every regulatory statement below cites a `research/background/` file
that holds the primary quote or, for ISO 13485, the flagged paraphrase.

### 1.1 21 CFR Part 11

Persons who use closed systems to create, modify, maintain, or transmit
electronic records must employ procedures and controls that ensure authenticity,
integrity, and, when appropriate, confidentiality, **and that the signer cannot
readily repudiate the signed record as not genuine**
(`research/background/regulatory.md` §2.1, quoting 21 CFR 11.10). That
non-repudiation sentence is the acceptance criterion the rest of this document
serves. The load-bearing clauses for an ERP are the audit trail in 11.10(e)
(computer-generated, time-stamped, independent of the operator, changes not
obscuring previously recorded information; 1997 preamble comments 73 and 76),
signature manifestation in 11.50, signature-to-record linking in 11.70, unique
and never-reassigned signatures in 11.100(a), two identification components in
11.200(a)(1), and uniqueness of the identification-code and password combination
in 11.300(a) (`research/background/regulatory.md` §§2.3–2.8). February 2026 CSA
guidance states that the 2003 Part 11 enforcement discretion over validation
**does not apply** to software used as part of production or the quality
management system under ISO 13485 4.1.6, 7.5.6, and 7.6
(`research/background/regulatory.md` §2.9).

### 1.2 21 CFR Part 820 as it now incorporates ISO 13485

The Quality Management System Regulation took effect 2 February 2026 (89 FR
7496). Design controls, production, acceptance, CAPA, and the rest of the old
subparts are gone from the CFR; they arrive by incorporation by reference of ISO
13485:2016 at 21 CFR 820.7(b), and 820.10(a) requires a manufacturer to
document a quality management system that meets ISO 13485 and the other
applicable requirements of Part 820. Failure against any applicable requirement
in that part renders a device adulterated under FD&C Act 501(h)
(`research/background/regulatory.md` §3.1). 21 CFR 820.70(i) no longer exists.
FDA's four overlays that hit an ERP directly are 820.10(b): UDI per Part 830
for ISO 7.5.8, traceability procedures per Part 821 for ISO 7.5.9.1 where
applicable, MDR per Part 803, and advisory notices per Part 806; 820.10(d)
extends ISO 7.5.9.2 implantable-grade traceability to life-supporting and
life-sustaining devices; 820.35(c) requires that **the UDI must be recorded for
each medical device or batch of medical devices**; 820.45 requires examination
of labeling (including UDI and expiry) and documented release of labeling for
use (`research/background/regulatory.md` §3.3). DHR, DMR, and DHF as named
record types are gone; the records are not. FDA's words: many former DHR
requirements now live in ISO 7.5.1's record of manufacture, and the MDF holds
the procedures and specifications that are current on the manufacturing floor
(`research/background/regulatory.md` §3.2).

### 1.3 21 CFR Part 821

Part 821 is device tracking. Both clocks in 21 CFR 821.25(a) start at a
request from FDA (`research/background/regulatory.md` §7.2, lines 653–654,
quoting 821.25(a)). Within 3 working days of a request from FDA, prior to
the distribution of a tracked device to a patient, the manufacturer must
name the holder and the location of the device. Within 10 working days of a
request from FDA, for tracked devices that are intended for use by a single
patient over the life of the device, after distribution to or implantation
in a patient, it must produce UDI, lot, batch, model or serial, ship date,
patient identity including SSN where available, prescribing physician, and
explant or death date. Records are kept for the useful life of each tracked device
(821.60). Residency is a regulation, not a preference: **"Records required to
be kept by this part shall be kept in a centralized point for each manufacturer
or distributor within the United States"** (`research/background/regulatory.md`
§7.2, quoting 821.50(b)). Patient names in a ledger or a hash chain collide with
European erasure duties; until a vault decision exists, kernel tables that
participate in the ledger, the genealogy graph, or the audit hash chain do not
carry civil identity (`docs/adr/0008-single-tenant.md`).

### 1.4 21 CFR Part 830 (UDI)

Part 830 is the unique device identification system. 820.10(b)(1) bolts it onto
ISO 13485 7.5.8: the manufacturer must document a system to assign UDI in
accordance with Part 830 (`research/background/regulatory.md` §3.3). 830.50(b)
requires a new device identifier whenever a new device package is created
(`research/background/regulatory-udi-aidc.md` §A1). 830.20(c) further restricts
UDI production identifiers to the ISO/IEC 646 invariant character set; 830.40(c)
forbids reassigning a device identifier
(`research/background/regulatory.md` §§1.0.4, 1.0.6). 820.35(c) is the ERP
hook: the UDI is recorded on the device or batch record, and complaint,
servicing, correction/removal, MDR, and tracking records all require it
alongside lot and serial (`research/background/regulatory.md` §1.0.7). DI
allocation, GUDID submission, barcode encoding, and direct part marking are a
later module. The kernel still has to own the structure those identifiers attach
to, or the module cannot land without rewriting history.

### 1.5 ISO 13485:2016 clauses cited in the research

ISO 13485:2016 clause language is copyrighted. This paragraph paraphrases
`research/background/regulatory.md` §4 and quotes only text FDA itself quotes.
Clause 4.1.6 is the validation hook for QMS software: documented procedures,
validate before initial use and as appropriate after changes, approach
proportionate to risk. Clause 4.2.3 is the medical device file — FDA, verbatim:
"the MDF will contain or reference the procedures and specifications that are
current on the manufacturing floor." Clause 4.2.4 controls documents; records
are a special type of document controlled under 4.2.5 (FDA Comment 35). Clause
4.2.5 requires records to remain legible, identifiable, and retrievable for
at least the lifetime of the device and not less than two years from release.
Clause 7.5.1 is the record of manufacture for each batch or device — quantity
manufactured, quantity approved for distribution, traceability — the DHR by
another name. Clauses 7.5.8 and 7.5.9 are identification and traceability, including
product status and, for implantables, components, materials, and work-environment
conditions. Clause 7.6 requires that an out-of-tolerance gage trigger assessment
of every prior result. Clause 8.3 makes quarantine, hold, reject, rework, and
concession inventory states with a signed disposition. These are the clauses an
ERP for this beachhead actually stores records for. Buy the standard before
printing clause language in a customer deliverable
(`research/background/regulatory.md` §4, provenance).

---

## 2. Requirement-to-design map

One row per requirement `research/background/regulatory.md` §1 identified as
unretrofittable. §1.1 numbers fifteen (the original four plus eleven more). §1.0
adds four that fail the same test, plus the catalogue-number no-reuse caveat in
§1.0.6. A module can add a process. A module cannot add a property to history
(`PLAN.md` §6a).

Every "where it lives" cell names a `PLAN.md` §6 invariant number, a crate in
`PLAN.md` §5, a Wave 2s module named in `PLAN.md` §3, or both. A row whose home
is a Wave 2b crate or a later module is marked in **Status**. Those rows are
not in the slice.

| Requirement | Citation | Where it lives | Status |
|---|---|---|---|
| Append-only audit trail, written independently of the operator, per-record queryable, reason captured on the write path | `research/background/regulatory.md` §1.1 item 1, §2.3 (11.10(e); preamble comments 73, 76) | inv 3; `wicket-audit` | kernel now |
| Electronic signature as a first-class object bound to a record version | `research/background/regulatory.md` §1.1 item 2, §§2.6–2.7 (11.50, 11.70) | inv 14, 15; `wicket-esign` (implements `SignatureGate`) | Wave 2b |
| Record immutability and a reconstructible version chain | `research/background/regulatory.md` §1.1 item 3 (11.10(b)–(c)) | inv 16 (no hard delete of a record); `wicket-documents` (version chain) | kernel now (inv 16); Wave 2b (version chain) |
| Lot- or unit-aware append-only ledger with genealogy edges | `research/background/regulatory.md` §1.1 item 4 (ISO 7.5.1, 7.5.9; 820.35(c); 806.10(c)(9)–(11); 821.25(a)) | inv 1, 2, 10; `wicket-ledger` | kernel now |
| Server-side time; client clocks never stored as time of record | `research/background/regulatory.md` §1.1 item 5 (11.10(e) "computer-generated, time-stamped") | inv 4; `wicket-audit`, `wicket-db` | kernel now |
| Identity lifecycle: no deletion, no identifier reuse | `research/background/regulatory.md` §1.1 item 6 (11.100(a), 11.300(a)) | inv 13; `wicket-identity` | kernel now |
| Signing credential separable from login; session boundaries audited. The 11.200(a)(1)(i) continuous-session relaxation is not implemented | `research/background/regulatory.md` §1.1 item 7 (11.200(a)(1)–(3)); design in `research/decisions/audit-persistence.md` §9 | inv 14; `wicket-identity`; `wicket-esign` | Wave 2b |
| Server-side workflow state machines enforcing permitted sequencing, including 11.10(h) device identity on the audit row | `research/background/regulatory.md` §1.1 item 8 (11.10(f)–(h)) | `wicket-statemachine`; `source_device_id` on `wicket-audit` | kernel now (state machine and device column); module later (device-check enforcement) |
| Effectivity and revision on master data, distinct from record versioning; work order snapshots the revision set at release | `research/background/regulatory.md` §1.1 item 9 (ISO 4.2.3 / 4.2.4; MDF to DHR) | `wicket-documents`; BOM, routing, spec, packaging, and label artwork modules consume it | Wave 2b |
| Deterministic record-rendering service, versioned independently of the UI | `research/background/regulatory.md` §1.1 item 10 (11.10(b), 11.50(b)) | `wicket-print` | Wave 2b |
| Content-addressed immutable blob storage for signed attachments | `research/background/regulatory.md` §1.1 item 11 (11.70) | `wicket-documents` | Wave 2b |
| Retention clock, legal hold, and no hard delete of a record, enforced at the database | `research/background/regulatory.md` §1.1 item 12 (ISO 4.2.5; 821.60; 806.20(c); 830.360) | inv 16; `wicket-db` grants. Expected life as an item-master attribute that drives the clock is module-owned | kernel now (inv 16); module later (retention clock) |
| Tenancy with a data-residency boundary | `research/background/regulatory.md` §1.1 item 13 (821.50(b); 11.10(b)) | Installation identity and declared residency in `wicket-module`. No `tenant_id`. ADR 0008. | kernel now |
| Software version and configuration version stamped on every record | `research/background/regulatory.md` §1.1 item 14 | inv 17; `wicket-audit` | kernel now |
| Declarative configuration layer with its own versioning, approval, and audit trail | `research/background/regulatory.md` §1.1 item 15 | inv 18; `wicket-module`. Approval of a configuration change uses `wicket-documents` / `wicket-esign` | kernel now (the layer); Wave 2b (approval) |
| Package hierarchy (each, inner, case, pallet, contained quantity, parent link) | `research/background/regulatory.md` §1.0.2 (830.50(b) is the DI rule; the hierarchy is inventory) | inv 11; consumed by `wicket-ledger` | kernel now. DI-per-package-level is module later |
| Lot and serial identifiers constrained at generation: `[0-9A-Z-]`, at most 20 characters | `research/background/regulatory.md` §1.0.4; three sources in §4.1 below | inv 9; `wicket-numbering` | kernel now |
| Expiry stored with a precision, never a bare date | `research/background/regulatory.md` §1.0.5; `research/background/regulatory-udi-aidc.md` §E (`yymmd0`, `YYMM00`) | inv 12 | kernel now |
| Catalogue number (the identifier the UDI module derives DIs from) is no-edit and no-reuse | `research/background/regulatory.md` §1.0.6 (830.40(c)) | Item-master number (`PLAN.md` §3 Wave 2s `mod-items`). Compressed PCN is a module field. | kernel field with the item record (Wave 2s); compressed PCN is module later |
| Stable UDI attachment point on lot, serial, and shipment records | `research/background/regulatory.md` §1.0.7 (820.35(c)) | Native nullable kernel column on lot, serial, and shipment records (`PLAN.md` §3 Wave 2s `mod-lots`; shipping later). Never `wicket-customfields`. | kernel column with the record (Wave 2s); population = module later |

Invariant 19 (gap-free regulated document numbers; `wicket-numbering`) is not in
the §1 list. It is a D3 obligation (`research/decisions/audit-persistence.md` §8)
and is stated in §3.7 so it is not lost.

---

## 3. Electronic records and signatures (Part 11)

### 3.1 Audit trail

21 CFR 11.10(e) requires secure, computer-generated, time-stamped audit trails
that independently record the date and time of operator entries and actions
that create, modify, or delete electronic records, and that record changes shall
not obscure previously recorded information
(`research/background/regulatory.md` §2.3). Preamble comment 73: independently
means the trail is not under the control of the operator and is created
independently of the operator. Comment 76: all changes to existing records need
to be documented, regardless of the reason. FDA's 2024 electronic-systems
guidance adds that audit trails should include the reasons for the changes, be
protected from modification and from being disabled, and be retained in a
searchable and sortable format (`research/background/regulatory.md` §2.3).

The design: a row trigger, attached automatically at `CREATE TABLE`, writes the
entry in the same transaction as the change. The application supplies actor and
business intent transaction-locally and fails closed when it cannot. The
application role holds `SELECT` on the audit table and holds no `INSERT`,
`UPDATE`, `DELETE`, or `TRUNCATE`. Entries reach the table only through the
security-definer trigger (`PLAN.md` §6 inv 3; ADR 0005 as amended;
`research/decisions/audit-persistence.md`). A module author cannot forget an
audit row, because they never write one. That is kernel now, in `wicket-audit`.
It is not a log file. It is a store with a per-record retrieval path.

A record — anything an audit trigger attests to, or that a history-bearing
table references — is retired by state change, never by `DELETE`. This is a
privilege fact: `wicket_app` holds no `DELETE` on schema `app` and no `TRUNCATE`
in any schema. Working state that carries no history (sessions, idempotency
keys, completed job rows, projection caches) lives in schema `transient`, where
`DELETE` is granted and expected. `ON DELETE CASCADE` is prohibited in every
schema, `transient` included (`PLAN.md` §6b inv 16;
`research/background/regulatory.md` §1.3 trap 2). One cascade permanently
removes the history of rows the author never looked at, and it is found at
inspection.

### 3.2 Time source

Time of record is `now()` inside the trigger, constant for the transaction, so
the twelve audit rows of one work-order completion share one time of record.
Intra-transaction order is `clock_timestamp()` in a separate column. No
timestamp is a bound parameter. Storage is `timestamptz`, UTC
(`research/decisions/audit-persistence.md` §4; `PLAN.md` §6 inv 4).

The time source is the **host clock**. Clock administration is a customer SOP.
Wicket records clock changes it can observe (`audit.log_event` on detected
backward jumps between statements). 11.10(e) requires "time-stamped", not
traceable to UTC via authenticated NTP
(`research/background/regulatory.md` §2.3; `research/decisions/audit-persistence.md`
§4). Server-side time is not trusted time. See §6.

For signatures, 11.50(a)(2) is the date and time when the signature was executed.
Preamble comment 101: the signer's local time is the one to be recorded
(`research/background/regulatory.md` §2.6). The signature row therefore stores the
UTC instant plus the signer's IANA zone captured at signing. A bare UTC stamp
cannot reconstruct that; a bare local stamp cannot be ordered. Capturing the zone
is Wave 2b (`wicket-esign`).

### 3.3 Identity lifecycle

11.100(a): each electronic signature shall be unique to one individual and shall
not be reused by, or reassigned to, anyone else. 11.300(a): no two individuals
have the same combination of identification code and password
(`research/background/regulatory.md` §2.8). Users deactivate. They are never
deleted. Usernames are never recycled. The user record outlives every record it
signed (`PLAN.md` §6b inv 13; `wicket-identity`; kernel now). An administrator who
deletes `jsmith` and later recreates `jsmith` makes every old signature on a bone
screw DHR ambiguous (`research/background/regulatory.md` §1.3 trap 4).

The signing credential is separable from the login credential, or single
sign-on later leaves nothing to re-prompt for (`PLAN.md` §6b inv 14;
`research/background/regulatory.md` §1.1 item 7). 11.200(a)(3) means
administrator tooling must make impersonation structurally impossible: resetting
a signing credential must not let one person alone assume another's identity
(`research/decisions/audit-persistence.md` §9). Shared accounts do not identify
the individual who released a lot (`research/background/regulatory.md` §1.1
item 6).

Identity is kernel now. The signing primitive that consumes it is Wave 2b.

### 3.4 Signature manifestation

11.50(a) requires three things associated with the signing: the printed name of
the signer, the date and time when the signature was executed, and the meaning
(review, approval, responsibility, authorship). 11.50(b) subjects those items
to the same controls as electronic records and requires them in any
human-readable form of the record (`research/background/regulatory.md` §2.6).

Two preamble rulings dictate schema. Comment 102: an identification code is not a
name; snapshot the display name at signing; do not join to a live user table at
render time (`PLAN.md` §6b inv 15). Comment 101: store UTC plus the signer's
zone, as §3.2. Meaning is an enumerated value on the signature, not inferred from
the screen (`research/background/regulatory.md` §2.6).

`wicket-esign` stores the manifestation. `wicket-print` renders it inline on the
human-readable copy. Both are Wave 2b. The slice does not have them. A transition
that declares a signature requirement and runs under `NoSignatures` is refused
with a typed error (`PLAN.md` §5, `SignatureGate`).

Every signing uses all identification components, every time. Wicket does not
implement the 11.200(a)(1)(i) continuous-session relaxation in v1. A session cookie
on a shared work-centre tablet next to a mill turning titanium bone screws is not
a component "designed to be used only by the individual"
(`research/decisions/audit-persistence.md` §9; ADR 0005). Relaxing later is a
policy flag. Tightening later, after a customer has validated the loose
behaviour, is not.

### 3.5 Signature/record linking

11.70: electronic signatures shall be linked to their respective electronic
records so that the signatures cannot be excised, copied, or otherwise
transferred to falsify an electronic record by ordinary means. Preamble comment
107: a technology-based link is necessary; procedural or administrative controls
alone are not sufficient (`research/background/regulatory.md` §2.7).

The design: sign a content hash of the exact serialized record version; store the
hash on the signature; make verification a first-class operation
(`research/decisions/audit-persistence.md` §9; ADR 0005). `SignatureToken`
carries `record_content_hash` (SHA-256 of the canonical record bytes at
`record.version`). `wicket-esign` loads the row, confirms both identification
components at mint, the stored hash, the live record at that version, the
permission snapshot taken at mint, the meaning, and the single-use claim
(DECISION D-W1-4, `research/decisions/traits-profiles.md` Q2; Wave 2b). The
statemachine, not the module author, calls `verify` on every `Required` edge
and fails closed. A release build whose enabled set contains a `Required` edge
while `NoSignatures` is bound fails at startup.

### 3.6 Tamper evidence

The sentence the project uses, quoted from `research/decisions/audit-persistence.md`
§7, and not improved upon:

> Every change to a regulated record in Wicket is written to the audit trail by the
> database itself, inside the same transaction as the change, with the operator's
> identity, the server time, the prior and new values, and the reason where one is
> required; a write that cannot be attributed to an authenticated operator is refused
> rather than recorded as unknown. The application — including any module, and including
> a defective one — can read the audit trail but cannot insert, alter, or delete an
> entry: that is enforced by database privileges, not by application code. Wicket does not
> claim the trail cannot be altered by someone with administrative control of the
> database server itself; instead, each transaction is sealed into a hash chain whose
> head is published off the server on a schedule you control, so that any later
> alteration of stored history is detectable by verifying an exported copy against
> those off-server records on a separate machine — which is the check your periodic
> audit-trail review performs.

That is tamper-**evident**, not tamper-proof, and evident only for periods an
off-box anchor covers (`research/decisions/audit-persistence.md` §§6.3, 7).
See §6.

### 3.7 Gap-free numbering

Regulated document numbers — a work order, a DHR, a nonconformance — are
allocated from a counter row in the caller's transaction, never from a
PostgreSQL sequence. A committed number is never reused. Cancellation is a visible
status, not a missing number (`PLAN.md` §6b inv 19;
`research/decisions/audit-persistence.md` §8; `wicket-numbering`; kernel now).
`nextval()` leaves a gap when a transaction rolls back. "Why is there no work
order WO-2026-0416" is a question an inspector asks; a voided document with a
reason is an answer.

---

## 4. Traceability and UDI

### 4.1 Lot and serial identifier constraint

The kernel mints lot and serial numbers. The generator is constrained to
`[0-9A-Z-]`, at most twenty characters, before the first lot of mill heat is
written (`PLAN.md` §6b inv 9; `wicket-numbering`). Lots already etched on a bone
screw in the field cannot be renumbered. Three sources, none of them optional:

1. GS1 AI (10) batch/lot and AI (21) serial are `X..20`, CSET 82 — cap 20
   characters (`research/background/regulatory.md` §1.0.4;
   `research/background/regulatory-udi-aidc.md` §E).
2. 21 CFR 830.20(c) further restricts to the ISO/IEC 646 invariant character set
   (`research/background/regulatory.md` §1.0.4).
3. HIBCC permits only `A-Z` and `0-9` in the relevant fields
   (`research/background/regulatory.md` §1.0.4;
   `research/background/regulatory-udi-aidc.md` §A3, ANSI/HIBC 2.6 §2.1.1).

The intersection the kernel enforces is `[0-9A-Z-]`, ≤20. That is engineering,
not a GS1 citation. GS1 CSET 82 permits lowercase; no normative GS1 document
recommends the restricted subset (`research/background/regulatory-udi-aidc.md`
§C2). Do not cite GS1 for the extra restriction. Cite 830.20(c), the 20-character
cap, and HIBCC.

### 4.2 Tracked entity

21 CFR 801.45 (direct part marking of reusable, reprocessed devices), ISO 13485
7.5.9.2 (implantable traceability), and 21 CFR 821.25(a)(2) (per-patient device
tracking) all operate at unit level (`research/background/regulatory.md` §1.0.3).
If the kernel's tracked entity is a lot, adding serialization later changes the
primary key of every downstream table — WIP, pick, pack, shipment line, return,
service, complaint. The tracked entity is a lot **or** a unit within a lot, from
the first posting (`PLAN.md` §6b inv 10). A finished serial of a titanium bone
screw traces to its lot and its mill heat. This is kernel now. Genealogy is a
directed acyclic graph of consumption edges on `wicket-ledger`, not a parent
column (`research/background/regulatory.md` §7.3).

### 4.3 Package hierarchy

21 CFR 830.50(b) requires a device identifier per package level, and it is
tempting to treat the whole hierarchy as UDI-shaped. It is not. Each, inner,
case, pallet, contained quantity, and parent link are core inventory structure.
The ledger cannot express "received 2 cases = 48 eaches" of bone screws without
it. Retrofitting package levels changes the effective unit of measure on every
historical posting (`research/background/regulatory.md` §1.0.2; `PLAN.md` §6b
inv 11).

**Correct split: package hierarchy is kernel; DI-per-package-level is a later
module.** The module attaches identifiers to a structure the kernel already
owns. GS1 Healthcare GTIN Allocation Rules §5.3.5 carves out sterile-barrier
packaging: multiple barrier levels are not a packaging level for GTIN allocation
(`research/background/regulatory-udi-aidc.md` §A1). `is_sterile_barrier` is
therefore a module-owned column on a **kernel** packaging-level table, not on
the item master. That column lands with the UDI module; the table it hangs on
is kernel now. `wicket-customfields` (Wave 2b) is the extension mechanism that
must support module columns on kernel tables.

### 4.4 Expiry precision

GS1 AI (17) use-by / expiry is `N6,yymmd0`. The linter "additionally permitting
YYMM00 format indicating an unspecified day" — meaning end of month
(`research/background/regulatory-udi-aidc.md` §E). If the kernel stores expiry as
a `DATE`, it invents a day at write time and the UDI module can never recover the
distinction (`research/background/regulatory.md` §1.0.5). Store expiry as a value
plus a precision, or as a string with a precision discriminator (`PLAN.md` §6b
inv 12). Kernel now. A lot received with a month-only expiry keeps that precision
through storage and the API.

### 4.5 UDI attachment point

820.35(c): the UDI must be recorded for each medical device or batch of medical
devices. Complaint records, servicing records, correction/removal reports, MDR
Block D, and device tracking all require UDI **alongside** lot and serial
(`research/background/regulatory.md` §1.0.7). Lot, serial, and shipment records
carry a native, nullable, module-populated kernel column so the UDI module does
not have to alter the ledger. The column is created with those records
(`PLAN.md` §3 Wave 2s `mod-lots`; shipping later). It is never a
`wicket-customfields` field. Populating it, allocating DIs, submitting to GUDID,
and encoding symbols are the Phase 6 UDI module, later.

One catalogue number the UDI module derives DIs from is not recycled. 830.40(c)
forbids reassigning a DI (`research/background/regulatory.md` §1.0.6). HIBCC's
compressed PCN is a module field; the source catalogue number is a core field
on the item master (`PLAN.md` §3 Wave 2s `mod-items`) with a no-edit, no-reuse
invariant.

HIBCC Basic UDI-DI currently has two conflicting formats on Commission servers
(`research/background/regulatory-udi-aidc.md` §B). That question is open. This
document does not pick one.

---

## 5. Validation posture

Wicket is not "validated software". The customer validates their installation
against their intended use. GPSV §4.10, which survived the February 2026 CSA
revision of GPSV §6: regardless of the distribution of tasks, contractual
relations, source of components, or the development environment, **the device
manufacturer or specification developer retains ultimate responsibility for
ensuring that the software is validated**
(`research/background/regulatory.md` §6.4). CSA's live equivalent: manufacturers
are responsible for determining the appropriate assurance activities
(`research/background/regulatory.md` §6.4). Identical software scores opposite
process-risk in FDA's own ERP examples, because one shop has a qualified person
check mill-heat bar stock before it enters a bone-screw work order and the other
does not (`research/background/regulatory.md` §6.2). The vendor cannot know which
shop this is.

### 5.1 What the manifests give a customer

`docs/03-module-system.md` §8 is the source of this split.

- **Module manifest.** Each module declares `id`, version, dependencies,
  permissions, whether `regulated = true`, and which transitions require a
  signature. A module marked `regulated = true` gets mandatory validation
  documentation, mandatory reverse migrations, and inclusion in the
  configuration manifest.
- **Configuration manifest.** The running system produces a hashed, exportable
  list of every module, every version, every enabled state. That attachment is
  part of the customer's installation qualification. Changing the module set is a
  change-control event in the customer's own quality system; the software records
  the change, who made it, when, and against what approval. The configuration
  manifest also lists every state-machine edge's `SignatureDeclaration` —
  `Required` and `NotRequired` with reasons — in both installation profiles
  (DECISION D-W1-4, `research/decisions/traits-profiles.md` Q2 (c)).
- **Per-module validation documents.** Each first-party module ships intended
  use, requirements, and executable test protocols
  (`docs/03-module-system.md` §8). A customer validating only the modules they
  enabled is doing less work than one validating a monolith. That is the
  commercial point of the architecture, and it is a cost reduction, not a
  completed validation.

Vendor evidence a customer can start from is the CSA §V.A.5 list: SDLC,
software QA, cybersecurity including SBOM, and the data-integrity controls
(retaining records, generating complete copies, the audit trail, access
controls, electronic signature controls)
(`research/background/regulatory.md` §6.2). For some lower-risk functions that
may be all the assurance the manufacturer needs. It is still their
determination.

Per release, CSA Appendix A Example 4 (SaaS PLM, the closest published analogue)
asks for a change summary scoped to the functions this customer registered, the
vendor's test results for those changes, and enough detail for the customer's
impact assessment (`research/background/regulatory.md` §6.5). ISO 4.1.6 says
validate "as appropriate, after changes". That is not "revalidate every
release".

### 5.2 What the customer still owns

The customer owns intended use, the risk score against their own process, PQ /
UAT, the validation summary, periodic review, and every procedural Part 11
clause no product can discharge: 11.10(i) training, 11.10(j) accountability
policies, 11.100(b) identity verification, 11.300(b)–(c) password aging and
token-loss management, and the 11.100(c)(1) certification to FDA signed with a
traditional handwritten signature (`research/background/regulatory.md` §§2.5,
2.8, 6.4). They own the off-box hash-chain anchor and the periodic audit-trail
review that makes tamper evidence mean something
(`research/decisions/audit-persistence.md` §6.2). They own clock administration
(§3.2). Feature gating lets them choose when to activate a new function so our
deployment cadence is not their validation cadence
(`research/background/regulatory.md` §6.5).

Customer-specific behaviour lives in declarative configuration, never in
bespoke code shipped to one customer (`PLAN.md` §6b inv 18;
`research/background/regulatory.md` §1.1 item 15). Shipping custom code moves
that installation into a stricter validation category permanently, including
every future change to it. That looks like a product decision. It is an
architectural one. GAMP 5 category designations in the research are
secondary-sourced (`research/background/regulatory.md` §6.3); this document
does not assign Wicket a category.

---

## 6. What is not claimed

Three things this project does not say, and will not say.

**Trusted time.** Time of record is the host clock. The shop administrator is the
host administrator. A clock set backwards, a restored VM snapshot, or a `date`
command produces audit rows with whatever the box believed. A hash chain does
not fix this — a backdated row hashes perfectly
(`research/decisions/audit-persistence.md` §4). Wicket records observed backward
jumps. It does not claim NTP, a timestamp authority, or traceability to UTC.

**Tamper-proof storage.** Grants stop the application, including a defective
module, from writing the trail. They do not bind the person who administers the
database server. On a self-hosted install that person is the customer. The
honest mechanism is a per-transaction hash chain whose head is published off the
server, verified on a different machine, for the periods an off-box anchor
covers (`research/decisions/audit-persistence.md` §§6–7). Tamper-evident, not
tamper-proof. "The audit trail cannot be altered" is not a sentence this project
makes.

**Compliance out of the box.** Installing Wicket does not complete a customer's
quality system, their validation, their Part 11 procedures, or their FDA
correspondence. The kernel makes the record properties in §2 structurally
present so they do not have to be retrofitted. The customer still validates the
installation. A Part 11 matrix that claimed full coverage would be an overclaim:
several 11.10 and 11.100 / 11.300 clauses are procedural, and 11.100(c)(1)
requires a handwritten certification the software cannot send
(`research/background/regulatory.md` §6.4). The configuration manifest, the
module validation documents, and the IQ-shaped exporter are inputs to that
work. They are not a substitute for it.
