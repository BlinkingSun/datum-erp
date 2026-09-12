# Regulatory spike: what an ERP/MRP must be to live inside a medical device quality system

**Date of research:** 2026-09-11
**Status:** primary-source where marked; paraphrase and secondary-source flagged inline.

---

## Provenance and currency

The QMSR took effect **2 Feb 2026** and FDA's Computer Software Assurance guidance was re-issued **3 Feb 2026**. Most secondary material online is stale — anything citing 21 CFR 820.70(i) as live, or CSA as draft, is wrong and should be discarded.

**Primary sources, fetched directly and quoted verbatim below:** 21 CFR Part 11 (full text + the 1997 final-rule preamble), Part 820 both pre- and post-QMSR, the QMSR final rule preamble (89 FR 7496), the December 2025 technical amendments, Parts 830 / 801 / 821 / 806 / 803 / 7, the February 2026 CSA guidance (full PDF), FDA's Data Integrity Q&A, FDA's October 2024 electronic systems guidance, the GS1 Barcode Syntax Dictionary, GS1 General Specifications Release 25.0, ANSI/HIBC 2.6.

eCFR and federalregister.gov block automated fetchers on their human-facing pages; their APIs do not. `govinfo.gov` also serves CFR text as XML, e.g.
`https://www.govinfo.gov/content/pkg/CFR-2023-title21-vol8/xml/CFR-2023-title21-vol8-sec830-50.xml`

**PARAPHRASE / SECONDARY — not primary, treat accordingly:**

- **ISO 13485:2016 clause text is paraphrased, not quoted.** The standard is copyrighted and I could not obtain a clean source. The only ISO clause text quoted verbatim in this document is text that FDA itself quotes in the QMSR preamble or in the CSA guidance, and those instances are marked. **Buy the standard before printing clause language in any deliverable.** https://www.iso.org/standard/59752.html
- **GAMP 5 Second Edition** appendix designations and the software-category count rest on secondary sources; ISPE paywalls the book. Verify against your own copy.
- **EUDAMED module dates** come from the European Commission's EUDAMED overview page, not from the text of Regulation (EU) 2024/1860 — EUR-Lex blocks automated fetch.

**Open gaps at time of writing:**

1. The **GUDID Data Elements Reference Table** itself was not retrieved. `§830.310` (quoted in full below) remains the authoritative element list until you obtain it from FDA directly.
2. The text of **Regulation (EU) 2024/1860** was not retrieved.
3. **ISO 13485 clause text** — see above.
4. **GAMP 5 Second Edition** — see above.
5. One open question in the UDI companion document (`spike-regulatory-udi.md`): two conflicting HIBCC Basic UDI-DI formats are in circulation, both on Commission servers. Confirm with HIBCC before coding.

---

# 1. SYNTHESIS — kernel vs. module vs. trap

## 1.0 Verdict on the proposed split

Your stated architecture: kernel carries audit trail, electronic signature, record immutability, and a lot-aware append-only ledger; everything regulated is an optional module; UDI is a Phase 6 module; `is_sterile_barrier` and `pcn_compressed` are module-owned extension fields on the item master.

**The split is mostly right. Five things are wrong or underspecified, and four of them are unfixable later.**

### 1.0.1 "Everything regulated is an optional module" is the wrong framing

Your kernel list of four is incomplete. At least eleven further regulated requirements cannot be modules, because a module cannot retroactively install them into records the kernel already wrote. They are enumerated in §1.1 below: server-side time; identity lifecycle with no deletion and no identifier reuse; the "continuous period of controlled system access" concept plus a signing credential separable from the login credential; server-side state machines enforcing permitted sequencing; master-data effectivity and revisioning; a deterministic record-rendering service; content-addressed immutable blob storage; retention clock + legal hold + a no-hard-delete invariant; a data-residency boundary in the tenancy model; app-version and config-version stamping on every record; and a declarative configuration layer.

Reframe the rule as: **regulated *workflows* are modules; regulated *record properties* are kernel.** A module can add a process. A module cannot add a property to history.

### 1.0.2 The package hierarchy is kernel, not UDI module — this is the biggest correction

21 CFR 830.50(b) requires a DI per package level, and it is tempting to read the whole package-level concept as UDI-shaped. It is not. The **hierarchy itself** — each / inner / case / pallet, contained quantity, parent link — is core inventory and logistics structure:

- The ledger cannot express "received 2 cases = 48 eaches" without it.
- Pick/pack/ship, SSCC-identified logistics units, and the §806.10(c)(11) "number of devices distributed to each such consignee" query all read it.
- Retrofitting package levels changes the effective unit of measure on every historical posting.

**Correct split: package hierarchy = kernel; DI-per-package-level = Phase 6 module.** The module attaches identifiers to a structure the kernel already owns. A consequence for your extension mechanism: `is_sterile_barrier` is a property of a *packaging level*, so it is a module-owned column on a **core** table, not on the item master. Confirm your extension mechanism supports module columns on kernel tables, not only on the item master.

### 1.0.3 "Lot-aware" is not enough — the ledger's unit of record must be lot-OR-unit from the first commit

21 CFR 801.45 (direct part marking of reusable, reprocessed devices), ISO 13485 7.5.9.2 (implantable traceability), and 21 CFR 821.25(a)(2) (per-patient device tracking) all operate at unit level. If the kernel's tracked entity is a lot, adding serialization later changes the primary key of every downstream table — WIP, pick, pack, shipment line, return, service record, complaint link.

Ship Phase 1 with a **tracked entity** abstraction that resolves to a lot *or* a unit-within-lot, even if nothing is serialized yet. This costs one indirection now and a rewrite later.

### 1.0.4 Lot and serial *generation constraints* are kernel, and this is the single highest-value item in this document

The kernel mints lot and serial numbers. If it mints them unconstrained — 32-character UUIDs, or strings containing `/`, `.`, spaces, or lowercase — then when the UDI module lands in Phase 6:

- GS1 AI (10) and AI (21) cap at **20 characters**, CSET 82.
- 21 CFR 830.20(c) further restricts to the **ISO/IEC 646 invariant character set**.
- HIBCC permits **only `A-Z` and `0-9`** in the relevant fields.

You cannot retroactively renumber lots that are already printed on product in the field. **Constrain the kernel's identifier generator to `[0-9A-Z-]`, ≤20 characters, before Phase 1 ships.** One line now; an unfixable data problem in Phase 6.

### 1.0.5 Expiry-date precision is kernel

GS1 AI (17) uses the `yymmd0` format, which explicitly permits `YYMM00` — "unspecified day," meaning end of month. If the kernel stores expiry as a `DATE`, it invents a day at write time and the UDI module can never recover the distinction. Store expiry as (value, precision) or as a string with a precision discriminator. Same class of trap as §1.0.4.

### 1.0.6 Where your split is right

- **UDI as a Phase 6 module: correct.** DI allocation and lifecycle, GUDID/EUDAMED submission, barcode encoding, symbology selection, verification grading, direct part marking — all genuinely deferrable, provided §1.0.2–1.0.5 are in the kernel.
- **`pcn_compressed` as a module extension field: correct**, with one caveat. The compressed product/catalogue number is derived and HIBCC-specific. But the *source* catalogue number is a core field, and §830.40(c) forbids ever reassigning a DI — which is derived from it. So the kernel needs a **no-edit, no-reuse invariant on whatever identifier the module derives DIs from**. If the kernel lets an admin recycle a catalogue number, the module cannot honour the regulation.
- **`is_sterile_barrier` as a module-owned field: correct** — it only affects DI allocation. See §1.0.2 on which table it belongs to.

### 1.0.7 One more hook to leave in the kernel

21 CFR 820.35(c): *"the UDI must be recorded for each medical device or batch of medical devices."* Complaint records (§820.35(a)(3)), servicing records (§820.35(b)(2)), correction/removal reports (§806.10(c)(5)), MDR Block D (§803.52(c)(4)) and device tracking (§821.25(a)(2)(i)) all require UDI *alongside* lot/serial. Leave a stable, module-populated UDI attachment point on the kernel's lot, serial and shipment records so Phase 6 does not have to alter the ledger.

---

## 1.1 Architectural: in the kernel from the first commit

**Your four, confirmed:**

1. **Append-only audit trail, server-generated, operator-independent, per-record queryable.** §11.10(e); 1997 preamble comment 73 ("not under the control of the operator... created independently of the operator"); comment 76 ("All changes to existing records need to be documented, regardless of the reason"); FDA 2024: "searchable and sortable"; FDA Data Integrity Q&A: reviewers "should review the audit trails... as they review the rest of the record." **Not a log file — a store**, with a per-record retrieval path, a reason-for-change field captured at the write path, and coverage of admin actions, bulk operations and deletes.
2. **Electronic signature as a first-class object bound to a record version.** §11.50 + §11.70 + preamble comment 107 ("a technology based link is necessary... procedural or administrative controls alone are [not] sufficient"). Sign a content hash of the exact serialized version; store the signer's display name **snapshotted**; store an enumerated meaning; store the UTC instant **plus the signer's time zone**.
3. **Record immutability and revisioning.** §11.10(b)–(c). Every business object has a version chain; any historical version is reconstructible and renderable exactly as it was signed.
4. **Lot/serial-aware append-only ledger with genealogy edges.** ISO 7.5.1 and 7.5.9; §820.35(c); §806.10(c)(9)–(11); §7.46(a); §821.25(a). Append-only postings; balances are folds over postings, never columns. **See §1.0.3 — the unit of record must be lot-OR-unit.**

**Eleven more that are equally unretrofittable:**

5. **Server-side time.** §11.10(e) "computer-generated, time-stamped"; NTP-disciplined; client timestamps never accepted; UTC plus signer zone for §11.50.
6. **Identity lifecycle with no deletion and no identifier reuse.** §11.100(a) ("shall not be reused by, or reassigned to, anyone else") and §11.300(a) (uniqueness of the ID+password combination). Users deactivate, never delete; usernames never recycle; the user record outlives every record it signed. No shared accounts — FDA: *"When login credentials are shared, a unique individual cannot be identified through the login... **Shared, read-only user accounts** that do not allow the user to modify data or settings are acceptable for viewing data, but they do not conform... for actions, such as second person review, to be attributable to a specific individual."*
7. **"Single, continuous period of controlled system access" as a real concept in the auth layer.** §11.200(a)(1)(i)–(ii). **The signing credential must be separable from the login credential** — otherwise SSO/OIDC leaves you nothing to re-prompt for. Session boundaries must be audited. §11.200(a)(3) additionally means your admin tooling must make impersonation structurally impossible, not merely policy-prohibited.
8. **Server-side workflow state machines enforcing permitted sequencing.** §11.10(f)–(h). Cannot ship before release; cannot release before acceptance; cannot consume quarantined stock; cannot print a label without a released artwork revision. If the state machine lives in the UI you have no control. Include §11.10(h) device checks: which terminal, scanner or printer may originate which transaction.
9. **Effectivity and revision on master data, distinct from record versioning.** ISO 4.2.3 / 4.2.4, the MDF↔DHR link. BOM, routing, spec, packaging config and label artwork each need effective-from/to plus revision identity, and every work order snapshots the revision set in force at release.
10. **A deterministic record-rendering service.** §11.10(b) and §11.50(b). Versioned templates, reproducible output, signatures rendered inline. The UI is not a renderer.
11. **Content-addressed immutable blob storage.** Signed records reference attachments; a path-based reference someone can overwrite breaks §11.70 silently.
12. **Retention clock, legal hold, and a no-hard-delete invariant enforced at the database layer.** ISO 4.2.5 (device lifetime, not less than two years from release); §821.60 (useful life); §806.20(c) (2 years beyond expected life); §830.360 (3 years past end of marketing). "Expected life" is an item-master attribute that drives retention.
13. **Tenancy with a data-residency boundary.** §821.50(b) requires US-centralized tracking records; EU customers will require EU residency; §11.10(b) requires per-customer export. A single global region with a `tenant_id` column cannot satisfy any of this later.
14. **Software version and configuration version stamped on every record.** CSA change assessment requires the customer to know which build and which configuration produced a record.
15. **A declarative configuration layer with its own versioning, approval and audit trail.** This is what keeps customers at **GAMP Category 4**. If customer-specific behaviour lives in per-tenant code you have manufactured Category 5 for every customer and permanently raised their validation bill. It looks like a product-management decision; it is an architectural one.

Plus, from §1.0: **package hierarchy**, **constrained lot/serial generation**, **expiry precision**, and a **UDI attachment point** on lot/serial/shipment records.

---

## 1.2 Safe as optional modules

These genuinely bolt on, **provided the core already exposes stable immutable identifiers for them to reference**:

- Supplier qualification, scorecards, audit scheduling (ISO 7.4) — as long as the PO snapshots supplier approval status at issue.
- Calibration / equipment management (ISO 7.6) — as long as the acceptance record carries an equipment ID from day one and the reverse query is indexable.
- Internal audit (8.2.4), management review, training records (6.2) — structurally inert.
- CAPA (8.5.2/8.5.3), complaint handling (8.2.2), MDR filing — normally eQMS; the ERP needs only unrenumbered link targets.
- Demand planning, MRP regeneration, finite scheduling, costing, AP/AR/GL. Note that CSA puts accounting **out of scope entirely**.
- Label template design and the print engine — but the *data* printed and the *print event record* are core.
- Barcode / AIDC scanning UI, mobile clients.
- EDI / ASN, carrier integration, customer and supplier portals.
- **UDI (Phase 6)**: DI allocation and lifecycle, GUDID (SPL/ESG) and EUDAMED submission, barcode encoding and symbology, verification grading, direct part marking — **provided** DI-per-issuing-agency, DI history, package-level DI attachment and Basic UDI-DI have somewhere to live in the core schema, and provided §1.0.2–1.0.5 are already in the kernel.
- Analytics / BI, scheduled reporting.
- MES / shop-floor data collection — separable, but the genealogy contract must be fixed before either side ships.
- Multi-step approval routing, delegation, escalation — layered on the signature *primitive*, which is not optional.
- Nonconformance investigation workflow — but the inventory **states** (quarantine / hold / reject / rework / concession) are core ledger states.

---

## 1.3 Traps — look deferrable, are not

1. **"We'll add the audit trail later with an ORM hook."** You cannot retro-fill history; every pre-existing record becomes unprovable. A generic hook also misses reason-for-change, cascade deletes, bulk/admin operations and direct SQL.
2. **Any hard delete anywhere, especially FK cascade deletes.** One `ON DELETE CASCADE` permanently violates §11.10(e) for those rows, and you find out at inspection.
3. **`UPDATE` in place on transactional quantities.** Once inventory is a mutable balance column you can never reconstruct as-of state or genealogy. Converting to a ledger later rewrites every posting path *and still cannot recover the lost history.*
4. **Username reuse and user deletion.** §11.100(a) forbids reassignment. An admin who deletes `jsmith` and later recreates `jsmith` retroactively makes every old signature ambiguous.
5. **Rendering the signer's name by joining to a live user table.** People change names. §11.50(a)(1) requires the printed name of the signer *as of the signing*. Snapshot it.
6. **SSO added after the fact.** §11.200(a)(1) needs two distinct components and a re-prompt at session boundaries. If your only credential lives at the IdP you have nothing to ask for at signing time.
7. **Naive local datetimes, or accepting client clocks.** You cannot order events, and you cannot render "the signer's local time" per preamble comment 101. Converting a `datetime` column to tz-aware later is lossy and unprovable.
8. **Lot-only inventory, "we'll serialize later."** See §1.0.3.
9. **One barcode field per SKU / no package hierarchy.** See §1.0.2. Retrofitting package levels rewrites every label, every GUDID submission and every scan-to-transact path.
10. **One "regulatory identifier" field.** You need DI per issuing agency (§830.40(a) permits several), superseded-DI history (§830.310(b)(2), §830.360), DPM DI where different, and **Basic UDI-DI / GMN** — which GS1 says is allocated *independently* of the GTIN, is 1:n to it, and *"SHALL NOT be used for supply chain identification or transactional purposes."*
11. **Unconstrained lot and serial generators.** See §1.0.4. This is the one to fix this week.
12. **A `DATE` column for expiry.** See §1.0.5.
13. **GTIN stored as an integer.** Leading zeros are meaningful for GTIN-12/U.P.C. and must be preserved. Normalize to 14 *and* keep a format discriminator; never round-trip by trimming.
14. **BOM tables with no effectivity.** After your first engineering change you cannot answer "what spec was this lot built to" — which is the entire point of the DHR↔MDF relationship.
15. **Label printing as a fire-and-forget side effect.** §820.45(b) requires the release of labeling for use to be documented; §820.45(a) requires examination for correct UDI, expiry, storage, handling and processing instructions; and the QMSR preamble requires that *"a designated individual must examine, at a minimum, a representative sampling of all labels that have been checked by automatic readers."* The print event and verification sampling are records.
16. **An audit trail nobody can review.** A single unpartitioned table that can only be dumped fails the review obligation. You can add indexes later; you cannot add a review workflow to an unreviewable firehose.
17. **Continuous deployment with no release identity in the data.** Without app-version and config-version stamps the customer cannot do the CSA change assessment.
18. **Patient PII colliding with GDPR.** Part 821 requires patient names, addresses and SSNs where available, retained for the device's useful life; EU customers owe erasure rights. Tokenize PII behind a vault boundary from the start so erasure does not break the ledger's hash chain or genealogy edges.
19. **Assuming Part 11's validation enforcement discretion covers you.** It does not — CSA says so in terms (quoted in §6). And the inverse trap: selling "revalidate every release." ISO 4.1.6 says "as appropriate, after changes."
20. **Shipping bespoke code per customer.** It converts that scope to GAMP Category 5 for that customer, forever, including every future change to it.

**One line:** the three things a vendor controls that actually move a customer's validation bill are how much of their requirement is met by *configuration* rather than custom code, whether risky automation has a **mandatory human confirmation step** (FDA's own ERP examples turn on exactly this), and whether every release ships a scoped change summary with the vendor's test results and per-customer feature gating. Everything in §1.1 is unbuyable after the fact.

---

# 2. 21 CFR Part 11

Full text: https://www.ecfr.gov/current/title-21/chapter-I/subchapter-A/part-11
Final rule preamble (62 FR 13430, 20 Mar 1997 — the interpretive gold): https://www.federalregister.gov/documents/1997/03/20/97-6833/electronic-records-electronic-signatures

*All Part 11 text in this section is verbatim primary source.*

## 2.1 §11.10 chapeau — the acceptance criterion

> "Persons who use closed systems to create, modify, maintain, or transmit electronic records shall employ procedures and controls designed to ensure the authenticity, integrity, and, when appropriate, the confidentiality of electronic records, **and to ensure that the signer cannot readily repudiate the signed record as not genuine.**"

Non-repudiation is the acceptance criterion. Every design decision in §1.1 serves it.

## 2.2 §11.10(b)–(c) — copies and retention

> "(b) The ability to generate accurate and complete copies of records in **both human readable and electronic form** suitable for inspection, review, and copying by the agency."
> "(c) Protection of records to enable their accurate and ready retrieval **throughout the records retention period**."

FDA's 2003 guidance recommends portable formats — PDF, XML, SGML — that "preserve the content and meaning of the record." Design consequence: a deterministic rendering service versioned independently of the UI. "Open the screen and print it" fails (c) the moment the UI changes.

## 2.3 §11.10(e) — the audit trail clause

> "Use of **secure, computer-generated, time-stamped audit trails to independently record the date and time of operator entries and actions that create, modify, or delete electronic records. Record changes shall not obscure previously recorded information.** Such audit trail documentation shall be retained for a period at least as long as that required for the subject electronic records and shall be available for agency review and copying."

Preamble comment 73 on "independently":

> "The word ''independently'' is intended to require that **the audit trail not be under the control of the operator and, to prevent ready alteration, that it be created independently of the operator.**"

Comment 76, which kills partial coverage:

> "The agency disagrees with the suggested revision because the rewording is too narrow. The agency believes that some record changes may not be ''updates'' but significant modifications or falsifications disguised as updates. **All changes to existing records need to be documented, regardless of the reason**, to maintain a complete and accurate history, to document individual responsibility, and to enable detection of record falsifications."

Also from the preamble: *"audit trail information may be contained as part of the electronic record itself or as a separate record. FDA does not intend to require one method over the other."*

FDA's operative definition, from *Data Integrity and Compliance With Drug CGMP: Questions and Answers* (Dec 2018) — **a drug guidance, but this is the definition device auditors use** — https://www.fda.gov/media/119267/download :

> "For purposes of this guidance, audit trail means a secure, computer-generated, time-stamped electronic record that allows for **reconstruction of the course of events** relating to the creation, modification, or deletion of an electronic record... Audit trails include those that track creation, modification, or deletion of data (such as processing parameters and results) **and those that track actions at the record or system level (such as attempts to access the system or rename or delete a file).**"

And the review obligation, which is what makes this a schema requirement rather than a logging requirement:

> "Audit trail review is similar to assessing cross-outs on paper when reviewing data. Personnel responsible for record review under CGMP should review the audit trails that capture changes to data associated with the record **as they review the rest of the record**."

FDA's newest statement is in *Electronic Systems, Electronic Records, and Electronic Signatures in Clinical Investigations: Questions and Answers*, **final October 2024** (clinical-trial scoped, but it is FDA's current thinking on cloud, IT service providers, audit trails and risk-based validation, and auditors read it across) — https://www.fda.gov/media/166215/download :

> "To ensure the trustworthiness and reliability of electronic records, audit trails must capture electronic record activities including all changes made to the electronic record, the individuals making the changes, and the date and time of the changes **and should include the reasons for the changes. Audit trails should be protected from modification and from being disabled.**"
> "**FDA recommends that the audit trail be retained in a format that is searchable and sortable.**"

## 2.4 §11.10(f)–(h) — checks

> "(f) Use of **operational system checks to enforce permitted sequencing of steps and events**, as appropriate.
> (g) Use of **authority checks** to ensure that only authorized individuals can use the system, electronically sign a record, access the operation or computer system input or output device, alter a record, or perform the operation at hand.
> (h) Use of **device (e.g., terminal) checks** to determine, as appropriate, the validity of the source of data input or operational instruction."

(f) means the workflow state machine is a compliance control, not UX. (g) is authorization at four distinct granularities — system, signature, device/IO, record. (h) is the forgotten one: which scanner, which label printer, which terminal may originate this transaction.

## 2.5 §11.10(i)–(k) — the ones no vendor can ship

> "(i) Determination that persons who develop, maintain, or use electronic record/electronic signature systems have the education, training, and experience to perform their assigned tasks.
> (j) The establishment of, and adherence to, written policies that hold individuals accountable and responsible for actions initiated under their electronic signatures, in order to deter record and signature falsification.
> (k) Use of appropriate controls over systems documentation including:
> (1) Adequate controls over the distribution of, access to, and use of documentation for system operation and maintenance.
> (2) Revision and change control procedures to maintain an audit trail that documents time-sequenced development and modification of systems documentation."

## 2.6 §11.50 — signature manifestations

> "(a) Signed electronic records shall contain information associated with the signing that clearly indicates all of the following: (1) **The printed name of the signer**; (2) **The date and time when the signature was executed**; and (3) **The meaning** (such as review, approval, responsibility, or authorship) associated with the signature.
> (b) The items identified in paragraphs (a)(1), (a)(2), and (a)(3) of this section shall be subject to the same controls as for electronic records and **shall be included as part of any human readable form of the electronic record (such as electronic display or printout).**"

Two preamble rulings that dictate schema:

- **User ID is not a name** (comment 102): *"The agency intends that the printed name of the signer be displayed for purposes of unambiguous documentation and to emphasize the importance of the act of signing to the signer. The agency believes that because an identification code is not an actual name, it would not be a satisfactory substitute."* → snapshot the display name at signing time; do not join to a mutable user table at render time.
- **Time zone** (comment 101): *"Regarding systems that may span different time zones, the agency advises that **the signer's local time is the one to be recorded.**"* → store the UTC instant *plus* the signer's IANA zone/offset captured at signing. A bare UTC timestamp cannot reconstruct this; a bare local timestamp cannot be ordered.

## 2.7 §11.70 — signature/record linking

> "Electronic signatures and handwritten signatures executed to electronic records shall be **linked to their respective electronic records** to ensure that the signatures cannot be **excised, copied, or otherwise transferred to falsify an electronic record by ordinary means.**"

Comment 107 rules out doing this procedurally:

> "FDA recognizes that, because it is relatively easy to copy an electronic signature to another electronic record and thus compromise or falsify that record, **a technology based link is necessary. The agency does not believe that procedural or administrative controls alone are sufficient** to ensure that objective because such controls could be more easily circumvented than a straightforward technology based approach."

Practical implementation: sign a **content hash of the exact serialized record version**, store the hash with the signature, and make verification a first-class operation.

## 2.8 §11.100 / §11.200 / §11.300 — identity

> "§11.100(a) **Each electronic signature shall be unique to one individual and shall not be reused by, or reassigned to, anyone else.**
> (b) Before an organization establishes, assigns, certifies, or otherwise sanctions an individual's electronic signature, or any element of such electronic signature, the organization shall verify the identity of the individual.
> (c) Persons using electronic signatures shall, prior to or at the time of such use, certify to the agency that the electronic signatures in their system, used on or after August 20, 1997, are intended to be the legally binding equivalent of traditional handwritten signatures.
> (1) **The certification shall be signed with a traditional handwritten signature** and submitted in electronic or paper form. Information on where to submit the certification can be found on FDA's web page on Letters of Non-Repudiation Agreement."

*[amended 88 FR 13018, 2 Mar 2023 — the mailing address was replaced by the web-page pointer]*

> "§11.200(a)(1) Employ at least **two distinct identification components** such as an identification code and password.
> (i) When an individual executes a series of signings during a **single, continuous period of controlled system access**, the first signing shall be executed using all electronic signature components; **subsequent signings shall be executed using at least one electronic signature component that is only executable by, and designed to be used only by, the individual.**
> (ii) When an individual executes one or more signings **not** performed during a single, continuous period of controlled system access, each signing shall be executed using **all** of the electronic signature components.
> (2) Be used only by their genuine owners; and
> (3) Be administered and executed to ensure that attempted use of an individual's electronic signature by anyone other than its genuine owner **requires collaboration of two or more individuals.**"

> "§11.300(a) Maintaining the uniqueness of each combined identification code and password, such that **no two individuals have the same combination** of identification code and password.
> (b) Ensuring that identification code and password issuances are periodically checked, recalled, or revised (e.g., to cover such events as password aging).
> (c) Following loss management procedures to electronically deauthorize lost, stolen, missing, or otherwise potentially compromised tokens, cards, and other devices that bear or generate identification code or password information, and to issue temporary or permanent replacements using suitable, rigorous controls.
> (d) Use of transaction safeguards to prevent unauthorized use of passwords and/or identification codes, and to **detect and report in an immediate and urgent manner any attempts at their unauthorized use** to the system security unit, and, as appropriate, to organizational management.
> (e) Initial and periodic testing of devices, such as tokens or cards, that bear or generate identification code or password information to ensure that they function properly and have not been altered in an unauthorized manner."

§11.200(a)(3) is the clause that forbids an administrator from being able to set a user's signing credential and then sign as them.

## 2.9 Current enforcement posture

**Scope guidance still governs:** *Part 11, Electronic Records; Electronic Signatures — Scope and Application*, September 2003, still final.
https://www.fda.gov/regulatory-information/search-fda-guidance-documents/part-11-electronic-records-electronic-signatures-scope-and-application · PDF https://www.fda.gov/media/75414/download

FDA narrowed Part 11 to records required under predicate rules and kept electronically in lieu of, or relied on in place of, paper; and announced enforcement discretion over §11.10(a) validation, §11.10(e)/(k)(2) audit trails, §11.10(b) copies, §11.10(c) retention and legacy systems — while enforcing *"all predicate rule requirements, including predicate rule record and recordkeeping requirements."*

**But that discretion does not reach a production/QMS system.** The February 2026 CSA guidance closes the door explicitly:

> "As discussed in the Electronic Records guidance, FDA intends to exercise enforcement discretion regarding specific Part 11 requirements for validation of computerized systems used to create, modify, maintain, or transmit electronic records (see 21 CFR 11.10(a) and 11.30). **But the enforcement discretion policy described in the Electronic Records guidance (concerning validation of computerized systems used to create, modify, maintain, or transmit electronic records) expressly does not apply to validation requirements for computer software used as part of production or the quality management system arising under Subclauses 4.1.6, 7.5.6, and 7.6 of ISO 13485.**"

**And CSA gives the clearest current test for when Part 11 attaches to a manufacturing system:**

> "For computer software used as part of production or the quality management system, the applicable predicate rules include those under Part 820. A document required under Part 820—**including, but not necessarily limited to, a document Part 820 requires to bear a signature**—and maintained in electronic form would generally be an 'electronic record' under Part 11 (see 21 CFR 11.3(b)(6)). To determine when a record is required under Part 820, manufacturers should consider, among other things, **whether the record would be necessary as evidence to document required validation.** If a manufacturer maintains in electronic form a document required under Part 820, then Part 11 generally applies."

---

# 3. QMSR — Part 820 as it now exists

**Final rule:** "Medical Devices; Quality System Regulation Amendments," **89 FR 7496**, published 2 Feb 2024, **effective 2 Feb 2026**.
https://www.federalregister.gov/documents/2024/02/02/2024-01709/medical-devices-quality-system-regulation-amendments · PDF https://www.govinfo.gov/content/pkg/FR-2024-02-02/pdf/2024-01709.pdf

**Technical amendments:** document 2025-21955, published 4 Dec 2025, effective 2 Feb 2026 — purely editorial, conforming cross-references in Parts 801, 803, 812, 860 and the classification regulations from old §820.30 / §820.180 / §820.198 to §820.10(c) / §820.35. *"This rule does not impose any new requirements on affected parties."*
https://www.federalregister.gov/documents/2025/12/04/2025-21955/medical-devices-quality-management-system-regulation-technical-amendments

**Current text:** https://www.ecfr.gov/current/title-21/chapter-I/subchapter-H/part-820

## 3.1 What Part 820 is now, in its entirety

- **Subpart A:** §820.1 Scope · §820.3 Definitions · §820.5 [Reserved] · §820.7 Incorporation by reference · §820.10 Requirements for a quality management system
- **Subpart B:** §§820.20–820.30 [Reserved] · §820.35 Control of records · §820.40 [Reserved] · §820.45 Device labeling and packaging controls
- **Subparts C–O [Reserved]**

Design controls, purchasing, production and process controls, acceptance, nonconforming product, CAPA, labeling, handling/storage/distribution, records, servicing and statistics are **gone from the CFR** and arrive by incorporation by reference:

> "§820.7(b) ISO 13485:2016(E) ('ISO 13485'), *Medical devices—Quality management systems—Requirements for regulatory purposes*, Third edition, March 1, 2016; IBR approved for §§ 820.1, 820.3, 820.10, 820.35, and 820.45."
> "§820.10 ... (a) *Document.* Document a quality management system that complies with the applicable requirements of ISO 13485 (incorporated by reference, see § 820.7) and other applicable requirements of this part...
> (e) *Enforcement.* The failure to comply with any applicable requirement in this part renders a device **adulterated** under section 501(h) of the Federal Food, Drug, and Cosmetic Act."

**21 CFR 820.70(i) no longer exists.** FDA states this itself in CSA footnote 3: this rule *"removed the majority of the current requirements in Part 820, **including 21 CFR 820.70**."* Any compliance material citing 820.70(i) as live needs rewriting around ISO 13485 4.1.6 / 7.5.6 / 7.6 via §820.10(a).

## 3.2 DHR / DMR / DHF / QSR: terms gone, records not

Preamble response to Comment 31 (89 FR 7507–08), verbatim:

> "FDA agrees with the comments to the extent that they correctly identify that ISO 13485 does not contain requirements for record types specified in the QS regulation, such as quality system record (QSR), DMR, DHF, and DHR. As stated in the QMSR proposed rule, **we are not retaining separate requirements for these record types in the QMSR and have eliminated terms associated with these specific record types** because we believe the elements that comprise those records are largely required to be documented by ISO 13485, including Clause 4.2 and its subclauses, and Clause 7 and its subclauses. For example, **many of the requirements previously in the DHR are largely required to be in the medical device or batch record, as described in Clause 7.5.1.**"
>
> "Similarly, consistent with the former DHF, **Clause 7.3.10 requires the design and development file** to contain or reference all the records necessary to establish compliance with design and development requirements, including the design and development plan and design and development procedures."
>
> "Clause 4.2.3 requires that the MDF will contain or reference the procedures and specifications **that are current on the manufacturing floor**. The final design output from the design phase, which is maintained or referenced in the design and development file, forms the basis or starting point for the MDF. Previously, product specifications, procedures for manufacturing, measuring, monitoring, and servicing, and requirements for installation were included in a manufacturer's DMR and **will now be located in the manufacturer's MDF.**"

"MDF" now appears in binding text — §820.3(a) defines *Rework* as *"action taken on a nonconforming product so that it will fulfill the specified requirements **in the medical device file (MDF)** before it is released for distribution."*

## 3.3 The four FDA overlays that hit an ERP directly

**§820.10(b)** bolts US regulations onto specific ISO clauses:

> "(1) For Clause 7.5.8 in ISO 13485, **Identification**, the manufacturer must document a system to assign unique device identification to the medical device **in accordance with the requirements of part 830** of this chapter.
> (2) For Clause 7.5.9.1 in ISO 13485, **Traceability—General**, the manufacturer must document procedures for traceability in accordance with the requirements of **part 821** of this chapter, if applicable.
> (3) For Clause 8.2.3 in ISO 13485, Reporting to regulatory authorities, the manufacturer must notify FDA of complaints that meet the reporting criteria of **part 803** of this chapter.
> (4) For Clauses 7.2.3, 8.2.3, and 8.3.3, advisory notices shall be handled in accordance with the requirements of **part 806** of this chapter."

**§820.10(d)** — implantable-grade traceability, scoped by device *function*, not class:

> "*Devices that support or sustain life.* Manufacturers of devices that support or sustain life, the failure of which to perform when properly used in accordance with instructions for use provided in the labeling can be reasonably expected to result in a significant injury, must comply with the requirements in **Traceability for Implantable Devices, Clause 7.5.9.2 in ISO 13485**, in addition to all other applicable requirements in this part, as appropriate."

FDA declined to narrow it and confirmed it replaces old §820.65 wholesale: *"much of the QS regulation is being removed or amended, including Sec. 820.65. Instead, the QMSR incorporates the traceability requirements set forth in Clause 7.5.9 of ISO 13485, including Clause 7.5.9.2."* §820.3(b) pins "implantable medical device" to the §860.3 definition of *implant* — *"a device that is placed into a surgically or naturally formed cavity of the human body... intended to remain implanted continuously for a period of 30 days or more."*

**§820.35 Control of records** — additive to ISO 4.2.5. The load-bearing paragraph for an ERP:

> "**(c) Unique Device Identification.** In addition to the requirements of Clauses 7.5.1, 7.5.8, and 7.5.9 in ISO 13485, **the UDI must be recorded for each medical device or batch of medical devices.**"

Plus complaint records that must carry *"(3) Any unique device identifier (UDI) or universal product code (UPC), and any other device identification(s)"* and servicing records that must carry *"(2) Any UDI or UPC, and any other device identification(s)"*.

**§820.45 Device labeling and packaging controls** — retained from old §820.120 because, in FDA's words, *"many device recalls are related to labeling and packaging"*:

> "(a) The manufacturer must ensure labeling and packaging has been examined for accuracy prior to release or storage where applicable, to include the following:
> (1) The correct unique device identifier (UDI) or universal product code (UPC), or any other device identification(s);
> (2) Expiration date;
> (3) Storage instructions;
> (4) Handling instructions; and
> (5) Any additional processing instructions.
> (b) **The release of the labeling for use must be documented** in accordance with Clause 4.2.5 of ISO 13485."

And the constraint that automated label-verification designs miss (89 FR 7516):

> "FDA notes that in its experience, manufacturers have recalled devices where automated readers have not caught label errors. The requirement to inspect labeling and packaging does not preclude automatic readers where that process is followed by human oversight. **A designated individual must examine, at a minimum, a representative sampling of all labels that have been checked by automatic readers.**"

## 3.4 On signatures, FDA relaxed then clarified (Comment 53)

FDA removed the proposed rule's blanket *"obtain the signature for each individual who approved or re-approved the record,"* then said:

> "FDA notes that where ISO 13485 uses the term ''approved,'' that term means that an approved document, or certain record of a type that requires approval by ISO 13485, **has a signature and date**. Additionally, we note that **FDA will consider signatures that utilize the method the Agency determines fulfills electronic signature requirements to be compliant with this requirement.** Manufacturers can choose to develop electronic records and electronic methods for denoting approval. **Our focus is on whether the substance of the requirements is met and not the physicality of the record or signature methodology.**"

## 3.5 State of play

QSIT was withdrawn 2 Feb 2026; inspections run under Compliance Program **7382.850**, replacing 7382.845 and 7383.001. FDA will neither issue nor accept ISO 13485 certificates in lieu of inspection: *"The FDA will not require certificates of conformance to ISO 13485 and will not issue certificates of conformance to ISO 13485. A certificate of conformance to ISO 13485 will not exempt a manufacturer from an FDA inspection."*
https://www.fda.gov/medical-devices/quality-management-system-regulation-qmsr/quality-management-system-regulation-frequently-asked-questions

No phase-in was granted — FDA rejected it because *"having two inspectional programs in operation at the same time would be inefficient."* Records created before 2 Feb 2026 remain inspectable.

---

# 4. ISO 13485:2016 clauses an ERP touches

**PARAPHRASE WARNING.** ISO 13485 clause text is copyrighted. Everything in this section is accurate paraphrase **except** passages explicitly marked as FDA quotations. Buy the standard (https://www.iso.org/standard/59752.html) before printing clause language. It is IBR'd at §820.7(b), so the 2016 third edition is the legally operative version in the US.

| Clause | Requirement (paraphrase unless marked) | ERP consequence |
|---|---|---|
| **4.1.6** | Documented procedures for validating computer software used in the QMS; validate before initial use and, as appropriate, after changes; approach and activities **proportionate to risk**; records maintained. | The validation hook for the ERP itself. Risk-proportionality is in binding text now, not merely guidance. |
| **4.2.3 Medical device file** | *FDA, verbatim:* "the MDF will contain or reference the procedures and specifications **that are current on the manufacturing floor**." Contains/references device description and intended use, labelling and IFU, product specs, manufacturing/packaging/storage/handling/distribution specs and procedures, measuring and monitoring procedures, and as appropriate installation and servicing. | The old DMR. The ERP holds the operational half: item master, BOM, routing, packaging spec, label spec — each with revision and **effectivity**. |
| **4.2.4 Control of documents** | *FDA, Comment 35, verbatim:* "Clause 4.2.4 of ISO 13485 specifies that documents required by the quality management system shall be controlled. **Records are a special type of document and shall be controlled according to the requirements given in 4.2.5.**" Review and approve before issue; identify changes and current revision status; prevent use of obsolete documents; control documents of external origin; retain at least one obsolete copy for the device lifetime. | Master data needs approval workflow, revision identity, obsolescence marking and retention of superseded revisions — not an `updated_at` column. |
| **4.2.5 Control of records** | Legible, identifiable, retrievable; protect confidential health information; retention **at least the lifetime of the device as defined by the organization, and not less than two years from release** (or as regulatory requirements specify). | Retention is device-lifetime-driven, so "expected life" is master data that drives a per-record retention clock. |
| **6.3 Infrastructure** | Document infrastructure requirements — buildings, process equipment (hardware **and software**), supporting services — and maintenance requirements including intervals where maintenance affects product quality. | PM scheduling and an equipment master. The ERP is itself infrastructure under this clause. |
| **7.4 Purchasing** | *FDA, Comment 40, quoting the clause:* suppliers must be evaluated "in terms of ability and performance of the supplier, commensurate with the **'effect of the purchased product on the quality of'** the final finished device and in terms of the **'proportionate risk associated with'** the final finished device. Additionally, **monitoring and reevaluation** of suppliers and the performance of purchased products is required." Purchasing information must describe the product and, where applicable, require the supplier to notify the organization of changes. | Approved supplier list with **effective-dated approval per supplier×part**, snapshotted onto the PO at issue; incoming inspection scaled to supplier risk; supplier change-notification obligations tracked. |
| **7.5.1 Control of production and service provision** | Plan and carry out production under controlled conditions: documented procedures, qualified infrastructure, monitoring of process parameters and product characteristics, availability and use of monitoring equipment, defined labelling and packaging operations, release/delivery/post-delivery activities. **A record of manufacture for each batch (or each device, or batch of devices) providing traceability and identifying the quantity manufactured and the quantity approved for distribution.** | This is the DHR. The single most ERP-native requirement in the standard. |
| **7.5.6 Validation of processes** | Validate processes whose output cannot be verified by subsequent monitoring or measurement; documented procedures covering criteria, equipment qualification, personnel qualification, methods, records, revalidation. Software used in such processes must be validated, proportionate to risk. | Process-validation status per process × product gates production release. |
| **7.5.8 Identification** | Documented procedures for product identification throughout realization, and for identifying **product status** with respect to monitoring and measurement. Plus §820.10(b)(1): a documented system to assign UDI per Part 830. | Every stock position carries an explicit acceptance status; status is part of identity, not a report flag. |
| **7.5.9.1 Traceability — general** | Documented procedures for traceability defining the extent of traceability and the records required. Plus §820.10(b)(2): Part 821 where applicable. | |
| **7.5.9.2 Traceability — implantable devices** | Records of **components, materials and work environment conditions** where these could cause the device not to meet its specified safety and performance requirements; and the organization requires its distributors or agents to maintain **distribution records** to allow traceability, with records available for inspection. Extended by §820.10(d) to life-supporting / life-sustaining devices. | Full backward genealogy to material lot plus work-environment record, and a distribution ledger reaching past your direct customer. |
| **7.6 Control of monitoring and measuring equipment** | Calibrate or verify at specified intervals against traceable standards; identify to allow calibration status to be determined; safeguard from adjustment; protect from damage; **assess and record the validity of previous results when equipment is found not to conform, and take action on the affected product**. Software used in monitoring and measurement must be validated. | Calibration status must gate the acceptance transaction, and an out-of-tolerance finding must query *every acceptance record made with that gauge since the last good calibration*. That reverse query is a hard schema requirement. |
| **8.2.4 Internal audit** | Documented procedure; plan the programme based on the status and importance of processes and previous audit results; auditors do not audit their own work; record audits and results; correct without undue delay; verify follow-up. | Pure eQMS. Touches the ERP only as an audit subject. |
| **8.3 Control of nonconforming product** | Identify and control nonconforming product to prevent unintended use or delivery; documented procedure for evaluation including whether investigation is needed and whether to notify an external party; dispositions are: take action to eliminate the nonconformity, authorize use/release/acceptance under concession, or take action to preclude the original intended use. *FDA, quoting Clause 8.3.2 verbatim:* acceptance by concession is allowed only if "**justification is provided, approval is obtained and applicable regulatory requirements are met.**" 8.3.3 covers nonconforming product detected after delivery, including advisory notices. 8.3.4 rework must follow a rework procedure, be re-verified, and the record must state that rework was performed. | Quarantine / hold / reject / rework / concession are **inventory ledger states**, with a signed disposition and, for concession, an identified approver and a stored justification. |
| **8.5.2 / 8.5.3 CAPA** | Two separate clauses. Corrective: review nonconformities including complaints, determine causes, evaluate the need for action, plan and document, **verify that the action does not adversely affect the ability to meet regulatory requirements**, record results. Preventive: the same for potential nonconformities. Both "without undue delay." *FDA:* action "shall be appropriate to the magnitude of the problem and commensurate with the risks encountered." | eQMS-native. The ERP supplies stable link targets — lot IDs, shipment IDs, supplier×part, UDI — and must not renumber them. |

---

# 5. DHR, DMR, DHF — contents and ownership

The terms are out of the CFR but remain the working vocabulary of every SOP and every legacy procedure. The precise definitions are the **pre-QMSR §820.3** text, quoted verbatim:

> "**(e) Design history file (DHF)** means a compilation of records which describes the design history of a finished device."
> "**(i) Device history record (DHR)** means a compilation of records containing the production history of a finished device."
> "**(j) Device master record (DMR)** means a compilation of records containing the procedures and specifications for a finished device."

## 5.1 Required contents (pre-QMSR text, verbatim)

**DMR — §820.181:** *"The DMR for each type of device shall include, or refer to the location of, the following information: (a) Device specifications including appropriate drawings, composition, formulation, component specifications, and software specifications; (b) Production process specifications including the appropriate equipment specifications, production methods, production procedures, and production environment specifications; (c) Quality assurance procedures and specifications including acceptance criteria and the quality assurance equipment to be used; (d) Packaging and labeling specifications, including methods and processes used; and (e) Installation, maintenance, and servicing procedures and methods."*
→ **now ISO 4.2.3 Medical Device File.**

**DHR — §820.184:** maintained *"for each batch, lot, or unit... to demonstrate that the device is manufactured in accordance with the DMR"*, containing *"(a) The dates of manufacture; (b) The quantity manufactured; (c) The quantity released for distribution; (d) The acceptance records which demonstrate the device is manufactured in accordance with the DMR; (e) The primary identification label and labeling used for each production unit; and (f) Any unique device identifier (UDI) or universal product code (UPC), and any other device identification(s) and control number(s) used."*
→ **now ISO 7.5.1's record of manufacture, plus §820.35(c) for UDI.**

**DHF — §820.30(j):** *"Each manufacturer shall establish and maintain a DHF for each type of device. The DHF shall contain or reference the records necessary to demonstrate that the design was developed in accordance with the approved design plan and the requirements of this part."*
→ **now ISO 7.3.10 design and development file**, and §820.10(c) (cross-referenced from §801.45(e) as amended Dec 2025).

**QSR — §820.186:** procedures and documentation of activities not specific to a particular device type.
→ **now ISO 4.2.1 / 4.2.2 (quality manual, QMS documentation).**

## 5.2 What feeds the DHR (still required via ISO 7.5.1 / 8.2.6)

- **§820.80(e)** acceptance records: *"(1) The acceptance activities performed; (2) the dates acceptance activities are performed; (3) the results; (4) **the signature of the individual(s) conducting the acceptance activities**; and (5) where appropriate the equipment used. **These records shall be part of the DHR.**"*
- **§820.80(d)** finished device release: *"Finished devices shall not be released for distribution until: (1) The activities required in the DMR are completed; (2) the associated data and documentation is reviewed; (3) **the release is authorized by the signature of a designated individual(s); and (4) the authorization is dated.**"*
- **§820.120(b)/(d)** labeling: release documented in the DHR with date and signature; *"The label and labeling used for each production unit, lot, or batch shall be documented in the DHR."* → now §820.45.
- **§820.160(b)** distribution records: *"(1) The name and address of the initial consignee; (2) The identification and quantity of devices shipped; (3) The date shipped; and (4) Any control number(s) used."*
- **§820.65** (old): *"identifying with a control number each unit, lot, or batch of finished devices and where appropriate components. The procedures shall facilitate corrective action. Such identification shall be documented in the DHR."* → now ISO 7.5.9 + §820.10(d).
- **§820.86** acceptance status: *"The identification of acceptance status shall be maintained throughout manufacturing, packaging, labeling, installation, and servicing of the product to ensure that only product which has passed the required acceptance activities is distributed, used, or installed."*

## 5.3 Ownership split — opinionated

| Record | ERP | PLM | eQMS |
|---|---|---|---|
| Item master, part numbers, UOM, package hierarchy | **Owner** | mirrors | — |
| DI / GTIN / Basic UDI-DI and their lifecycle | **Owner (Phase 6 UDI module, on core tables)** | supplies some attributes | — |
| Manufacturing BOM + routing + effectivity | **Owner** | source of the engineering BOM | — |
| Drawings, specs, IFU artwork, design outputs, risk file, V&V | consumer (revision pointer only) | **Owner (DHF + spec half of MDF)** | — |
| Label templates and artwork | prints them | **Owner** of the approved artwork | approves |
| Work order, material issue, lot genealogy, in-process data, yields, scrap | **Owner (DHR)** | — | — |
| Acceptance / inspection records, release signature | **Owner** | — | reads |
| Calibration / equipment master | **Owner** (or eQMS — pick one and gate the acceptance transaction on it) | — | alternate owner |
| Nonconformance: inventory state | **Owner** | — | owns the investigation |
| CAPA, complaints, MDR, audits, training, management review, change control | — | — | **Owner** |
| Supplier master + approved supplier list | **Owner of the transactional half** | — | **Owner of the qualification half** |
| Distribution records, consignee, shipment, serial→customer | **Owner** | — | — |
| GUDID / EUDAMED submission payload | **Owner of the data** | owns some attributes | — |

**The seam that matters:** the ERP must never hold the only copy of a controlled specification, and the eQMS must never hold the only copy of a lot genealogy. Every cross-system reference must be to an **immutable, versioned identifier** (`PART-1234 rev C`, `LOT-A9931`), never to a mutable row.

---

# 6. Computer system validation

## 6.1 The obligation, restated for 2026

820.70(i) is gone. The chain is now §820.10(a) → ISO 13485 **4.1.6** (QMS software), **7.5.6** (production/service software), **7.6** (monitoring/measurement software). For an ERP, 4.1.6 governs, with 7.5.6 reaching any MRP function that drives production.

Meanwhile §11.10(a) is unchanged — *"Validation of systems to ensure accuracy, reliability, consistent intended performance, **and the ability to discern invalid or altered records**"* — and its 2003 enforcement discretion does not reach production/QMS software (see §2.9).

Substantively this loosened one thing and tightened another: old 820.70(i) demanded validation *"according to an established protocol"*; ISO 4.1.6 demands documented *procedures* and puts risk-proportionality in binding text.

## 6.2 FDA Computer Software Assurance — current version

**"Computer Software Assurance for Production and Quality Management System Software," issued 3 February 2026, FINAL.** Docket FDA-2022-D-0795.
https://www.fda.gov/regulatory-information/search-fda-guidance-documents/computer-software-assurance-production-and-quality-management-system-software
PDF: **https://www.fda.gov/media/188844/download**

Lineage: draft September 2022 → **final 24 September 2025** (as "…Quality *System* Software") → **Level 2 revision 3 February 2026**, retitled "Quality *Management* System" to align with QMSR. It *"supersedes Section 6: Validation of Automated Process Equipment and Quality System Software"* of the 2002 *General Principles of Software Validation* (https://www.fda.gov/media/73141/download). GPSV Sections 1–5 survive.

### Step 1 — intended use, feature by feature

Directly part of production/QMS includes anything *"maintaining a quality record established under applicable quality management system obligations."* Explicitly out of scope:

> "software with the following intended uses generally is not considered to be used as part of production or the quality management system... Software intended for management of general business processes or operations not specific to production or the quality management system, **such as email or accounting applications**; and Software intended for establishing or supporting infrastructure not specific to production or the quality management system, such as networking, user authentication, or continuity of operations (e.g., backup and restore)."

This is a real lever: **an ERP is not monolithically in scope.** The GL/AP/AR side is accounting. The DHR-bearing, lot-genealogy, nonconformance, MDF and labeling functions are in scope.

### Step 2 — binary process risk

> "FDA considers a software feature, function, or operation to pose a **high process risk** when its failure to perform as intended **may result in a quality problem that foreseeably compromises safety**, meaning a medical device risk."

Explicitly **not** high risk: *"Are used as part the quality management system for **Corrective and Preventive Actions (CAPA) routing, automated logging/tracking of complaints, automated change control management, or automated procedure management**."*

### FDA uses an ERP as its running example three times, and the discriminator is a human step

1. ERP automates material restocking, *"**a qualified person checks the materials before their use in production**"* → *"the delivery of the wrong materials to the qualified person should result in the rejection of those materials before use in production; as such, the quality problem should not foreseeably lead to compromised safety. The manufacturer identifies this as an **intermediate (not high) process risk**."*
2. Same feature, but it *"**also automates checking the materials before their use in production**. A qualified person does not check the material first."* → **high process risk.**
3. ERP automates **product delivery** → *"A failure of this feature to perform as intended may result in a delivery mix-up, which would be a quality problem that foreseeably compromises safety; as such, the manufacturer identifies this as a **high process risk**."*

**Design lesson: a mandatory human confirmation step is what moves an ERP function out of the high-risk bucket.** That is a product-architecture choice, not a documentation choice.

### The assurance-activity continuum

Definitions are sourced to IEC/IEEE/ISO 29119-1:2022:

- **Unscripted testing** — *"Dynamic testing in which the tester's actions are not prescribed by written instructions in a test case."* Comprising:
  - **Scenario testing (a.k.a. ad-hoc testing)** — *"a specification-based test case design technique based on exercising sequences of interactions between the test item and other systems."*
  - **Experience-based testing**, including **error guessing** (*"test cases are derived on the basis of the tester's knowledge of past failures or general knowledge of failure modes"*) and **exploratory testing** (*"the tester spontaneously designs and executes tests based on the tester's existing relevant knowledge, prior exploration of the test item... and heuristic 'rules of thumb'"*).
- **Scripted testing** — *"test cases are recorded (e.g., document in a test management tool or in a spreadsheet) and can then be executed manually or executed automatically... depending on the intended use, a more **robust scripted testing** where the test cases and evidence may include detailed requirements for repeatability, traceability, or auditability may be appropriate."*

The authorizing passage:

> "For high process risk software features, functions, and operations, manufacturers may choose to consider more rigor such as the use of scripted testing or a hybrid approach of scripted testing and unscripted testing, scaled as appropriate... **In contrast, for software features, functions, and operations that are not high process risk, manufacturers may consider using unscripted testing methods such as scenario testing, error-guessing, exploratory testing, or a combination of methods that is suitable for the risk.** The testing examples discussed for high process risk and not high process risk are not exclusive to those categories... For example, **unscripted testing may be better suited to assure the software performs as intended even for high process risk features**, functions, and operations."

That last sentence is stronger than most commentary acknowledges — FDA is not saying "unscripted only for low risk."

And the governing principle: *"Because the computer software assurance effort is risk-based, it follows a **least-burdensome approach, where the burden of validation is no more than necessary to address the risk.**"*

### What the record must contain

> "When establishing the record, the manufacturer should capture sufficient objective evidence to demonstrate that the software feature, function, or operation was assessed and performs as intended. In general, FDA recommends the record include the following:
> - The intended use of the software feature, function, or operation;
> - The result of the risk-based analysis of the software feature, function, or operation; and
> - Documentation of the assurance activities conducted, including:
>   - A description of the testing conducted based on the assurance activity.
>   - Issues found during testing (e.g., deviations, defects, and/or failures).
>   - A conclusion statement declaring acceptability of the software for its intended use...
>   - Record of who performed testing/assessment and date the testing/assessment was performed.
>   - Established review and approval when appropriate (e.g., when necessary, a signature and date of an individual with signatory authority)."

Ceiling: *"**Documentation of assurance activities need not include more evidence than necessary** to show that the software feature, function, or operation performs as intended for the risk identified."*

And the screenshot-killer, which is a direct product requirement:

> "As a least-burdensome approach, FDA recommends **incorporating the use of digital records, such as system logs, audit trails, and other data generated and maintained by the software, as opposed to paper documentation, screenshots, or duplicating results already digitally retained by the software** when establishing the record associated with the assurance activities."

Table 1 grades the record by activity: robust scripted testing needs test objectives, step-by-step cases, expected results, independent review/approval of the plan, per-case results and a detailed report; **exploratory testing needs only "high level test plan objectives with pass/fail criteria for each objective (no step-by-step procedure is necessary)"**; error guessing and scenario testing need "Testing of features and functions with **no test plan**."

### Vendor leverage — FDA's own list, effectively the spec for a vendor package

> "Established purchasing control processes for selecting and monitoring software vendors. For example, the medical device manufacturer could incorporate **the software development practices, validation work, and electronic information already performed by developers of the software as the starting point** and determine what additional activities may be needed. **For some lower-risk software features, functions, and operations, this may be all the assurance that is needed by the manufacturer.**"

Acceptable vendor evidence, per CSA §V.A.5: onsite audits *"if applicable"* (FDA concedes *"it may not be feasible or appropriate for a device manufacturer to audit the software vendor"*); *"Review of the vendor's accreditations and certifications (e.g., **Service Organization Controls reports**), and industry standard certifications (e.g., **ISO certifications**)"*; review of SDLC, software QA, cybersecurity documentation including **SBOM**, threat modeling and security testing; and review of *"the vendor's or software's **data integrity capabilities or controls**"* — namely *"Retaining records, archiving data, and generating accurate and complete copies of records; Securing data at rest and in transit (i.e., maintaining secure, computer-generated, time-stamped audit trails of users' actions and changes to data, encrypting data); and/or Establishing and maintaining access controls, electronic signature controls and authorization checks for users' actions."*

## 6.3 GAMP 5 Second Edition (ISPE, July 2022)

**SECONDARY-SOURCED IN PART — see flags.** https://ispe.org/publications/guidance-documents/gamp-5-guide-2nd-edition

FDA cites it by name (CSA footnote 25): *"manufacturers may refer to various software standards and industry guidance, such as, but not limited to **GAMP5 - A Risk-Based Approach to Compliant GxP Computerized Systems (Second Edition)**."*

What changed, per ISPE's own description: **critical thinking** as an explicit discipline; *"**Increased importance of service providers**, which includes encouraging regulated companies to **maximize supplier involvement to leverage knowledge, experience, and documentation where possible**"*; *"the GAMP specification and verification approach **is not inherently linear but also fully supports iterative and incremental methods**"* — the V-model demoted from mandate to one instantiation, which is what makes the Agile appendix coherent; expanded data integrity / ALCOA+; new appendices on cloud, IT infrastructure, AI/ML, blockchain and open-source software.

> **SECONDARY:** appendix designations (M11 IT Infrastructure, M12 Critical Thinking, D1 Specifying Requirements absorbing D2, D5 Testing, D8 Agile, D9 Software Tools, D10 Blockchain, D11 AI/ML) come from a single secondary source. Verify against the book.

> **SECONDARY / DISPUTED:** **Categories are 1, 3, 4, 5 — there is no Category 2 in GAMP 5** (firmware was a GAMP 4 category). Secondary sources disagree loudly on this; the substance is consistent across the credible ones, but this could not be verified against the book because ISPE paywalls it.

**Category 4 vs 5 is the commercial crux.** Category 4 (configured product): *"The supplier remains responsible for the core product, while the regulated organization is responsible for establishing that the selected configuration supports its intended use."* Deliverables: URS, functional and configuration specifications, **configuration workbooks and role/permission matrices**, supplier evidence, risk assessments, test scripts, migration evidence, traceability, an approved configuration baseline and change records. Category 5 (custom) adds source-code records under version control, code review, unit and integration test evidence, build/deployment records and defect records — because *"the organization cannot rely solely on broad commercial use or standard product evidence."* And: *"**Supplier development does not make bespoke code a standard commercial product.**"*

Two caveats worth keeping: GAMP resists mechanical category-to-effort mapping (*"Software category informs validation strategy but does not automatically determine scope or dictate a fixed IQ/OQ/PQ package"*), and depth scales within Category 5 (*"A short script performing a critical calculation may require focused review and testing rather than the complete documentation structure used for a large custom application"*).

## 6.4 What a vendor validation package actually contains

| Artifact | Who can legitimately author it |
|---|---|
| Validation Plan / Master Validation Plan | Vendor template; customer approves and scopes to its intended use |
| URS template (pre-written requirements + matching test cases) | Vendor ships; **customer must own the content** |
| Functional / Design Specification | **Vendor** — genuinely transferable |
| Configuration Specification, role/permission matrix | Vendor template, customer fills |
| Requirements Traceability Matrix | Vendor seeds, customer extends to its own requirements |
| Risk assessment (FMEA-style) | Vendor can pre-assess features; **customer must re-score against its own process** — FDA's three ERP examples prove identical software scores differently |
| **IQ** protocol and execution | **Vendor**, especially for SaaS |
| **OQ** protocol, scripts, execution | **Vendor** |
| **PQ / UAT** | **Customer, always** |
| Test scripts with expected vs actual, signed/dated execution records | Vendor supplies; customer executes |
| Supplier audit package: quality manual, SDLC procedures, ISO 9001/13485 certs, SOC 2, bug tracking, release notes, SBOM, security testing | **Vendor** — this is FDA's §V.A.5 list verbatim |
| Data migration plan and verification | Joint; the data is the customer's |
| Validation Summary Report | Customer signs |
| Part 11 assessment / clause-by-clause matrix | Vendor **for technical controls only** |
| Periodic review + change control SOPs | Vendor templates, customer adopts |

**The customer always owns validation, and the citation for it survives CSA.** Cite **GPSV §4.10**, *not* §6.2 — §6.2 was superseded in Feb 2026, so the famous "device manufacturer retains the ultimate responsibility" sentence everyone quotes is no longer live authority:

> "Software validation activities and tasks may be dispersed, occurring at different locations and being conducted by different organizations. However, **regardless of the distribution of tasks, contractual relations, source of components, or the development environment, the device manufacturer or specification developer retains ultimate responsibility for ensuring that the software is validated.**"

CSA's live equivalent: *"**Manufacturers are responsible for determining the appropriate assurance activities** for ensuring the software features, functions, or operations maintain a validated state."*

**Why a vendor structurally cannot validate for the customer:** validation is against *intended use*, and intended use is a property of the customer's process, not the product. FDA's two material-check ERP examples are the proof — identical software, opposite risk classifications, because one customer has a human inspection step and the other does not.

**A Part 11 matrix claiming full coverage is an overclaim.** §11.10(i) training, §11.10(j) accountability policies, §11.100(b) identity verification, §11.300(b)–(c) password aging and token loss management are procedural — and §11.100(c)(1) requires a certification *"signed with a traditional handwritten signature"* sent to FDA. No software does that. A product can be **Part 11-capable**; only a deployment can be Part 11-compliant. Say it that way in marketing and you will never have to walk it back in an audit.

Clauses a vendor genuinely *can* discharge in the product: 11.10(b), (c), (d), **(e)**, (f), (g), (h), (k)(2); **11.50** in full; **11.70**; **11.100(a)**; **11.200(a)(1)**; **11.300(a), (d), (e)**.

## 6.5 SaaS, continuous deployment, revalidation

ISO 4.1.6 says validate *"as appropriate, after changes."* That is **not** "revalidate every release" — vendor literature saying so is selling unnecessary work, and it is exactly the reflex CSA was written to kill.

FDA's actual answer is **CSA Appendix A, Example 4 (SaaS PLM)** — the closest published analogue to a SaaS ERP:

> "**The SaaS vendor provides the manufacturer documentation summarizing the changes, testing, and testing results of all automatic updates made to the SaaS system functions identified by the manufacturer as part of the service agreement.** The manufacturer performs an assessment of the changes and the effect they may have on the intended use. The manufacturer performs risk-based assurance testing of the changes appropriate to the impact identified. **The manufacturer maintains a record summarizing the risk assessment of the change and any assurance activities performed.**"

Three shippable obligations per release: a **change summary scoped to the functions this customer registered**, the vendor's own test results for those changes, and enough detail for the customer's impact assessment. Note the contractual hook — *"identified by the manufacturer as part of the service agreement"* — meaning **per-customer function registration should be a product feature**, not an account-management favour.

The same example specifies the one-time vendor assessment the customer performs: SDLC evaluation, QMS and certifications review, cybersecurity documentation and lifecycle plans, infrastructure availability and reliability, plus *"a service agreement with the SaaS vendor that includes requirements for **security, data integrity, privacy, availability, change management, and business continuity**."*

It also shows how little testing a low-risk SaaS deployment needs: configuration verification and UAT *"using exploratory unscripted testing"*; e-signature functions get *"scenario testing of this function with users"*; only the access-control and traceability functions get *"an **automated test script that will quickly exercise the access controls to also support verification of future changes**"* — i.e. build the regression suite exactly where it will be re-run every release.

**Feature gating** — letting the customer choose when to activate a new feature — decouples your deployment cadence from their validation cadence and is the single most valuable architectural feature for a regulated SaaS ERP.

**Continuous monitoring is explicitly credited as a substitute for test effort:** monitoring capability *"may reduce the risk associated with a failure of the software to perform as intended and may be considered when deciding on assurance activities."* A vendor that exposes health and anomaly telemetry to the customer is directly reducing that customer's test burden.

**Regression scope** still comes from **GPSV §4.7** (not superseded): *"When any change (even a small change) is made to the software, the validation status of the software needs to be re-established. **Whenever software is changed, a validation analysis should be conducted not just for validation of the individual change, but also to determine the extent and impact of that change on the entire software system.** Based on this analysis, the software developer should then conduct an appropriate level of software regression testing..."*

One under-appreciated SaaS obligation, from MHRA's *GXP Data Integrity Guidance* §6.20 — **UK-scoped, not FDA authority**, but convergent and increasingly quoted by auditors:

> "**Appropriate arrangements must exist for the restoration of the software/system as per its original validated state, including validation and change control information to permit this restoration.**"

You must be able to restore a *specific validated version + configuration + change history*, not merely "the latest version."

---

# 7. Traceability and genealogy

## 7.1 Forward and backward, operationally

**Backward trace (from a finished lot/serial):** every component lot, raw material lot, sub-assembly lot and purchased part consumed; the supplier and the receipt; the work orders, operations and operators; the equipment and **gauge IDs** used at each acceptance step; the process parameters and work-environment conditions; the spec revision in force (BOM rev, routing rev, drawing rev, label artwork rev); the acceptance records and the release signature; the label and labeling actually applied.

**Forward trace (from a material lot, a gauge, a supplier, or an operator):** every finished lot/serial that consumed it, and from there every shipment, consignee, quantity and date — and past the first consignee where the device is tracked. The gauge case is the one people forget: a calibration failure requires you to name every acceptance record taken with that instrument since its last good calibration, and then every device those records released.

## 7.2 What a recall query must answer, and how fast

This is enumerated, not vague.

**21 CFR 806.10 — correction/removal report, due within 10 working days**
https://www.ecfr.gov/current/title-21/chapter-I/subchapter-H/part-806

> "(b) The manufacturer or importer shall submit any report required by paragraph (a) of this section **within 10-working days of initiating such correction or removal.**"
> "(c)(5) **The unique device identifier (UDI)** that appears on the device label or on the device package, or the device identifier, universal product code (UPC), model, catalog, or code number of the device **and the manufacturing lot or serial number** of the device or other identification number.
> (c)(9) **The total number of devices manufactured or distributed subject to the correction or removal and the number in the same batch, lot, or equivalent unit of production** subject to the correction or removal.
> (c)(10) **The date of manufacture or distribution and the device's expiration date or expected life.**
> (c)(11) **The names, addresses, and telephone numbers of all domestic and foreign consignees of the device and the dates and number of devices distributed to each such consignee.**"

Scope expansion: *"(d) If, after submitting a report under this part, a manufacturer or importer determines that the same correction or removal should be extended to additional lots or batches of the same device, the manufacturer or importer shall **within 10-working days of initiating the extension** of the correction or removal, amend the report..."*

**21 CFR 7.46(a) — firm-initiated recall**, the information FDA will ask for:

> "(4) Total amount of such products produced and/or the timespan of the production.
> (5) Total amount of such products estimated to be in distribution channels.
> (6) **Distribution information, including the number of direct accounts and, where necessary, the identity of the direct accounts.**"

**§7.53 recall status reports**, generally every 2–4 weeks:

> "(1) Number of consignees notified of the recall, and date and method of notification.
> (2) Number of consignees responding to the recall communication **and quantity of products on hand at the time it was received**.
> (3) Number of consignees that did not respond...
> (4) Number of products returned or corrected **by each consignee contacted** and the quantity of products accounted for.
> (5) Number and results of effectiveness checks that were made.
> (6) Estimated time frames for completion of the recall."

That is a **recall response ledger**, per consignee, per lot — a schema, not a spreadsheet.

**§7.59** is FDA telling you to build it in advance: *"(b) Use sufficient coding of regulated products to make possible **positive lot identification** and to facilitate effective recall of all violative lots. (c) Maintain such product distribution records as are necessary to facilitate location of products that are being recalled."*

**21 CFR 821 — device tracking, the hardest SLAs in the chapter**
https://www.ecfr.gov/current/title-21/chapter-I/subchapter-H/part-821

> "§821.25(a)(1) ... **within 3 working days of a request from FDA**, prior to the distribution of a tracked device to a patient, the name, address, and telephone number of the distributor, multiple distributor, or final distributor holding the device for distribution and **the location of the device**;
> (a)(2) **Within 10 working days of a request from FDA** for tracked devices that are intended for use by a single patient over the life of the device, after distribution to or implantation in a patient: (i) **The unique device identifier (UDI), lot number, batch number, model number, or serial number** of the device or other identifier necessary to provide for effective tracking of the devices; (ii) the date the device was shipped by the manufacturer; (iii) the name, address, telephone number, and social security number (if available) **of the patient receiving the device**...; (iv) the date the device was provided to the patient; (v) the name, mailing address, and telephone number of **the prescribing physician**; (vi) ... the physician regularly following the patient if different...; (vii) if applicable, **the date the device was explanted**... the date of the patient's death; or the date the device was returned to the manufacturer, permanently retired from use, or otherwise permanently disposed of."

Three architecture facts fall out:

1. **Patient PII is in scope** — names, addresses, SSNs where available. Collides head-on with GDPR erasure rights if you also sell into the EU.
2. **Retention is open-ended.** §821.60: *"Persons required to maintain records under this part shall maintain such records **for the useful life of each tracked device** they manufacture or distribute. The useful life of a device is the time a device is in use or in distribution for use."*
3. **Data residency is a regulation, not a preference.** §821.50(b): *"Records and information referenced in paragraph (a) of this section shall be available to FDA personnel for purposes of reviewing, copying, or any other use related to the enforcement of the act and this part. **Records required to be kept by this part shall be kept in a centralized point for each manufacturer or distributor within the United States.**"*

**Adjacent SLAs the ERP must feed:** MDR within **30 calendar days**, or **5 work days** for §803.53 events (§803.50); §803.52(c) Block D requires *"Model number, catalog number, serial number, lot number, or other identifying number; **expiration date; and unique device identifier (UDI)** that appears on the device label or on the device package"*; multiple distributors must answer the manufacturer within **5 working days** (§821.30(c)(2)); GUDID non-label attribute updates within **10 business days** (§830.330(b)).

## 7.3 Data model implications

- **The unit of record is a *tracked entity*, not a row in an item table.** It resolves to a lot, a serial, or a unit-within-lot, and it is the same type everywhere downstream — WIP, finished goods, pick, pack, ship, return, service, complaint, recall. **This is kernel (see §1.0.3).**
- **Genealogy is a directed acyclic graph of consumption edges**, not a parent column. Splits, merges, rework loops, re-labeling, repacking, kitting and de-kitting all create edges. Every posting that consumes or produces material writes edges; no posting path may bypass this.
- **The ledger is append-only.** "Quantity on hand" is a fold over postings, not a column. This is what makes as-of reconstruction and bidirectional trace tractable, and it is the hardest thing to retrofit.
- **Distribution is part of the ledger**, not a downstream report: consignee identity, address, quantity, date, lot/serial, and for tracked devices the chain past the first consignee.
- **Every trace query has a deadline** — 3 working days, 10 working days. Design indexes and denormalizations for those queries from the start; treat "trace a lot both directions in under a minute" as a functional requirement with a test.
- **Trace crosses the spec dimension.** "What was lot X built to" requires effectivity-dated BOM, routing, spec and label revisions — the MDF↔DHR link. Without effectivity your genealogy is half a genealogy.

---

# 8. Companion document

UDI, AIDC, GS1 / HIBCC / ICCBBA issuing-agency mechanics, symbol quality tables, GUDID element list, EUDAMED status and the open HIBCC Basic UDI-DI format question are in:

`C:/Users/fireb/Desktop/Shared/ERP/_team/reports/spike-regulatory-udi.md`

That document carries its own provenance caveats and open-gaps list.
