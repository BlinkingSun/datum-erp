# Sweep 8 — REGULATED FIT vs 21 CFR Part 11

**Slice:** plan-audit #8
**Role:** adversarial researcher
**Date:** 2026-09-11
**WORKDIR:** `C:/Users/fireb/Desktop/Shared/ERP` (read-only except this report)
**TIER:** deep

## 0. Verdict

**The kernel list covers the non-retrofittable *core* of Part 11 electronic records and signatures (11.10(e) generation, 11.70 binding, 11.200 re-auth, identity, state machines, numbering, documents). It does not cover 21 CFR Part 11.**

Roughly **half the clauses are customer SOPs by nature** (validation execution, training, accountability policies, FDA non-repudiation letter, identity proofing, token loss procedures). That is fine. The project’s central claim in `docs/01` is “compliance as architecture rather than as a feature.” Against the actual regulation text, that claim is true for audit *generation* and signature *binding*, and **false as a completeness claim**.

Two omissions are kernel-shaped and already named in architecture, then **dropped from the PLAN crate graph**: reporting/print, and backup/restore. One schema decision (**hash-chained audit vs grant-only**) is not required by the letter of 11.10(e) but **must be made by opus before Wave 2**, because `datum-audit`’s table is the contract. One classic hole (**gap-free numbering vs PostgreSQL `SEQUENCE`**) will produce unanswerable gaps if Wave 2 uses identity columns.

`docs/06-regulatory.md` **does not exist**. PLAN §2 is correct: a spike is in flight and `doc-regulatory` is gated on it. This report does not treat `docs/06` as written.

---

## 1. Sources (regulation text, not blogs)

| Instrument | What was read | Status |
|---|---|---|
| 21 CFR Part 11 | eCFR Title 21 Part 11, current as of **2026-09-10** (last amended 2026-09-10). Full text of 11.1–11.3, 11.10(a)–(k), 11.30, 11.50, 11.70, 11.100, 11.200, 11.300. https://www.ecfr.gov/current/title-21/chapter-I/subchapter-A/part-11 | Primary |
| FDA 2003 | *Part 11, Electronic Records; Electronic Signatures — Scope and Application* (enforcement discretion is **not** a waiver for a green-field 2026 system) | Interpretive |
| EU GMP Annex 11 | Official 2011 revision 1 PDF, EudraLex Vol. 4, in operation 30 June 2011. https://health.ec.europa.eu/system/files/2016-11/annex11_01-2011_en_0.pdf | Primary. 2025 draft exists; **2011 is in force**. |
| ISO 13485:2016 | Clauses 4.1.6, 4.2.4, 4.2.5, 7.5.6, 7.6. ISO paywalled; wording cross-checked against multiple independent clause excerpts that agree. | Secondary for exact punctuation; clause numbers and obligations are not in dispute |
| 21 CFR 820 QMSR | eCFR current. Effective **2026-02-02**. Incorporates ISO 13485:2016 by reference at 820.7/820.10. Supplemental 820.35 (records) and 820.45 (labeling). Old 820.70(i) / 820.30 / 820.180 are **gone**. | Primary |
| FDA CSA | *Computer Software Assurance for Production and Quality System Software*, Sept 2025 (nonbinding). Replaces “validate everything like a device” with risk-based assurance for production/QMS software. | Interpretive |
| Project | `docs/adr/0005`, `docs/02` kernel list + §8–§9, `docs/01`, `docs/03` §8, `docs/04` Phase 0/6, `PLAN.md`, `docs/adr/0003`, `docs/adr/0004`. Confirmed `docs/06-regulatory.md` is absent. | Primary |

Quotes below from Part 11 and Annex 11 are from those official texts.

---

## 2. What ADR 0005 actually decided (and what it did not)

ADR 0005 puts in the kernel: persistence-layer audit, grant-level append-only, server time, attributable actor, e-sign bound to a hash of the signed record version, versioned records. It cites 21 CFR Part 11 in prose, not clause numbers, and only the 11.10(e) + signature-binding slice.

It does **not** mention: 11.10(a)(b)(c)(f)(h)(i)(j)(k), 11.30, 11.50 manifestation on printouts, 11.100, 11.200(a)(1) two-component / continuous-session rules, 11.300, Annex 11, ISO 13485, QMSR, CSA, backup, archival/export, trusted time, hash-chaining, device checks, or a customer-runnable IQ suite.

`docs/02` is broader than the ADR (print, backups, IQ test suite, 11.200 re-auth, floor session lock). **PLAN is narrower than `docs/02`.** The crate graph in PLAN §5 has no print, no backup, no archive, no reporting. That is the load-bearing inconsistency.

---

## 3. Closed vs open (11.10 vs 11.30) — classify, do not waffle

**11.3(b)(4):** *Closed system* = “an environment in which system access is controlled by persons who are responsible for the content of electronic records that are on the system.”

**11.3(b)(9):** *Open system* = access is **not** so controlled.

**11.30** (open): all of 11.10 “as appropriate” **plus** “additional measures such as document encryption and use of appropriate digital signature standards.”

Default Datum topology (`docs/02` §6): one binary + bundled PostgreSQL on a shop machine, clients on the LAN in a browser or Tauri, **no internet dependency**. The shop that owns the records also owns the accounts, the host, and the LAN.

**That is a closed system.** A browser client does not flip the classification. LAN HTTP is not “open” under 11.3.

It **becomes open** (or mixed) if any of these land without a boundary decision:

- Optional OIDC to a cloud IdP (`docs/02` §8).
- Phase 7 `portal` (customer-facing, internet).
- The shop exposes the server beyond the LAN / VPN they do not control.
- A future SaaS offering (explicitly rejected by ADR 0008’s intended content; file not yet written per PLAN §2).

**Coverage:** customer procedure (classification + network SOP) + kernel TLS-if-open. **Not MISSING from the kernel.** **MISSING from docs** — no document currently asserts “closed, with these boundary conditions.” That is a `docs/06` job.

Do **not** implement 11.30 encryption/digital-signature-of-records for the beachhead closed claim. Do **not** pretend the browser makes it open. Do record the OIDC and portal exceptions as open-system surfaces when they exist.

---

## 4. Clause-by-clause coverage

Legend: **K** kernel crate (Wave 2 unless noted) · **M** later module · **P** customer procedure · **X** MISSING (not in kernel, not in a named module, not honestly a SOP)

A row may be mixed. “SOP is fine” means the regulation places the duty on the *person* (the manufacturer), not on the software vendor — architecture cannot eat it.

### 4.1 § 11.10 Controls for closed systems

Chapeau: procedures and controls to ensure authenticity, integrity, confidentiality when appropriate, and that “the signer cannot readily repudiate the signed record as not genuine.”

| Clause | eCFR requirement (abridged, official) | Map | Vehicle | Notes |
|---|---|---|---|---|
| **11.10(a)** | “Validation of systems to ensure accuracy, reliability, consistent intended performance, and the ability to discern invalid or altered records.” | **P + M + PLAN gap** | Customer CSV/CSA; catalog Phase 6 `validation-pack`; `docs/02` §9 IQ suite | Kernel must be *designed so tests are evidence*. PLAN §7 does not mention a customer-runnable IQ suite. Not a storage rewrite. See §6.9. |
| **11.10(b)** | “Ability to generate accurate and complete copies of records in both human readable and electronic form suitable for inspection, review, and copying by the agency.” | **X / K claimed, crate absent** | Architecture kernel “reporting + print”; **no PLAN crate** | This is the complete-copy duty. If the audit/record schema cannot emit a durable human-readable + electronic copy, you cannot add it later without rewriting storage. Print is named kernel in `docs/02` and catalog Phase 0, **absent from PLAN §5**. See §6.1 and §6.4. |
| **11.10(c)** | “Protection of records to enable their accurate and ready retrieval throughout the records retention period.” | **X / P mixed** | Backup (claimed `docs/02` §8, **no crate**); archival (ADR 0005 “revisit if” volume, not designed); customer retention schedule | Predicate retention is ISO 13485 4.2.5 (lifetime of device, not less than 2 years from release) / EU MDR 10–15 years. Software must retrieve for that long, including after version upgrades (Annex 11 §17). See §6.1, §6.10. |
| **11.10(d)** | “Limiting system access to authorized individuals.” | **K + P** | `datum-identity` (Argon2id, RBAC, optional OIDC, floor session lock) | Kernel. Shared shop tablets are called out in `docs/02` §8. SOP for physical access / OS accounts. |
| **11.10(e)** | “Secure, computer-generated, time-stamped audit trails to independently record the date and time of operator entries and actions that create, modify, or delete electronic records. Record changes shall not obscure previously recorded information. … retained … at least as long as … the subject electronic records and … available for agency review and copying.” | **K, with honesty gap** | `datum-audit` + persistence layer (ADR 0005); grant-level append-only (PLAN invariant 3) | Generation and prior-value are the one thing ADR 0005 actually designs. “Shall not obscure” is **not** the same as “the cluster owner cannot alter.” Grant-only vs hash-chain: §6.3. Trail must itself satisfy 11.10(b)/(c). |
| **11.10(f)** | “Use of operational system checks to enforce permitted sequencing of steps and events, as appropriate.” | **K + M** | `datum-statemachine` + `datum-numbering`; module-declared transitions | Sequencing is the state-machine claim. Gap-free numbering after rollback is the classic hole. See §6.7. |
| **11.10(g)** | “Use of authority checks to ensure that only authorized individuals can use the system, electronically sign a record, access the operation or computer system input or output device, alter a record, or perform the operation at hand.” | **K** | `datum-identity` RBAC + `datum-esign` + `datum-statemachine` permissioned transitions | On target. Must apply to sign, not only to login. Input/output device access is the floor-terminal lock + 11.10(h). |
| **11.10(h)** | “Use of device (e.g., terminal) checks to determine, as appropriate, the validity of the source of data input or operational instruction.” | **M + kernel hook** | `shop-floor` Phase 3, `barcode` Phase 7, `calibration` Phase 4 | Not a later-module-only afterthought: audit rows need an optional `source_device_id` **now**. See §6.8. |
| **11.10(i)** | “Determination that persons who develop, maintain, or use electronic record/electronic signature systems have the education, training, and experience to perform their assigned tasks.” | **P + M** | Customer SOP; `training` Phase 4 for *users*; vendor training records for *developers* are the project’s QMS, not the product | Fine as SOP. Kernel cannot determine competence. |
| **11.10(j)** | “The establishment of, and adherence to, written policies that hold individuals accountable and responsible for actions initiated under their electronic signatures, in order to deter record and signature falsification.” | **P** | Customer SOP. Product can display a meaning + legal-equivalent attestation at sign time (`datum-esign`) | Fine as SOP. Do not pretend a checkbox in the kernel satisfies “written policies.” |
| **11.10(k)(1)** | Controls over “distribution of, access to, and use of documentation for system operation and maintenance.” | **P + K/M** | Customer SOP; `datum-documents` + `doc-control` Phase 4 | Vendor-supplied manuals/IQ protocols need revision control too (`docs/03` §8 validation docs). |
| **11.10(k)(2)** | “Revision and change control procedures to maintain an audit trail that documents time-sequenced development and modification of systems documentation.” | **K + P** | `datum-documents`; customer change control; `docs/03` §8 module-set change is a change-control event | Kernel documents primitive is the right place. Systems *software* change history is also git + release manifest — see hashed config manifest §6.11. |

### 4.2 § 11.30 Open systems

| Clause | eCFR requirement | Map | Vehicle | Notes |
|---|---|---|---|---|
| **11.30** | 11.10 as appropriate, plus encryption and “appropriate digital signature standards” as necessary for authenticity, integrity, confidentiality from creation to receipt. | **P (classification) + deferred K** | Default topology = closed. TLS + record-level digital signatures only if an open surface exists | Do not build 11.30 into Wave 2. Do classify in `docs/06`. Flag OIDC-to-cloud and `portal` as the first open surfaces. |

### 4.3 § 11.50 Signature manifestations

| Clause | eCFR requirement | Map | Vehicle | Notes |
|---|---|---|---|---|
| **11.50(a)** | Signed electronic records shall contain associated information that clearly indicates (1) printed name of the signer; (2) date and time when the signature was executed; (3) meaning (review, approval, responsibility, authorship). | **K** | `datum-esign` (ADR 0005: signer, server time, meaning, hash of record version) | Wave 2 can store this. Name must be the **printed name**, not only a user id. |
| **11.50(b)** | The items in (a) “shall be subject to the same controls as for electronic records and shall be **included as part of any human readable form** of the electronic record (**such as electronic display or printout**).” | **X for print; K for display-if-UI** | Display = Wave 3 UI. Print = kernel per `docs/02`, **crate missing** | This is the clause that makes print kernel, not a report writer a module can skip. Annex 11 §8.2 adds “printouts indicating if any of the data has been changed since the original entry” for batch-release records. See §6.4. |

### 4.4 § 11.70 Signature/record linking

| Clause | eCFR requirement | Map | Vehicle | Notes |
|---|---|---|---|---|
| **11.70** | Electronic signatures and handwritten signatures executed to electronic records “shall be linked to their respective electronic records to ensure that the signatures cannot be excised, copied, or otherwise transferred to falsify an electronic record by ordinary means.” | **K + P (wet ink)** | `datum-esign` hash-of-version (ADR 0005). Hybrid wet-ink-on-print then scan = customer procedure / later documents | This is the second thing ADR 0005 actually designs, and it is the right primitive. “Ordinary means” is a copy-paste / reattach bar, not a nation-state bar. Grant-only on the signature table still leaves the superuser. Hash of content makes *transfer to another record* detectable even without a chain. |

### 4.5 § 11.100 General requirements

| Clause | eCFR requirement | Map | Vehicle | Notes |
|---|---|---|---|---|
| **11.100(a)** | Each electronic signature “shall be unique to one individual and shall not be reused by, or reassigned to, anyone else.” | **K** | `datum-identity`: never recycle user/signature ids; deactivate, do not delete; unique login | Must be a Wave 2 invariant, not a style guide. Shared floor logins are an audit finding (`docs/02` already knows this). |
| **11.100(b)** | Before an organization “establishes, assigns, certifies, or otherwise sanctions” an individual’s electronic signature, it “shall verify the identity of the individual.” | **P** | Customer identity-proofing SOP. Product can require an admin attestation flag | Software cannot verify a passport. Optional workflow is fine; not kernel-blocking. |
| **11.100(c)** | Persons using electronic signatures shall, prior to or at the time of such use, **certify to the agency** that the signatures are the legally binding equivalent of handwritten signatures. Certification “signed with a traditional handwritten signature” and submitted; FDA Letters of Non-Repudiation Agreement page. Upon request, additional certification/testimony. | **P** | Customer sends the letter. Product can nag, cannot file | Amended 88 FR 13018 (2023) only changed *where* to send it. Not a crate. |

### 4.6 § 11.200 Electronic signature components and controls

| Clause | eCFR requirement | Map | Vehicle | Notes |
|---|---|---|---|---|
| **11.200(a)(1)** | Non-biometric signatures shall “employ at least two distinct identification components such as an identification code and password.” | **K, underspecified** | `datum-identity` + `datum-esign` | `docs/02` §8 says re-auth on signing. It does **not** say two components. A session cookie is one component. Signing must prompt for password (or second factor), not “click to approve.” |
| **11.200(a)(1)(i)** | Continuous period of controlled system access: first signing uses **all** components; subsequent signings use **at least one** component “only executable by, and designed to be used only by, the individual.” | **K, unmentioned** | `datum-esign` session-of-signings | Not in ADR 0005, not in PLAN. Implementable in Wave 2. Default conservative: **always all components** (matches `docs/02` re-auth). The (i) relaxation is optional later. |
| **11.200(a)(1)(ii)** | Signings **not** in a continuous controlled period: **all** components every time. | **K, claimed** | Re-auth after inactivity timeout (`docs/02` §8) | This is the `docs/02` sentence. Wave 2 must implement timeout + full re-auth. |
| **11.200(a)(2)** | “Be used only by their genuine owners.” | **K + P** | No shared accounts; no recoverable password store | Collides with shop-floor shared tablets: lock the *session*, never share the *identity*. |
| **11.200(a)(3)** | Administered so attempted use by anyone other than the genuine owner “requires collaboration of two or more individuals.” | **K** | Password not known to admin (hashed Argon2id already); admin reset must be a two-person or audited-reset flow | Admin who can set a user’s password to a known value **breaks (a)(3)** by themselves. Reset design is Wave 2 identity scope, currently unstated. |
| **11.200(b)** | Biometric signatures “shall be designed to ensure that they cannot be used by anyone other than their genuine owners.” | **out of scope / P** | Do not ship biometrics in Wave 2 | Fine to omit. Do not leave a stub that stores a “biometric_ok” flag. |

### 4.7 § 11.300 Controls for identification codes/passwords

This is the most handwaved cluster. `docs/02` §8 lists Argon2id, optional OIDC, RBAC, re-auth, floor session lock. It does **not** list 11.300(a)–(e). PLAN has no Wave 2 acceptance criteria for `datum-identity` beyond the crate graph.

| Clause | eCFR requirement | Map | Vehicle | Notes |
|---|---|---|---|---|
| **11.300(a)** | Uniqueness of each combined identification code and password: “no two individuals have the same combination.” | **K** | Unique username; password uniqueness across users is not generally enforced and is **not** what FDA means — they mean unique *ID*, not unique password string | Unique, never-recycled identification **code**. Wave 2 invariant. |
| **11.300(b)** | Issuances “periodically checked, recalled, or revised (e.g., to cover such events as password aging).” | **K, unmentioned** | Password max-age, forced rotate, disable stale accounts | **Not in docs or PLAN.** Cheap in `datum-identity` schema (`password_changed_at`, `must_rotate`). If omitted from Wave 2, it is a migration, not a module rewrite. Put it in Wave 2 scope anyway. |
| **11.300(c)** | Loss-management: deauthorize lost/stolen/missing tokens/cards and issue replacements under rigorous controls. | **P + K hook** | SOP for badges; product must be able to revoke a credential immediately | No token/card story in Wave 1–2. Revoke-session + disable-user is the kernel hook. OIDC shops: IdP is in the SOP. |
| **11.300(d)** | Transaction safeguards to **prevent** unauthorized use and to **detect and report in an immediate and urgent manner** any attempts at unauthorized use to the system security unit and, as appropriate, organizational management. | **K, unmentioned** | Lockout after N failures; alert channel (email/local banner is weak on an air-gapped box — needs an in-app security-event log + optional SMTP) | **Handwaved.** Need a `security_events` append-only stream (can live in `datum-audit` or identity). Air-gap means “report” cannot assume internet. In-app queue for the quality manager is the closed-system answer. |
| **11.300(e)** | Initial and periodic testing of devices (tokens/cards) that bear or generate ID/password information. | **P** | No such devices in the beachhead design | SOP if they add YubiKeys later. Not a kernel miss today. |

**Wave 2 identity scope is handwaved.** 11.200 re-auth is named; 11.300 aging, lockout, uniqueness-of-id, unauthorized-use reporting, and admin-reset collusion are not. None of these force a rewrite of every module. All of them belong in `datum-identity`’s Wave 2 acceptance criteria, or they become a schema migration plus a security finding.

---

## 5. Candidate gaps — confirm or kill

### 5.1 11.10(c)/(b) record retention, archival, migration, complete copies — CONFIRM as design-now; not “rewrite every module” if the schema is complete

**Regulation.** 11.10(b) complete copies, human-readable **and** electronic. 11.10(c) protection and ready retrieval for the retention period. 11.10(e) last sentence: the *audit trail* is retained as long as the subject records and is itself copyable. Annex 11 §7.1 access throughout retention; §8.1 clear printed copies; §17 archiving remains accessible/readable/integrity-checked after equipment or program changes. ISO 13485:2016 4.2.5: retain “at least the lifetime of the medical device as defined by the organization, or as specified by applicable regulatory requirements, but not less than two years from the medical device release.”

**If the audit schema cannot export a complete durable copy, you cannot add this later without rewriting storage.** That sentence is true. Whether Datum’s *intended* schema can is a different question.

ADR 0005 already requires: actor, change, prior value, server time, no obscuring, same transaction as the write. If Wave 2 actually stores those, plus record identity, action/reason, and a stable type name, an exporter can be written later **without touching modules**.

What **cannot** be retrofitted cheaply:

1. **Fields you never stored** (reason-for-change is Annex 11 §9 explicit; ADR 0005 says “where required, why” in `docs/02` but ADR body is weaker). If reason is optional in the schema and modules do not pass it, you will not get it back.
2. **A stable electronic copy format.** If “complete copy” is “pg_dump of the live cluster,” that is not a human-readable copy and it is not readable after a breaking schema change (Annex 11 §4.8, §17). You need a versioned export (JSON/CSV/PDF-A) whose meaning survives Datum 2.0.
3. **Partition/archive keys.** ADR 0005 “Revisit if” volume says partitioning and archival of audit data. If `datum-audit` has no `recorded_at` bracketing / table inheritance / declarative partition key, later archival is a rewrite of the audit table — one table, not every module, but it is the hottest table in the system.
4. **Human-readable renderer.** That is print. See 5.4.

**Kill:** “you will have to rewrite every module.” **Confirm:** export format + partition-friendly audit schema + reason field are Wave 2 `datum-audit` / `datum-db` decisions. **Confirm PLAN gap:** no archival crate, no export API, no print crate.

### 5.2 Trusted time / clock source on an air-gapped shop PC — KILL as Part 11 letter; CONFIRM as honesty gap

**Regulation.** 11.10(e) says “time-stamped,” not “traceable to UTC via authenticated NTP.” 11.50(a)(2) is “the date and time when the signature was executed.” Annex 11 §14(c) “include the time and date that they were applied.” Annex 11 §12.4 requires recording identity of operators including date and time; it does **not** in the 2011 text require cryptographic time.

ADR 0005: “Time comes from the server, always.” `docs/02`: no internet dependency. Bundled PostgreSQL (`adr/0003`) uses the OS clock. The shop admin is the OS admin and the cluster superuser.

**Server time is not authenticated time.** A clock set back, a VM snapshot restored, or `date` run as Administrator will timestamp audit rows with whatever the box believes. Hash-chaining does not fix this (a backdated row hashes fine).

Part 11 accepted practice for closed systems: restrict who can set the clock, SOP for time, audit clock changes. That is **P**, plus a kernel nice-to-have (detect jumps, log `clock_set` as a security event, store `timestamptz`).

**Kill as non-retrofittable kernel miss.** You can add jump detection later. **Confirm:** `docs/06` must say the time source is the host OS, air-gapped, not NTP, and the customer SOP owns clock admin. Do not market “server-authoritative time” as a substitute for trusted time. Store timezone (`timestamptz`); Annex 11 2025 draft adds timezone explicitly.

### 5.3 Hash-chaining vs grant-level append-only — KILL as Part 11 letter; CONFIRM as threat-model lie if overclaimed; OPUS DECISION before Wave 2

**11.10(e)** “Record changes shall not obscure previously recorded information” is about **not overwriting prior values**. It is satisfied by insert-only rows that carry old and new values. It is **not** a cryptographic-binding requirement. 11.3’s *digital signature* is a defined term and is invoked in **11.30 (open systems)**, not in 11.10(e).

ADR 0005 is honest at the application boundary: “the application role has insert and select. It does not have update or delete… an application bug cannot violate it.” PLAN invariant 3 repeats this.

`docs/02` §8 threat model: “mostly insider and accident rather than nation-state.” The insider who matters on a bundled cluster is the **shop admin, who is PostgreSQL superuser.** Superuser bypasses grants. They can `UPDATE`, `DELETE`, `COPY`, drop triggers, reassign owners, or restore a tampered dump. Grant-level append-only is **theater against that actor**.

Annex 11 §9: audit trail is system-generated; reason documented; available in intelligible form; regularly reviewed. It does not require a hash chain. It does imply the administrator of the *application* cannot quietly edit GMP data. The 2011 text does not say “including the DBA.”

**Opus must decide, before Wave 2 fan-out of `datum-audit`:**

| Option | What it buys | Cost |
|---|---|---|
| **A. Grant-only (current ADR)** | Application bugs cannot UPDATE/DELETE. Honest if the claim is scoped to the app role. | Superuser can obscure. Inspection answer is SOP + OS/DB admin segregation (unrealistic on a 30-person shop with one IT person). |
| **B. Hash chain of audit rows** (row_hash = H(prev_hash \|\| canonical(row))) | Tampering by superuser becomes *detectable* on verify. Can sign the head with a shop key held offline. | Schema now (`prev_hash`, `row_hash`). Concurrent inserts need a chain design (per-record chain is easier than a global chain). Verify job. Not a module rewrite. |
| **C. WORM / separate audit store / signed export** | Off-box copies the admin of the live cluster cannot rewrite. | Operationally heavier; fits backup crate. Complements A or B. |

**Retrofit:** B can be added later with a backfill, **if** you assume the historical rows were not already altered. Schema columns now are cheap insurance. **Not a rewrite of every module.**

**Decision required before Wave 2** because `datum-audit`’s public row type is the integration contract (PLAN §5). Adding columns later is a migration across every Wave 2 worktree that already compiled against the stub.

Recommendation to opus: **A is legally sufficient for 11.10(e). B is what makes “cannot obscure” true under the actual deployment.** If `docs/01` keeps “compliance as architecture,” pick B or stop saying the trail cannot be obscured. Do not pick B as a Part 11 citation; pick it as a threat-model citation.

### 5.4 11.50 manifestations on printed/PDF records — CONFIRM PLAN gap; kernel-shaped; retrofit cost is every report, not every module

`docs/02` kernel list includes “reporting + print.” Catalog Phase 0: “Reporting and print | M | Server-side PDF for travelers, certificates, labels.” Stack table: “Server-rendered to PDF… must archive.”

**PLAN §5 crate graph: absent.** No `datum-print`, `datum-report`, `datum-pdf`. Wave 3 `datum-server` “everything” would swallow it, which means Wave 2 `datum-esign` cannot depend on a print primitive, and every Phase 1–4 module will invent a PDF.

11.50(b) is unambiguous: the three manifestation fields “shall be included as part of any human readable form … (such as electronic display or **printout**).” Annex 11 §8.1 clear printed copies; §8.2 batch-release printouts **indicate if data changed since original entry**.

If print is not a kernel choke point, a third-party module (the reason the kernel exists — `docs/01`, `docs/03`) can ship a traveler PDF that omits the signature block. That is the same failure mode ADR 0005 used to reject “audit as a module.”

**Confirm:** PLAN dropped a kernel crate the architecture already required. **Not a rewrite of storage.** **Is a rewrite of every printed form** if you wait until Phase 4 DHR. Put a `datum-print` (or fold into `datum-documents`) on the crate graph before Wave 2 stubs freeze names. Minimum Wave 2: a type for “human-readable rendering of a signed record” that **must** include 11.50(a)(1)–(3) and, for release records, an “unchanged since” / “changed, see audit” mark (Annex 11 §8.2).

### 5.5 11.200 two components / re-auth / 11.300 — CONFIRM handwave; KILL non-retrofit

See tables 4.6–4.7. `docs/02` names 11.200 re-auth and inactivity timeout. It does not name two identification components, (a)(1)(i) continuous-session rule, (a)(3) collusion, or any of 11.300.

**Kill as non-retrofittable.** Identity schema migrations are local to `datum-identity`. **Confirm:** write 11.300(a)(b)(d) and 11.200(a)(1)+(a)(3) into the Wave 2 identity/esign acceptance criteria or they will be “we’ll do it in the UI” and then they will not be.

OIDC: identification code is the IdP subject; password lives at the IdP. 11.300 then splits: uniqueness and disable are still Datum; aging/lockout/alerting may be the IdP. Signing still needs a step-up (IdP re-auth or a local second component). `docs/06` must say this. Do not assume OIDC shops are Part 11-complete because SSO exists.

### 5.6 Closed vs open for LAN + browser — KILL as a kernel miss (see §3)

Default is closed. Document it. Watch OIDC and portal.

### 5.7 Operational checks 11.10(f) + gap-free numbering — CONFIRM classic hole; KILL “rewrite every module”

State machines as a kernel engine are the right 11.10(f) answer. Modules declare transitions; the engine refuses illegal ones; hooks veto (training, calibration). That *is* retrofit-safe.

**Numbering is not.** `docs/02`: “Gap-free, collision-free, configurable sequences per document type. Sounds trivial and is not, because a gap in a regulated numbering sequence is a question you have to answer.” PLAN has `datum-numbering` depending only on `core` + `db`. No algorithm.

PostgreSQL `SEQUENCE` / `GENERATED … AS IDENTITY` **are not transactional**. A rolled-back insert consumes a value. Failed transactions produce gaps. That is the classic hole.

Gap-free requires allocating the number **in the same transaction** from a counter row (`UPDATE … RETURNING` / `SELECT … FOR UPDATE` on a `document_type` register), not from `nextval()`. Collision-free under concurrency is the lock. Performance at shop volume is not a problem.

If Wave 2 ships `SEQUENCE`-backed numbers, historical gaps are permanent audit questions. Fixing the crate later does not fill them. Modules that used the kernel API do **not** need a rewrite.

**Confirm PLAN underspecification.** `datum-numbering` acceptance must ban `SEQUENCE` for regulated document numbers. Tests: abort a transaction, assert the number was not consumed.

### 5.8 Device checks 11.10(h) — later module + kernel hook; CONFIRM cheap column now

Barcode gun / instrument identity is `barcode` (Phase 7), `shop-floor` (Phase 3), `calibration` (Phase 4). The regulation says “as appropriate.” A medical-device shop capturing inspection results from a gage, or clocking on from a shared tablet, is “appropriate.”

Kernel hook: persistence/audit stamps `source_device_id` / `terminal_id` / `source_kind` when the caller supplies it, and a policy bit can make it required for floor routes. Without the column, historical records never gain a device identity.

**Not a rewrite of modules. Not deferrable out of the audit row type.**

### 5.9 Validation 11.10(a) + IQ/OQ — CONFIRM PLAN §7 gap; KILL storage rewrite

Customer validates. Vendor supplies intended use, requirements, protocols, and executable tests. QMSR 820.10 → ISO 13485 4.1.6: “The organization shall document procedures for the validation of the application of computer software used in the quality management system. Such software applications shall be validated prior to initial use and, as appropriate, after changes… proportionate to the risk… Records of such activities shall be maintained (see 4.2.5).” Same duty at 7.5.6 (production software) and 7.6 (metrology software). FDA CSA (2025) is how FDA now wants that evidence built (risk-based, not script-everything).

`docs/02` §9: “A published test suite the customer can run as part of their own installation qualification, which turns our test coverage into part of their validation package rather than an internal artifact.”

Catalog Phase 6 `validation-pack`: “Generates the customer's installation and operational qualification package: configuration manifest, requirement traceability matrix, and executable test protocols with results.”

**PLAN §7:** unit tests, ledger property tests, migration tests. **No customer-runnable IQ suite. No requirement IDs. No protocol packaging.**

This is a **design constraint on Wave 2 tests**, not a crate. If tests are written as internal `#[test]` with throwaway fixtures and unstable names, `validation-pack` in Phase 6 will have nothing to generate and you will rewrite the test harness — not the modules.

**Confirm PLAN gap.** Wave 1 workspace or `doc-regulatory` should freeze: stable protocol IDs, a `just iq` (or equivalent) that a customer can run against a fresh install, mapping to intended-use requirements. Deep for `datum-ledger` property tests because those *are* the 11.10(a) “discern invalid or altered records” evidence.

### 5.10 Backup/restore as a validated procedure — CONFIRM PLAN gap; KILL non-retrofit

`docs/02` §8 last bullet: “Backups are a first-class feature with a restore drill documented, not an exercise left to the customer.”

ADR 0003: “Point-in-time recovery and logical replication, so backup is a solved problem rather than a feature to write.” That sentence is false as a product claim. PostgreSQL *can* PITR. A bundled cluster on a shop PC with no DBA does not PITR itself. Annex 11 §7.2: regular backups; “Integrity and accuracy of back-up data and the ability to restore the data should be checked during validation and monitored periodically.”

**No crate. No Wave 1 task. No Wave 2 crate.** For a vendor that hides PostgreSQL (`adr/0003`: “The user never learns it is there”), backup **is the vendor’s feature**. Telling the customer “use pg_dump” contradicts the install story and 11.10(c).

Retrofit-able as `datum-backup` or `datum-server` commands (`backup`, `restore`, `restore-drill`). Not a module rewrite. Still a PLAN honesty gap against `docs/02`.

### 5.11 Configuration manifest (docs/03 §8) — CONFIRM Wave 2 acceptance, not a missing crate

`docs/03` §8: “The system must produce a configuration manifest: every module, every version, every enabled state, hashed and exportable.” Change of module set is a customer change-control event. `regulated = true` modules are in the manifest.

PLAN: `datum-module` Wave 2, depends on everything. Crate exists. **Hashed export format is not an acceptance criterion in PLAN.** If Wave 2 ships install/enable without a hashed, signed-capable export, IQ attachments in Phase 6 have nothing to attach.

Not non-retrofittable. Put the format in the Wave 2 stub (canonical JSON + sha256, later signature). Needs to exist *now* as a type, not as a Phase 6 surprise.

---

## 6. EU Annex 11, ISO 13485, QMSR — software-design constraints ADR 0005 does not mention

ADR 0005 is a Part 11 11.10(e)+signature ADR. The beachhead customer is “Almost certainly ISO 13485 certified, probably FDA registered” (`docs/01` §3). As of 2026-02-02 that FDA registration is **QMSR**, i.e. ISO 13485 with 820.35/820.45 extras. Annex 11 is not legally binding on a US-only device shop, but it is the EU inspector’s computerised-system text and the project’s adjacent market.

### 6.1 Annex 11 (2011, in force) — design constraints the kernel must not pretend are SOP-only

| Annex 11 | Constraint | Vs ADR 0005 / PLAN |
|---|---|---|
| Principle | Application validated; **IT infrastructure qualified**. Replacing manual ops must not increase risk. | IQ of the *bundled* PostgreSQL+OS is vendor-owned on a hidden cluster. Unmentioned. |
| §1 Risk management | Lifecycle, patient safety, data integrity, product quality. Extent of validation from documented risk. | No risk file for the kernel. CSA wants this. `docs/06`. |
| §3 Suppliers | Formal agreements; supplier audit risk-based; **quality system and audit information relating to suppliers or developers of software … available to inspectors on request** (§3.4). | Open-source helps *and* hurts: there is no vendor QMS to hand over unless the project keeps one. `docs/06` / CONTRIBUTING. |
| §4.3 | Up-to-date listing of all relevant systems and GMP functionality. Critical systems: physical/logical arrangements, data flows, interfaces, prerequisites, security. | Config manifest (`docs/03` §8) is the listing. Hashed export. |
| §4.4 | URS traceable throughout the life-cycle. | validation-pack RTM. PLAN §7 has no requirement IDs. |
| §4.8 | If data are transferred to another format or system, validation includes checks that data are **not altered in value and/or meaning**. | Complete-copy / migration format. Kernel storage concern. |
| §5 Data | Built-in checks on electronic exchange with other systems. | Event bus + public API. Later. |
| §6 Accuracy checks | Critical **manual** data: second operator **or** validated electronic means. | State-machine dual-sign / verification transitions. Kernel can offer “requires confirmation by a second actor.” Unmentioned. |
| §7.1–7.2 | Physical+electronic protection; accessibility/readability/accuracy throughout retention; **regular backups; restore checked during validation and periodically**. | Backup crate missing. |
| **§8.1–8.2 Printouts** | Clear printed copies. **Batch-release printouts indicate if any data changed since original entry.** | Print crate missing. 8.2 is a renderer requirement on top of 11.50(b). |
| **§9 Audit trails** | Risk-based, system-generated, **reason for change or deletion documented**, available and convertible to a generally intelligible form, **regularly reviewed**. | Reason field weaker in ADR than Annex 11. Review workflow = later module (`internal-audit` is Phase 6 and is the wrong module — need an *audit-trail review* UX). Intelligible form = print/export. |
| §10 Change/config | Changes including configurations only in a controlled manner. | `docs/03` §8. `datum-module` enable/disable is a change-control event. |
| §12.3 | Creation, change, cancellation of **access authorisations** recorded. | Identity changes must be audited. `datum-identity` depends on `datum-audit` — good. |
| §12.4 | Management systems “designed to record the identity of operators entering, changing, confirming or deleting data including date and time.” | Persistence-layer audit. On target. |
| §13 Incidents | All incidents reported and assessed; critical root cause → CAPA. | P + later `capa`. Kernel: crash/restore events into audit. |
| §14 E-sign | Same impact as handwritten within the company; permanently linked; time and date applied. | Weaker than Part 11 11.200/11.300. ADR 0005 covers this subset. |
| §15 Batch release | Only Qualified Persons; identity recorded; **electronic signature**. | Later production/DHR. Kernel e-sign is the primitive. |
| §16 Business continuity | Alternative arrangements documented and **tested**; time to switch based on risk. | P. Air-gap already. |
| **§17 Archiving** | Archived data checked for accessibility, readability, integrity. If equipment or programs change, **ability to retrieve is ensured and tested**. | The migration/export format again. This is why “pg_dump is the archive” fails. |

2025 Annex 11 draft (consultation) tightens audit-trail “who/what/when/why,” timezone, and signature manifestation on screen **and** print. Do not design against a draft; do not pick a schema that the draft would immediately break (store timezone; store reason; manifestation on print).

### 6.2 ISO 13485:2016 — record control and software validation

| Clause | Obligation (standard wording, cross-checked) | Software-design implication ADR 0005 omits |
|---|---|---|
| **4.1.6** | Documented procedures for validation of computer software used in the QMS. Validate prior to initial use and after changes. Approach proportionate to risk. Records maintained (4.2.5). | Customer duty. Vendor must ship intended-use + tests that *are* that evidence. PLAN §7 silent. |
| **4.2.4** | Control of documents: approve, review, revision status, relevant versions at points of use, obsolete identified and retained for at least the lifetime of the device (not less than resulting records). | `datum-documents` is the kernel primitive. Effectivity + obsolete retention is a schema concern (do not hard-delete obsolete revisions — already implied). |
| **4.2.5** | Records: identification, storage, **security and integrity**, retrieval, retention time, disposition. Protect confidential health information. Remain legible, readily identifiable, retrievable. **Changes to a record shall remain identifiable.** Retain at least lifetime of the device, or regulatory, **but not less than two years from release**. | Integrity + “changes remain identifiable” is audit + versioned records (ADR 0005). **Retention measured in decades** is archival/export (MISSING). Confidential health info appears in `complaints` (Phase 6) — confidentiality was in the 11.10 chapeau “when appropriate” and is unmentioned in the ADR. |
| **7.5.6** | Validation of processes for production and service provision — includes software used in production, same proportionate-to-risk rule. | Datum *is* production software for a device shop (work orders, DHR). 4.1.6 + 7.5.6 both apply to the customer’s use of Datum. |
| **7.6** | Monitoring and measuring equipment — software used in metrology validated. | `calibration` + inspection. Later module; kernel e-sign/audit still apply. |

### 6.3 21 CFR 820 QMSR (in effect 2026-02-02)

Datum is **not a medical device**. It is production/QMS software. Consequences:

- Old **820.70(i)** (validate software used in production or the quality system) is **withdrawn**. The duty now lives in ISO 13485 4.1.6 / 7.5.6 / 7.6 via **820.10(a)**.
- Old **820.30 design controls** do **not** apply to Datum-the-product. They apply to the customer’s Class II/III devices (and Class I “devices automated with computer software” — 820.10(c)(1)). Do not build a DHF into Datum (`docs/01` non-goal is correct).
- Old **820.180** record retention is replaced by ISO 13485 4.2.5.
- **820.35** adds content requirements on top of 4.2.5: complaint records (name of device, date received, UDI/UPC, complainant, nature, correction, reply), servicing records, **UDI recorded for each device or batch**, confidentiality marking. These are **later modules** (`complaints`, `udi`, `maintenance`) but the **record schema must be able to carry UDI and confidentiality flags** without a kernel rewrite. Custom fields can do some of this; UDI-on-every-record is a genealogy/DHR concern, not Wave 2.
- **820.45** labeling/packaging mixup controls, UDI/expiry/storage on the label, documented inspection before release. `barcode` + print + state machine. Print crate again.

**ADR 0005 does not mention QMSR, CSA, or ISO 13485 at all.** `docs/04` still uses DHR/DMR vocabulary (fine as customer artifacts; the QMSR no longer names those files). `docs/06` must retarget 820 → QMSR or the first FDA-literate reader will think the project froze in 2023.

---

## 7. SOP half vs kernel half

If Part 11 still requires customer SOPs for half the clauses, say which half. Here is the split. “SOP” means the manufacturer’s written procedure is the primary control; the software may assist.

**Primarily customer SOP (software cannot discharge):**

- 11.10(a) *execution* of validation (vendor supplies the pack)
- 11.10(i) education/training/experience determination
- 11.10(j) written accountability policies
- 11.10(k)(1) distribution of operation/maintenance documentation (partial)
- 11.100(b) identity verification before assigning a signature
- 11.100(c) letter of non-repudiation to FDA
- 11.300(c) physical token loss procedures
- 11.300(e) periodic testing of physical tokens
- Closed-system physical access, OS administration, clock administration
- Annex 11 §1 risk management, §2 personnel, §3 supplier agreements, §11 periodic evaluation, §13 incidents, §16 business continuity alternatives
- ISO 13485 QMS procedures, retention *schedule* (the number of years), confidential-health-info policy

**Must be kernel (or a kernel hook), or a third-party module can punch a hole:**

- 11.10(d)(e)(f)(g) access, audit generation, sequencing, authority
- 11.10(b)(c) complete copies + retrieval — **claimed kernel, crates missing**
- 11.10(h) device identity stamp
- 11.10(k)(2) revision trail of controlled docs
- 11.50(a)(b) manifestation stored **and** printed
- 11.70 linkage
- 11.100(a) unique never-reassigned identity
- 11.200(a) two components, re-auth, collusion-resistant admin reset
- 11.300(a)(b)(d) unique IDs, aging, lockout + unauthorized-use reporting
- Hashed configuration manifest
- Backup of a hidden PostgreSQL

That is not “half SOP therefore architecture is a marketing line.” The architecture claim is true for **generation and binding**. It is not yet true for **copy, retain, print, backup, or identity-lifecycle**. Those are the clauses inspectors actually walk.

---

## 8. PLAN gaps (actionable)

`docs/06` gated on spike — **good. Do not ungate.** This report and `spike-regulatory.md` are inputs, not substitutes.

| Gap | Where claimed | PLAN today | Severity |
|---|---|---|---|
| `docs/06-regulatory.md` absent | PLAN §2 | Gated — correct | Expected |
| **No print/report crate** | `docs/02` kernel, catalog Phase 0, 11.50(b), Annex 11 §8 | Not in §5 graph | **High** — freeze of Wave 1 stubs will omit it |
| **No backup/restore crate or Wave 1 task** | `docs/02` §8, Annex 11 §7.2, 11.10(c) | Absent; ADR 0003 handwaves PITR | **High** for bundled PG |
| **No archival/export complete-copy API** | 11.10(b)(c)(e), Annex 11 §17 | Absent | **High** for schema, medium for implementation timing |
| **`datum-identity` 11.300 / 11.200(a)(1)(3) scope** | `docs/02` §8 partial | Crate exists, scope handwaved | **Medium** (schema now) |
| **`datum-numbering` algorithm** | `docs/02` gap-free | Crate exists, SEQUENCE hole unaddressed | **High** if they use IDENTITY |
| **Customer-runnable IQ suite** | `docs/02` §9, catalog Phase 6 | PLAN §7 internal tests only | **Medium** (harness design now) |
| **Hashed module manifest format** | `docs/03` §8 | `datum-module` exists, format unstated | **Medium** |
| **Audit reason + device + hash columns** | Annex 11 §9; 11.10(h); §5.3 | Unstated row type | **High** for Wave 2 contract |
| Closed/open classification | 11.3, 11.30 | Unstated | `docs/06` |
| QMSR/CSA/ISO 13485 4.1.6 retarget | Beachhead | Unstated | `docs/06` |

Nothing in PLAN §10 (out of scope) excuses these: they are kernel, not Phase 2+ modules.

---

## 9. Non-retrofittable misses (the actual list)

A miss is **non-retrofittable** iff adding it in v3 requires rewriting modules already written, or permanently losing evidence for records already stored.

| Miss | Non-retrofittable? | Why |
|---|---|---|
| Persistence-layer audit (ADR 0005) | Yes — **already decided correctly** | This is the one `docs/01` is right about. |
| E-sign bound to record-version hash | Yes — **already decided correctly** | 11.70. |
| Complete-copy **fields** (prior value, reason, actor, server time, record id, printed name) | **Yes, if omitted from Wave 2 schema** | Cannot reconstruct. |
| Complete-copy **exporter / PDF-A format** | No (if fields exist) | Add a crate. |
| Print choke point for 11.50(b) / Annex 11 §8.2 | **Yes, as an architectural property**; no as storage | Same argument as audit-as-module. PLAN currently fails this. |
| Hash chain | **No** (backfill once); **schema yes if you want it from row 1** | Opus decision. |
| Trusted time | No | SOP + jump detection. |
| 11.300 aging/lockout/alerts | No | Identity migration. |
| Gap-free numbering algorithm | **Yes for historical numbers** if Wave 2 uses SEQUENCE | Crate-local fix going forward. |
| Device id on audit row | **Yes for historical rows** if column absent | One column. |
| IQ harness as customer evidence | No for modules; **yes for test-name stability** | Write tests with protocol IDs now. |
| Backup/restore | No | Server feature. |
| Config manifest hash | No | `datum-module` Wave 2. |
| 11.30 encryption | No | Only if they go open. |

**There is no kernel omission that forces a rewrite of every module, provided Wave 2 does not ship a thin audit row and does not let modules print.** The failure mode to fear is Wave 2 freezing an audit DTO without reason/device/hash/export, and Wave 3 letting each module emit PDFs. That is how you spend the “cannot be retrofitted” card on the wrong things and then have to retrofit the right ones.

---

## 10. EXECUTOR, DECISIONS, TIER

### 10.1 `doc-regulatory` (PLAN: deep — **agree**)

- **EXECUTOR:** grok or cursor, **deep**, gated on `spike-regulatory.md` **and** this report. Do not dispatch blind. Do not let the lane summarize blogs.
- **Must use:** eCFR Part 11, Annex 11 2011 PDF, QMSR 820.10/820.35/820.45, ISO 13485 4.1.6/4.2.4/4.2.5/7.5.6/7.6, FDA 2003 Scope, FDA CSA 2025.
- **Must not:** invent `docs/06` content that contradicts ADR 0005; must *extend* it clause-by-clause using a table like §4.
- **Deliverable shape:** coverage table (K/M/P/X), closed-system claim with boundary conditions, IQ-pack mapping, QMSR retarget (stop citing 820.70(i) as if it existed).
- **AUDIT TIER:** deep. This document becomes the validation traceability backbone.

### 10.2 Identity / e-sign / audit lanes (Wave 2)

| Lane | TIER | EXECUTOR | Binding acceptance this sweep adds |
|---|---|---|---|
| `datum-audit` | **deep** | grok or cursor; **not** a casual crate | Row: actor, server `timestamptz`, action, entity type/id, old value, new value, **reason**, **source_device optional**, same transaction as write. Grant INSERT/SELECT only for app role. Export-shaped DTO (complete copy). **Hash columns if opus picks B.** |
| `datum-esign` | **deep** | same | Hash of exact record version; printed name; meaning; server time; re-auth all components; inactivity timeout; signature rows append-only; cannot attach a signature to a different record and have the hash match. |
| `datum-identity` | **deep** | same | Unique never-recycled IDs; Argon2id; RBAC; floor session lock; **password aging; lockout; security-event log for 11.300(d); admin reset that does not unilaterally break 11.200(a)(3).** OIDC is optional and does not skip sign-time step-up. |
| `datum-numbering` | **deep** | same | **No `SEQUENCE` for regulated numbers.** Transactional allocate. Property test: aborted txn does not consume a number. |
| `datum-statemachine` | deep (already, for other reasons) | — | 11.10(f) sequencing; signature-required transitions; optional second-actor confirmation (Annex 11 §6). |
| `datum-module` | standard+ | — | Hashed export of id/version/enabled/`regulated`. |
| `datum-documents` | standard | — | 11.10(k)(2), ISO 4.2.4. Candidate home for print if no new crate. |
| **print crate (missing)** | **deep once added** | — | 11.50(b) + Annex 11 §8. Opus: add to graph or formally fold into `datum-documents` **before Wave 1 stubs freeze**. |
| **backup (missing)** | standard, Wave 3-ok if accepted as not-Wave-2 | — | Restore drill command. Do not leave it as “PITR exists in PostgreSQL.” |

### 10.3 Opus decisions required **before Wave 2**

1. **Hash-chained audit vs grant-only (A/B/C in §5.3).** Not a Part 11 letter requirement. Is a `docs/01` honesty requirement. **Decide before `datum-audit` stubs.** Recommendation: B (per-record or global chain) + C (signed/off-box export) if the marketing line stays “cannot obscure”; A only if the claim is rewritten to “the application cannot obscure.”
2. **Print crate in the PLAN §5 graph, or an explicit fold into `datum-documents` with an edge from `datum-esign`.** Today the architecture and the plan contradict.
3. **Conservative 11.200:** always all components (ignore (a)(1)(i) relaxation) until a shop asks. Cheap.

Not opus-blocking: trusted time (document as OS clock), 11.30 (closed), device checks (optional column), IQ harness (design constraint, Wave 1 `justfile`).

### 10.4 TIER for this slice’s effect on the plan-audit

**deep.** Integration-critical, contract-bearing for four Wave 2 crates, and the project’s central product claim.

---

## 11. What I did not verify

- `spike-regulatory.md` has not landed; I did not read it.
- I did not buy ISO 13485:2016; clause text is cross-checked from multiple independent excerpts that agree. Treat punctuation as unofficial.
- I did not inspect a Wave 2 stub (none exist). Numbering-SEQUENCE is a predicted hole, not an observed one.
- Annex 11 2025 draft was not treated as in-force.
- No product files were edited.

---

## 12. One-page answer for the master auditor

**Does the kernel list cover 21 CFR Part 11?** No. It covers 11.10(e) generation, 11.70 binding, identity/RBAC, state machines, numbering-as-a-problem-statement, documents, and a 11.200 re-auth sentence. It does not cover complete copies, retention/archival, print manifestations, backup, 11.300, device checks, IQ-as-customer-evidence, or closed/open classification.

**Something missing that cannot be retrofitted?** Only if Wave 2 ships an incomplete audit row or lets modules print. Print is already declared kernel and then omitted from PLAN — that is the miss most likely to force a later rewrite of every report. Hash-chaining is not legally required; opus should still decide it before the audit schema freezes. Grant-only is not “cannot obscure” on a bundled cluster whose owner is superuser.

**SOP half is real and acceptable.** Training, policies, FDA letter, identity proofing, validation *execution*, token procedures, clock admin. Do not try to kernel those.

**PLAN:** keep `docs/06` gated (good). Add print (or fold), backup, numbering algorithm, identity 11.300 scope, audit export DTO, hashed manifest format, customer IQ entrypoint. `doc-regulatory` stays **deep**.
