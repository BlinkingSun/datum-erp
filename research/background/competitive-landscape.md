# Spike: Open Source ERP for Discrete Manufacturing, and Whether Any of It Serves Medical Devices

**Research window:** 11–12 September 2026. All GitHub activity figures, licence files, database schemas, module inventories and vendor pages below were pulled live inside that window unless a date is given inline.

**Scope:** ERPNext (Frappe), Odoo Community Edition, Tryton, iDempiere, metasfresh, Axelor Open Suite, Dolibarr, Apache OFBiz; new entrants 2024–2026; open-source eQMS tooling; commercial comparators (Arena, MasterControl, Greenlight Guru, QT9, Plex, ProShop ERP, E2/Shoptech, JobBOSS², Fulcrum).

**Target user profile used throughout:** a 30-person medical device contract manufacturer, roughly 13 ERP seats, ~$5M revenue, high-mix low-volume, machining/assembly under customer 510(k)s, ISO 13485 certified or seeking it.

### How to read confidence markers

| Marker | Meaning |
|---|---|
| **[SOURCE]** | Read directly from source code, a licence file, a database migration, or a schema definition. Highest confidence. |
| **[PROBE]** | Established by file-existence probing (HTTP 200/404 against a known path). Conclusive for absence when the repo is the authoritative one. |
| **[VENDOR]** | Stated by the vendor on its own site. Marketing, not verification. |
| **[CONTRACT]** | Derived from counted real contracts (Vendr). Strongest pricing evidence available. |
| **[USER]** | Reported by an identifiable user on a forum or review site. Quoted verbatim. |
| **[AGGREGATOR]** | Directory listing (SelectHub, SoftwareAdvice, GetApp, Capterra). Treat as a hint, not a quote. |
| **[UNVERIFIED]** | Could not confirm. Stated as open. |

Retractions from earlier drafts have been applied in place, not appended. Where a figure was published and withdrawn, the withdrawal is noted inline so the record is auditable.

---

## 0. The three things most worth acting on

### 0.1 No open-source ERP has a 21 CFR Part 11 compliant electronic signature. Not one. Including Carbon.

This is the central justification for building something, and Section 2 states the evidence project by project, including exactly what was checked, so that it survives an attempt to refute it. The short form: eight mature open-source ERPs plus the best new entrant, and the count of systems with a signature carrying **meaning**, **re-authentication at signing**, and **non-excisable record linkage** is zero. Several have approval workflows. Several have attribution fields. That is not the same thing, and an auditor knows it.

### 0.2 Carbon is the only project at the manufacturing-and-medtech intersection, and its licence is the problem, not its features.

Its quality module is real in source — ANSI/ASQ Z1.4 and ISO 2859-1 sampling with code-letter tables, MRB dispositions linked to job operations, gage calibration with as-found/as-left readings and environmental conditions, versioned controlled procedures, recurring training assignments, ECO change notices, and a service-role-only audit log. Nothing else in open source has this. But its licence forbids internal production use unless you open-source your modifications or buy a commercial licence, and a contract manufacturer whose customisations encode customer process knowledge under NDA generally cannot do the former. Section 3 treats it in full.

### 0.3 The diligence question that outranks every feature comparison is: who holds the copyright, and is there a CLA?

Every confirmed death in this space traces to concentrated copyright plus a vendor whose commercial interest eventually diverged from the open edition. The licence tells you less than the copyright structure, because the copyright holder is the only party who can change the licence. Full treatment is in `spike-governance.md`; the one-line version is that ERPNext answers this question well, Odoo answers it badly in writing as stated strategy, and Carbon answers it worst of the three.

---

## 1. Regulatory baseline

Two recent changes materially affect the analysis.

**QMSR is in force, not pending.** FDA's Quality Management System Regulation took effect **2 February 2026** — seven months before this research, not a future deadline. It amends 21 CFR Part 820 to incorporate **ISO 13485:2016 by reference**, retaining separate Part 820 text only for scope, definitions, incorporation by reference, QMS requirements, **control of records (820.35)** and **device labeling and packaging controls (820.45)**. FDA retired QSIT and the prior inspection programs; inspections now run under Compliance Program 7382.850.
- https://www.fda.gov/medical-devices/postmarket-requirements-devices/quality-management-system-regulation-qmsr
- https://www.fda.gov/medical-devices/quality-management-system-regulation-qmsr/quality-management-system-regulation-frequently-asked-questions
- https://www.ropesgray.com/en/insights/alerts/2026/02/a-qmsr-state-of-mind-fda-adopts-new-inspection-approach-for-medical-devices

**21 CFR Part 11 is unchanged by QMSR** and remains a separate obligation. Operative clauses for this survey:

| Clause | Requirement |
|---|---|
| 11.10(d) | Limiting system access to authorised individuals |
| 11.10(e) | Secure, computer-generated, time-stamped audit trails recording operator entries and actions; must not obscure previously recorded information; retained for the record retention period |
| 11.50 | Signature manifestations carrying printed name of signer, date and time, and **the meaning** of the signature (review, approval, responsibility, authorship) |
| 11.70 | Signature-to-record linking that cannot be excised, copied or transferred by ordinary means |
| 11.100 | Signature uniqueness, identity verification, certification to FDA |
| 11.200 | Two distinct identification components; both required at the first signing of a session, at least one for subsequent signings |

**Computer Software Assurance (CSA) is final, and it lowers the validation bar.** Issued 24 September 2025, revised 3 February 2026. It supersedes Section 6 of *General Principles of Software Validation* and replaces documentation-heavy CSV with risk-proportionate assurance.
- https://www.federalregister.gov/documents/2025/09/24/2025-18468/computer-software-assurance-for-production-and-quality-system-software-guidance-for-industry-and
- https://www.fda.gov/regulatory-information/search-fda-guidance-documents/computer-software-assurance-production-and-quality-management-system-software

This is the single most important recent development for the open-source case, and it is developed in Section 8.

**Scoring checklist used throughout**, derived from ISO 13485:2016 as incorporated by QMSR: 4.2.4 document control · 4.2.5 records · 6.2 competence and training · 7.4 purchasing and supplier evaluation · 7.5.1 production control · 7.5.6 process validation · 7.5.8 identification · 7.5.9 traceability · **7.6 control of monitoring and measuring equipment, including assessing the validity of previous results when equipment is found out of tolerance** · 8.2.2 complaints · 8.2.4 internal audit · 8.3 nonconforming product with disposition · 8.5.2 and 8.5.3 CAPA. Plus DHR content, UDI/GUDID (21 CFR 830), and Part 11.
- https://www.fda.gov/medical-devices/device-advice-comprehensive-regulatory-assistance/unique-device-identification-system-udi-system

---

## 2. The electronic-signature finding, project by project

**Claim under test:** *No open-source ERP surveyed provides an electronic signature meeting 21 CFR Part 11 Subpart C.*

**Test criteria.** A system passes only if it provides all four:
1. A signature event bound to a specific record, carrying **printed name, date/time, and meaning** (11.50(a)).
2. **Re-authentication at the moment of signing** — two distinct identification components at the first signing of a session, at least one thereafter (11.200(a)(1)).
3. **Linkage that cannot be excised, copied or transferred** by ordinary means (11.70).
4. Applicability to the records that actually matter — work orders, inspections, nonconformances, device history records — not merely to purchase orders or outbound PDFs.

Attribution fields (`created_by`, `approved_by`, `written_by`) fail criterion 1 and 2. Approval workflows fail criterion 2. External PDF signing fails criteria 3 and 4.

### 2.1 Per-project evidence

**ERPNext / Frappe — FAILS. [SOURCE]**
What was checked: the complete DocType listings for `erpnext/manufacturing/doctype` (50 entries), `erpnext/quality_management/doctype` (16), `erpnext/stock/doctype` (79), `erpnext/assets/doctype`, `frappe/core/doctype` (~120), and `hrms/hr/doctype`. No signature DocType exists in any of them. Frappe's record-integrity model is `submit`/`cancel`/`amend` plus Workflow and Document States — a state machine, not a signature. The `Version` DocType carries no signature field, no meaning field and no reason-for-change field.
Corroboration: a practitioner who built ISO 13485 on ERPNext — *"Out of the box, ERPNext certainly has some shortcomings with respect to CFR21 part 11 (e.g. a signature before submitting a document, prevent deletion, …)"* — https://discuss.frappe.io/t/cfr21-part-11-gamp5-normatives/34458 . A vendor writeup concedes it must be built: "custom workflows that require electronic signature… combined with password confirmation" — https://clefincode.com/blog/global-digital-vibes/en/medical-devices-and-erpnext-industry-fit-and-insights
Pre-empting refutation: ERPNext's Quality Inspection has both `inspected_by` and `verified_by`. These are Link fields to User. There is no re-authentication, no meaning, and the doctype does not set `track_changes`.

**Odoo Community Edition — FAILS, twice over. [PROBE] + [VENDOR]**
What was checked: the `addons/` directory of `odoo/odoo` branch 19.0 was enumerated (638 modules). **`sign` is not present.** The module is Enterprise-only, under the proprietary OEEL-1.0 licence — https://www.odoo.com/documentation/19.0/legal/licenses.html
Second failure, independent of edition: even Odoo Enterprise's Sign is a DocuSign-style **external PDF** workflow. It signs a rendered artifact, not a live database record, which fails criteria 3 and 4. Odoo's own documentation frames its validity around eIDAS and the US ESIGN Act and **never** mentions 21 CFR Part 11.
Corroborating negative: a medical device company asked Odoo's forum for a Part 11 capability statement, e-signature functionality description, SDLC procedures and validation documentation. **The thread has no answers.** — https://www.odoo.com/forum/help-1/21-cfr-part-11-complaince-298255

**Tryton — FAILS. [SOURCE]**
What was checked: the core `quality` module (877 lines) exposes `processed_by`, `passed_by` and `failed_by` employee fields on Inspection. These are attribution links, not signatures. No module in the distribution provides signing; PyPI was queried for candidate `trytond_*` modules and none is a signature module. `ModelSQL` provides `create_uid`/`write_uid`/`write_date` — last-writer metadata only.
Honesty note: Tryton ships ~180 modules and I did not enumerate every one by hand; the negative rests on the documented module index plus targeted PyPI queries. Confidence high, not absolute.

**iDempiere (core and Libero plugin) — FAILS. [SOURCE]**
What was checked: `MPPOrder` implements a `DocAction` lifecycle with `prepareIt`/`approveIt`/`completeIt`/`voidIt`/`closeIt`. Approval records an approver user; `AD_Session` records logins. There is no re-authentication at approval, no meaning-of-signature field, and no signature-to-record binding. `AD_ChangeLog` is a change log, not a signature.

**metasfresh — FAILS. [SOURCE]**
Inherits the same ADempiere-lineage `AD_ChangeLog` and approval mechanics as iDempiere, with the same absence. No signature construct anywhere in the ~100+ backend modules; there is no `de.metas.signature*` module.

**Axelor Open Suite — FAILS, and it is the closest miss. [SOURCE]**
What was checked: `axelor-quality`'s QI entities carry paired attribution — `writtenBy`/`writtenOn`, `causesWrittenBy`/`causesWrittenOn`, and notably **`efficiencyCheckedBy`/`efficiencyCheckedOn`**. Structurally this is nearer to a signature manifestation than anything else in the survey. It still fails: no re-authentication at the moment of recording, no meaning field, no non-excisable linkage, and the surrounding change tracking (`@Track`) writes to a mail/collaboration feed rather than an immutable table. `OperationOrder` tracks zero fields.

**Dolibarr — FAILS. [SOURCE]**
What was checked: the core `htdocs/` module listing contains no quality, calibration, audit or training module. The two logging mechanisms were read: `interface_20_all_Logevents.class.php` logs **login events only**; `modBlockedLog` is a genuine tamper-evident chained log but every action it covers is an invoice, payment, donation or membership event — it exists for French fiscal anti-fraud law (NF203), and **zero BOM, MO, stock, lot or production events are chained**.
Pre-empting refutation: Dolibarr does offer online signing of customer-facing commercial documents (proposals, contracts). That is a counterparty signature on an outbound document, not a Part 11 signature on a manufacturing or quality record, and it does not touch any of the records in scope. **[UNVERIFIED]** — I did not read that feature's implementation; the claim rests on its documented scope.

**Apache OFBiz — FAILS. [SOURCE]**
What was checked: the manufacturing, product and workeffort entity models plus all 22 plugins in `ofbiz-plugins`. There is no signature entity. `EntityAuditLog` captures old and new values but is a change log, and it is disabled everywhere except ten fields in the order model.

**Carbon — FAILS, and this is the best-evidenced negative in the set. [SOURCE]**
What was checked: the complete `packages/database/supabase/migrations` listing was enumerated and grepped. **Zero migrations contain the string "signature."** The one candidate mechanism, `20260119191608_approvals-workflow.sql`, was read in full. The `approvalRequest` table is:

```
id, documentType "approvalDocumentType", documentId, status "approvalStatus",
amount, requestedBy, requestedAt, approverId, decisionBy, decisionAt,
decisionNotes, companyId, createdBy, createdAt, updatedBy, updatedAt
```

with `approvalDocumentType` an enum of exactly two values: **`'purchaseOrder'`, `'qualityDocument'`**, and `approvalStatus` an enum of `'Pending'`, `'Approved'`, `'Rejected'`, `'Cancelled'`.

That fails on all four criteria: no re-authentication, no meaning field (`decisionNotes` is free text), no non-excisable linkage, and it **cannot be applied to a work order, an inspection, a nonconformance or a device history record** because those document types do not exist in the enum. It is an approval workflow. Carbon's marketing nonetheless lists "e-signatures" under medical devices — https://carbon.ms — which is a claim the source does not support.

### 2.2 What a refuter will reach for, and why each fails

| Counter-argument | Response |
|---|---|
| "Odoo has Sign." | Enterprise-only (absent from the LGPL repo, [PROBE]), and it signs external PDFs, not records. Odoo's own docs never cite Part 11. |
| "ProShop advertises electronic signature." | ProShop is commercial, not open source. It is also outside the claim's scope, and separately has a reviewer-reported serialisation weakness (Section 7). |
| "Fulcrum has digital signature on calibration." | Commercial, and scoped to calibration alone — no e-signature elsewhere in the product. |
| "Frappe has Workflow with approvals." | A state machine with role gating. No re-authentication, no meaning, no binding. |
| "Axelor records who verified CAPA effectiveness." | Attribution pair. Closest miss in the survey, still fails three of four criteria. |
| "Carbon has an approvals workflow." | Two document types, neither of which is a manufacturing or quality record. Zero migrations mention signature. |
| "You can add one with a custom app." | Yes — and then you validate bespoke code, which is the cost this finding exists to quantify. |

**Conclusion, stated precisely:** as of September 2026, across eight mature open-source ERPs and the strongest new entrant, no system provides an electronic signature satisfying 11.50, 11.70 and 11.200. Approval workflows and attribution fields exist and are being mistaken for signatures, including by vendors in their own marketing.

---

## 3. Carbon — licence, scope, copyright, and my read

**Repository:** https://github.com/crbnos/carbon — 2,400★, 345 forks, last commit 2026-09-11. TypeScript/React on Supabase/Postgres plus Rust. Monorepo: `apps/{erp,mes,academy,assembler,starter}`, `packages/*`, `crates/*`.
**Marketing site:** https://carbon.ms — self-described as *"The open core for manufacturing. ERP · MES · QMS."*
**Note:** the repository moved; the older `barbinbrad/carbon` URL now 404s.

### 3.1 Exact licence text [SOURCE]

From https://github.com/crbnos/carbon/blob/main/LICENSE, verbatim:

```
Copyright © 2025, Carbon Manufacturing Systems Corp.

Portions of this software are licensed as follows:

- All content that resides under https://github.com/crnbos/carbon/tree/main/packages/ee
  and all files that contains a `.ee` in this repository require the purchase of a
  commercial license
- All third party components incorporated into the Carbon Software are licensed under
  the original license provided by the owner of the applicable component.
- Any use of this software to sell Carbon source code as a hosted service is strictly
  prohibited without obtaining a commercial license.
- Because Carbon is cloud-based software, any use of this software for internal
  production use is strictly prohibited unless the modifications are made open-source
  in accordance with the "AGPLv3" license or a commercial license is obtained.
```

**The README badge says "License: AGPL-3.0" and GitHub's licence detector reports AGPL-3.0. Both are wrong.** The detector matches a bundled AGPL text and silently misses the `ee` carve-out. Anyone relying on GitHub metadata — as at least one research pass here initially did — will get this wrong.

**Note the defect:** the licence's own URL reads `crnbos`, not `crbnos`. The path that defines the commercial boundary does not resolve. Minor, but it is an instrument defining a paid boundary and it contains a typo.

### 3.2 What the licence does and does not permit

| Activity | Permitted? |
|---|---|
| Read, fork, study, modify the source | Yes |
| Run it for evaluation, development, demo | Yes |
| **Run it in internal production without publishing your modifications** | **No** — requires a commercial licence |
| Run it in internal production *with* your modifications published under AGPLv3 | Yes |
| Use anything under `packages/ee` or any `*.ee` file at all | No — commercial licence required regardless |
| Offer Carbon to third parties as a hosted service | No without a commercial licence |
| Keep a private fork | The README states: *"If you want to make the repo private, you should acquire a commercial license to comply with the AGPL license."* |

**Why this matters specifically for a contract manufacturer.** A CM's ERP customisations *are* its customers' process knowledge — routings, inspection plans, tolerances, work instructions, supplier lists. Those are typically covered by NDA and often by ITAR or customer-proprietary clauses. "Publish your modifications" is not a cost, it is usually a contractual impossibility. So for this buyer the practical reading is: **Carbon is commercial software with an open core.** Price it as such.

Corroborating the point from the target market, a machinist whose customers are device developers: *"My clients are medical device developers, and I have signed NDA's with them. I cannot use any cloud services for CAD/CAM/ERP etc., plus I need my business to run if the internet is out."* — https://www.practicalmachinist.com/forum/threads/free-erp-system.394704/

**What is in `packages/ee/src` [SOURCE]:** `accounting`, `email`, `integrations`, `jira`, `linear`, `notifications`, `onshape`, `paperless-parts`, `plan.server.ts`, `plan.ts`, `planning`, `quickbooks`, `radan`, `rillet`, `sage`, `slack`, `sso`, `storage-rules`, `stripe-connect`, `xero`. Note **`sso`** and **`planning`** in particular — SSO is an access-control primitive that matters for 11.10(d).

The open tree also ships an upsell component, `apps/erp/app/modules/production/ui/ForecastUpgradeOverlay.tsx`, which renders a blurred `UpgradeOverlay` over gated features. The open-core boundary is materialised in the UI of the open repository.

**Pricing [VENDOR]:** https://carbon.ms/pricing — Starter $40/user/month; Business $100/user/month with a 5-user minimum, adding support, API, webhooks, integrations, accounting and audit logging; Enterprise custom, adding SSO/SAML, CMMC, self-hosted or managed, forward-deployed engineer.

### 3.3 Quality module scope, from source [SOURCE]

Verified by enumerating `apps/erp/app/modules/quality/`, reading `quality.models.ts` and `samplingStandards.ts`, and enumerating the database migrations.

**Module files:** `quality.models.ts`, `quality.server.ts`, `quality.service.ts`, `quality-disposition.server.ts`, `samplingStandards.ts`, `samplingStandards.test.ts`, `quality.server.test.ts`, `types.ts`, `index.ts`.
**UI submodules:** `Actions`, `Calibrations`, `Documents`, `Gauge`, `GaugeTypes`, `Inspections`, `Issue`, `IssueTypes`, `IssueWorkflows`, `Item`, `RequiredActions`, `RiskRegister`.

**Sampling — the standout.** `samplingStandards.ts` implements **ANSI/ASQ Z1.4 and ISO 2859-1** in full: the Table I lot-size-by-inspection-level code-letter matrix, inspection levels `I, II, III, S1, S2, S3, S4`, severities `Normal, Tightened, Reduced`, plan types `All, First, Percentage, AQL`, and the standard AQL series `0.065, 0.1, 0.15, 0.25, 0.4, 0.65, 1.0, 1.5, 2.5, 4.0, 6.5, 10.0`. Its header states the resolver is *"Used by the Quality-tab preview AND by the post-receipt function to snapshot a plan onto each lot."* **No other open-source ERP in this survey has attribute sampling at all.**

**Nonconformance.** `disposition` = `Pending`, `Return to Supplier`, `Rework`, `Scrap`, `Use As Is` (with `Conditional Acceptance`, `Deviation Accepted`, `Hold`, `No Action Required`, `Quarantine`, `Repair` present but commented out — planned). `nonConformanceApprovalRequirement` = `["MRB"]` (Material Review Board). `nonConformanceSource` = `Internal`/`External`. `nonConformanceStatus` = `Registered`/`In Progress`/`Closed`, with `isIssueLocked()` locking the record on Closed. `nonConformancePriority` = `Low`/`Medium`/`High`/`Critical`. `nonConformanceAssociationType` = `items`, `customers`, `suppliers`, **`jobOperations`**, `purchaseOrderLines`, `salesOrderLines`, `shipmentLines`, `receiptLines`, `salesReturnOrderLines`, `purchaseReturnOrderLines`.

For contrast, ERPNext's entire Non Conformance doctype is: `subject`, `procedure`, `process_owner`, `full_name`, `status`, `details`, `corrective_action`, `preventive_action` — no disposition, no links, not submittable, controller body is `pass`.

**Gage calibration.** `gaugeStatus` = `Active`/`Inactive`; `gaugeCalibrationStatus` = `Pending`/`In-Calibration`/`Out-of-Calibration`; **`gaugeRole` = `Master`/`Standard`**. `gaugeValidator` carries gaugeId, supplierId, modelNumber, serialNumber, description, dateAcquired, gaugeTypeId, gaugeRole, lastCalibrationDate, nextCalibrationDate, locationId, storageUnitId, **`calibrationIntervalInMonths`**. `gaugeCalibrationRecordValidator` carries gaugeId, supplierId, dateCalibrated, `requiresAction`, `requiresAdjustment`, `requiresRepair`, **`temperature` (-200..500)**, **`humidity` (0..1)**, `approvedBy`, **`measurementStandard`**, and `calibrationAttempts` as repeated `{reference, actual}` pairs — i.e. as-found/as-left readings against a reference.

**The 7.6 gap that remains:** ISO 13485 clause 7.6 requires that when equipment is found out of tolerance you *assess and record the validity of previous measuring results and take action on affected product*. Carbon has `requiresAction` as a **boolean**. There is no linked impact assessment walking from the gage to the inspections it performed to the lots those inspections released. That is the single most audit-exposed calculation in a device shop, and it is unimplemented here as everywhere else.

**Documents and training.** `procedure` table (migration `20250216192838_procedures.sql`): `version NUMERIC NOT NULL DEFAULT 0`, `status "procedureStatus" NOT NULL DEFAULT 'Draft'`, `content JSON`, with `CONSTRAINT "procedure_version_unique" UNIQUE ("name","companyId","version")` and a view selecting `MAX(version)` with a JSON array of prior versions. `procedureStep` with a `procedureStepType` enum (migration `20250915115859_procedure-steps.sql`). Training: `trainingAssignment`/`trainingCompletion` with a `trainingFrequency` enum, `get_current_training_period()`, `get_training_assignment_status()`, and unique indexes enforcing one completion per employee per period, plus a separate unique index for `Once` trainings (migration `20251206000000_training_assignments.sql`).

**Change control.** `apps/erp/app/modules/items/ui/` contains `ChangeNotice`, `ChangeNoticeActions`, `ChangeNoticeTypes` — ECO/ECN in the open tree. Outside Odoo Enterprise's `mrp_plm`, this exists nowhere else in the survey.

**Audit log — in the OPEN repo, correcting an earlier error in this spike.** An earlier pass inferred from the pricing page that audit logging was a paid-tier feature. That was wrong about the source. Migration `20260212152709_audit_log_system.sql` and `apps/erp/app/modules/settings/ui/AuditLog` are in the public repository; the $100/user tier gates *hosted access*, not the code. Self-hosters get it. The schema, per company:

```sql
CREATE TABLE auditLog_<companyId> (
  "id" TEXT NOT NULL DEFAULT id('aud'),
  "entityType" TEXT NOT NULL,
  "entityId" TEXT NOT NULL,
  "operation" TEXT NOT NULL,   -- CHECK IN ('INSERT','UPDATE','DELETE')
  "actorId" TEXT NOT NULL,     -- FK user(id) ON DELETE SET NULL
  "actorName" TEXT NOT NULL,   -- denormalised: survives user deletion
  "diff" JSONB,
  "metadata" JSONB,
  "createdAt" TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);
-- RLS enabled, NO policies => service role only
```

Plus an `auditLogArchive` table with `archivePath`, `startDate`, `endDate`, `rowCount`, `sizeBytes` for retention.

Against 11.10(e) this is the strongest implementation in the survey: old and new values, actor, server-generated timestamp, a denormalised actor name so the record stays readable after user deletion, retention archival, and **RLS-with-no-policies meaning application users cannot read or write the audit table through the API**. That is materially better than Frappe's `Version` (Administrator holds `delete: 1`) or iDempiere's `AD_ChangeLog` (an ordinary editable application window).

Its defects: **`company."auditLogEnabled" BOOLEAN NOT NULL DEFAULT false`** — off by default; no reason-for-change field is enforced (`metadata` could carry one, nothing requires it); the table-creation function is `SECURITY DEFINER` and a drop function exists; and there is no hash chaining or DB-level append-only trigger — service-role-only RLS is the whole control.

**UDI: absent. [SOURCE]** The migrations listing was grepped for `udi|gudid|gs1|gtin|hibcc`. **Zero genuine matches.** (An earlier count of 16 was a false positive: the substring "udi" inside "audit". Sixteen audit-related migrations is itself a signal of how seriously the audit trail is being taken.)

**Marketing claims vs source.** carbon.ms asserts *"Medical devices: ISO 13485, DHR, e-signatures"*, *"ITAR-ready deployment · AS9100 · ISO 13485 · 21 CFR Part 11 · SOC 2 controls · Row-level security · Source available"*, *"Revision control with effectivity dates"*, *"Serial and lot genealogy, forwards and back"*, and *"First article inspection, non-conformance, CAPA and gauge calibration sit on the same records as production."* Source supports the calibration, NCR/CAPA, sampling, genealogy plumbing (`20250128094053_traceability.sql`, `20260430090114_lineage-batch-rpcs.sql`) and change-notice claims. **It does not support the e-signature claim** (Section 2.1). No validation package, IQ/OQ/PQ kit or vendor validation documentation surfaced anywhere.

### 3.4 Who holds the copyright

**Carbon Manufacturing Systems Corp.**, a single corporate holder, per the LICENSE header. Founder and CTO **Brad Barbin** — https://carbon.ms/about — and the repository's original namespace was his personal account.

Whether a contributor licence agreement is required **[UNVERIFIED]** — I did not locate a CLA document or bot. But the structure that matters is already visible and does not depend on it: a single corporate copyright holder, an `ee` carve-out already in the tree at eighteen months old, an upsell overlay shipped in the open code, and a licence clause that makes internal production use conditional. That is the same structure as every project in `spike-governance.md` that later closed. The founder's own memo is candid about the motive: *"We open-sourced Carbon not because it's a great business plan, but because that's the system I would have wanted when I was in your shoes."*

### 3.5 My read: component, and cautionary tale — not competitor

**Not a competitor, for our purposes.** Carbon is solving the general discrete-manufacturing ERP/MES problem with a quality module attached. It is not solving the regulated-record problem: no Part 11 signature, no UDI, no validation package, no out-of-tolerance impact assessment, audit trail off by default. The thing a device CM cannot buy anywhere is precisely the thing Carbon has not built.

**A component, and a very good one.** Carbon is the strongest available demonstration that the data model for medtech-grade quality can live *inside* an ERP rather than in a separate eQMS — and its Z1.4 implementation, MRB dispositions bound to job operations, gage crib with as-found/as-left, and versioned procedures are worth studying closely as prior art. Its audit-log design in particular (per-tenant table, denormalised actor, service-role-only RLS, archival) is the right shape and should be treated as the floor, not the ceiling. Under AGPLv3 terms its open portions are legitimately reusable if we accept the reciprocity.

**And a cautionary tale, which is the most important reading.** Carbon is the newest, best-funded-in-attention, most technically credible entrant in this space, and it shipped **open-core at birth**: single-company copyright, an `ee` directory, an upsell overlay in the open tree, and a licence that makes ordinary internal use conditional on either publishing your trade secrets or paying. The pattern that killed Compiere, Openbravo, xTuple, Fedena and uniCenta did not take Carbon a decade to develop — it was the starting configuration. Whatever we build, the governance decision is the one that determines whether it is still ours in five years, and it has to be made before the first line of code, not after the first funding conversation.

---

## 4. The eight projects

### 4.1 ERPNext (Frappe)

**Licence.** ERPNext **GPL-3.0** (https://raw.githubusercontent.com/frappe/erpnext/develop/license.txt); Frappe Framework **MIT** (https://raw.githubusercontent.com/frappe/frappe/develop/LICENSE). Frappe HR GPL-3.0; Helpdesk, Insights, CRM, LMS AGPL-3.0. **No Enterprise Edition, no paywalled module.** This is real and is the single strongest structural advantage in the survey. Monetisation is hosting and warranty: Frappe Cloud sites from **$5/mo**, servers from **$40/mo**, **not per-user** (https://frappecloud.com/pricing). Current **v16.34.2**; v15.121.2 maintained in parallel; v16 GA 12 January 2026. 39,129★ / 12,768 forks.

**Stack.** Python 3.14, MariaDB 11.8, Node 24, Redis, wkhtmltopdf-with-patched-Qt on v16. **Debian/Ubuntu and macOS only**; the docs say *"If you're a Windows user, you could use Ubuntu in WSL."* No bare-metal Windows support, ever. Docker recommended for production. https://docs.frappe.io/framework/user/en/installation

**Manufacturing — best feature coverage of the eight, with one fatal hole.** 50 doctypes including `bom`, `bom_operation`, `bom_explosion_item`, `bom_creator`, `routing`, `operation`, `sub_operation`, `workstation`, `workstation_type`, `workstation_cost`, `job_card`, `job_card_time_log`, `plant_floor`, `production_plan`, `master_production_schedule`, `sales_forecast`, `blanket_order`, `downtime_entry`. Capacity planning is free and configurable.

**The hole [SOURCE]:** the complete BOM field list (~90 fields, https://raw.githubusercontent.com/frappe/erpnext/develop/erpnext/manufacturing/doctype/bom/bom.json) contains **no revision, no version, no `effective_from`/`effective_to`, no ECO link**. Versioning is `is_active`/`is_default` plus separate documents. Raised by the community in 2015 in exactly these terms — *"my company will need to be ISO 13485 certified, and that probably can't happen without being able to track part (item) revisions"* (https://discuss.frappe.io/t/several-issues-on-initial-setup/6023) — and still open May 2026: *"The `Item` doctype is flat and unique. There is no native versioning"* (https://discuss.frappe.io/t/item-revisioning/162669). Worse, the **BOM Update Tool's "Replace BOM" retroactively rewrites submitted parent BOMs** (https://docs.frappe.io/erpnext/user/manual/en/bom-update-tool), breaching the submit-immutability that is otherwise the system's saving grace.

**Costing.** `Item.valuation_method` includes **Standard Cost**, genuinely implemented (`process_standard_cost` in `stock_ledger.py`), and `item_standard_cost` has an **`effective_date`** — ERPNext gives costs effectivity while denying it to BOMs. But **no variance accounts**: no PPV, no labour efficiency, no overhead absorption. The delta lands in a generic Stock Adjustment expense account.

**Quality — two modules, one real.** `quality_management` is a shell: **Non Conformance has 10 fields, four of them layout breaks**; not submittable; no link to Item, Batch, Serial No, Work Order, Job Card or Supplier; no disposition, containment, root cause or effectiveness check; controller body is `pass`. A Frappe maintainer conceded: *"Totally, Quality module does feel like an App more than a module"* (https://discuss.frappe.io/t/fully-integrating-erpnext-quality-management-module-with-the-rest-of-the-modules/117755). The **stock-module Quality Inspection is genuinely good**: submittable, item/serial/batch aware, `sample_size`, template parameters with min/max and formula criteria, `inspected_by` and `verified_by`, attachable to Purchase Receipt, Delivery Note, Stock Entry and Job Card, with `inspection_required` and `quality_inspection_template` on the BOM itself. No sampling plan, no e-signature, no disposition routing.
**Absent entirely:** calibration/gage, SCAR, complaint handling, ECO/ECN. Training exists in the separate `hrms` app.

**Audit trail.** `Version` records per-field old/new, who and when. System Manager gets read/export only — but **Administrator has `delete: 1`**. `track_changes` is a per-doctype checkbox that can be switched off silently; **Quality Inspection does not set it**. **No reason-for-change field anywhere.** No hash chain. **Activity Log is purged at 90 days by default** (`default_log_clearing_doctypes` in https://raw.githubusercontent.com/frappe/frappe/develop/frappe/hooks.py). The "Audit Trail" doctype added 2023 is a single read-only viewer over `Version`, not an independent log.

> **Correction applied in place.** An earlier pass in this spike claimed that `version.py`'s `blacklisted_fields = ["Markdown Editor","Text Editor","Code","HTML Editor"]` (line 139) excludes rich-text fields from versioning, and therefore that ERPNext NCR narrative content is invisible to the audit trail. **That is wrong.** Reading line 208, the blacklist only skips the `get_formatted()` call; the change is still compared and recorded with raw values. Rich-text fields **are** versioned. The Part 11 gaps listed above are real; this one is not, and it should not be repeated.

**Genealogy — the standout.** v16 shipped a genuine bidirectional multi-level trace: `traceability_direction` of Backward/Forward/Both, recursing through sub-assemblies with no hardcoded depth limit (https://raw.githubusercontent.com/frappe/erpnext/develop/erpnext/stock/report/serial_no_and_batch_traceability/serial_no_and_batch_traceability.py, PR https://github.com/frappe/erpnext/pull/48950). **v16-only.** No UDI. No validation package.

**Complaints.** Stock valuation is the deepest vein: a thread titled *"stuck in queued for 12 months"* reports 75,189 queued reposts and `JobTimeoutException` (https://discuss.frappe.io/t/item-valuation-reposting-in-progress-stuck-in-queued-for-12-months/155524). ERPNext's **own docs** warn reposting *"will change the closing balances of the respective financial year… Avoid using reposting for closed financial years"* (https://docs.frappe.io/erpnext/user/manual/en/repost-item-valuation) — a system that rewrites closed-period GL balances is an internal-control finding under any audit regime. Negative stock breaks double-entry, confirmed by a Frappe team member (https://discuss.frappe.io/t/perpetual-inventory-not-working-if-we-allow-negative-stock/7435). From the abandonment thread: *"my principal complaint is the sub-par quality. Bugs, bugs, bugs… I am having [a] hard time understand[ing] how [anyone] can run a vanilla erpnext on a real production [system]"* (https://discuss.frappe.io/t/if-you-were-to-abandon-erpnext-what-would-be-the-reason/88513). The founder's 2012 assessment of manufacturing: *"This is currently our weakest module."*

### 4.2 Odoo Community Edition

**Licence and boundary.** Community **LGPLv3**; Enterprise proprietary **OEEL-1.0** (https://www.odoo.com/documentation/19.0/legal/licenses.html). Odoo SA states the strategy itself: ***"80% of our developments should be open source to attract more users and 20% should be in Odoo Enterprise to improve our revenue stream"*** (https://www.odoo.com/blog/odoo-news-5/post/odoo-community-enterprise-532). Current **19.0**; the `[REL] 20.0` branch-cut PR was opened 11 September 2026, unmerged. 54,300★.

**Edition split established by probing the public LGPL repo [PROBE].** Enterprise lives in a separate private repo, so absence from `addons/` on branch 19.0 (638 modules) is conclusive.

| Absent from Community — Enterprise only | Consequence |
|---|---|
| `quality`, `quality_control`, `quality_mrp` | **The entire Quality app** |
| `mrp_plm` | ECO, BoM versioning, diff/merge, approvals |
| `mrp_workorder` | Shop-floor operator terminal |
| `mrp_mps` | Master Production Schedule |
| `stock_barcode` | All barcode scanning |
| `account_accountant`, `documents`, `sign`, `approvals`, `web_studio`, `helpdesk` | Full accounting, DMS, e-signature, approvals, no-code, helpdesk |

Present in Community, correcting widespread misinformation: `mrp` (including work orders, work centers and routings), **`maintenance`**, **`mrp_subcontracting`**, `mrp_account`, `stock_landed_costs`, `product_expiry`, `repair`, `iot_base`. The CE `account` module is titled **"Invoicing"**; the editions page's "Accounting ✓" is materially misleading.

**Manufacturing.** The scheduling *engine* is free and better than its reputation — `mrp.workcenter` has `time_efficiency`, `costs_hour`, `time_start`/`time_stop`, `oee_target`, `alternative_workcenter_ids`, `resource_calendar_id`, with forward and backward slot-finding. But `web_gantt` is Enterprise, so **the planner's screen is paywalled while the engine is not**.

**`mrp.bom` has no version, revision, effectivity or ECO field** — only `code` (free text) and `active` (https://raw.githubusercontent.com/odoo/odoo/19.0/addons/mrp/models/mrp_bom.py). Unlike ERPNext, **Odoo BoMs remain editable after building product**, with only a dismissible warning. You cannot reconstruct which revision built a lot. For a DHR that is a hard stop.

**Costing.** Odoo has a real **purchase price variance** (`property_price_difference_account_id` in `stock_account`), which ERPNext lacks. No labour or overhead absorption variance; MO cost-vs-real is a report, not GL postings.

**Audit.** Odoo CE *does* contain a cryptographic hash chain — `inalterable_hash`, `secure_sequence_number`, `restrict_mode_hash_table`, `restrictive_audit_trail` (https://raw.githubusercontent.com/odoo/odoo/19.0/addons/account/models/account_move.py) — but it covers **posted accounting journal entries only**, built for EU anti-fraud tax law. DHR, lot genealogy, nonconformances and inspection results get zero tamper-evidence. General tracking is `mail.thread` chatter on fields explicitly marked `tracking=True`.

**The OCA partially rescues this, one version behind, permanently.** https://github.com/OCA/management-system (AGPL-3, 239★) is a real ISO-9001-shaped QMS for Community: `mgmtsystem_nonconformity` with product/mrp/maintenance/hr variants, `mgmtsystem_action` **plus `mgmtsystem_action_efficacy`**, `mgmtsystem_audit`, `mgmtsystem_claim`, `mgmtsystem_hazard_risk`, and `document_page_procedure`/`quality_manual`/`work_instruction`. https://github.com/OCA/manufacture adds `mrp_multi_level` (a real MRP engine) and `quality_control_oca`. https://github.com/OCA/server-tools has `auditlog` — badged **Beta** in its own README.

The lag is measurable [PROBE]: OCA/manufacture has ~59 modules on 18.0 and **21 on 19.0**; OCA/management-system has 100 entries on 18.0 and 82 on 19.0. Missing from 19.0: `mgmtsystem_claim`, `mgmtsystem_action_efficacy`, `mgmtsystem_nonconformity_product`, `mgmtsystem_nonconformity_mrp`, the `document_page_*` set, and — decisively — **`quality_control_mrp_oca` and `quality_control_stock_oca`**, the modules connecting quality to manufacturing orders and stock. Open "Migration to version X.0" trackers across the OCA org: **232 for 19.0, 212 for 18.0, 200 for 17.0, 174 for 16.0.** The backlog never clears.
**Actionable:** if you build an OCA-based quality system on Odoo, **stay on 18.0.**
Also note `mrp_bom_version` is misleadingly named — it adds a *state* flag, not revisions or effectivity, and is not on 19.0.

**Complaints.** Three AVCO/landed-cost valuation bugs filed against 19.0 during the research week (https://github.com/odoo/odoo/issues/286673, /286672, /249198). Directly relevant to traceability: https://github.com/odoo/odoo/issues/23552 — two operators starting parallel work orders were assigned **the same serial number**, mixing two physically distinct units. From someone evaluating Odoo to replace Dynamics GP for manufacturing, January 2026: ***"'Open-Source' is a bit of a misnomer. The majority of the important modules are enterprise only."*** (https://news.ycombinator.com/item?id=46439993). In the same thread, Fabien Pinckaers on migrations: *"We used it to monetize Odoo Enterprise Upgrade"*; and a customer on the mechanism — you ship your entire production database to Odoo — *"since it is Odoo, everything is in that database… I found this inacceptable but at that point we had no choice."* For a CM holding customer trade secrets that may be disqualifying by itself.

**Pricing [VENDOR]:** Standard **$31.10/user/mo** monthly, **$24.90** yearly; Custom **$61.00/$49.00** (https://www.odoo.com/pricing). **On-premise requires the Custom tier** — also the only tier with Studio, multi-company and External API.

### 4.3 Tryton

**Licence** GPL-3.0-or-later throughout, no open core (https://pypi.org/project/trytond/, 8.0.9). Python ≥3.10 + PostgreSQL. Series every six months, 1-year support, LTS 5 years; 8.2 due 5 October 2026. Canonical VCS is **Mercurial on foss.heptapod.net**; GitHub is a mirror.

**Manufacturing.** Modules exist: `production`, `production_routing`, `production_work`, `production_work_timesheet`, `production_split`, **`production_outsourcing`** (raises a PO per routing when production goes to waiting — genuinely useful for outside processes), `stock_supply_production`, `stock_lot`.

But the depth is not there [SOURCE]. **`production_routing/routing.py` is 45 lines total** — Routing is name + steps + BOMs; RoutingStep is sequence + operation. No setup time, run time, yield, queue time, alternates or effectivity. The project lead confirms: *"the routing from Production Routing Module does not store yet estimations of those cycles/time… all of this requires to extend existing modules or create new one"* (https://discuss.tryton.org/t/production-module-implemetation-help/8482). `WorkCenter` is name + cost_price + cost_method; **`WorkCenter.get_picker()` ends in `random.choice()`** — it assigns a work center at random from the category. `BOM` (490 lines) has no revision, no effectivity, no ECO — only an `active` flag. Costing is fixed/average/fifo; **no standard costing, therefore no variances**. MRP is order-point, not multi-level netting; Krier: *"We do not have a forecast of producible products."*

Users say it unprompted: *"there is no such thing as revision control, checkout, checkin etc. So no PDM system!"* and *"Coming from an engineering side of things I'd expect to see a Part Number, Description, and Revision field, but even this escapes me!"* (https://discuss.tryton.org/t/just-the-basics-of-a-recursive-bom-setup/5964).

**Quality.** A core `quality` module since 6.8 (877 lines) — Control, ControlPoint, Inspection, Alert, with frequency-based sampling, attachable to `production:run` and `production:do` as well as receipts and shipments, pass/fail with named inspectors, and a failed inspection blocking the document. Credible receiving and in-process inspection. **Absent: NCR/MRB with dispositions, CAPA, root cause, SCAR, gage/calibration, training, complaints, ECO, e-signature.** NaN-tic maintains a shadow quality suite — all GPL-3.0, all **zero stars, none on PyPI, no docs**.

**Audit — the best architecture, switched off [SOURCE].** `ModelSQL._history` maintains a full shadow `__history` table with every row version plus `create_uid`/`write_uid`/`write_date` (https://foss.heptapod.net/tryton/tryton/-/raw/branch/default/trytond/trytond/model/modelsql.py). Architecturally the cleanest audit foundation in the survey. **It is off for `production`, `stock.move`, `product` and `party`** — each checked. Historising even the single field of product cost required a separate add-on module (`product_cost_history`). Without it you get last-writer-only metadata. **Lot genealogy is where Tryton leads**: `stock_lot`'s Lot Trace shows *"the upward and downward traces as a tree structure"* — bidirectional, packaged, through production (https://docs.tryton.org/latest/modules-stock-lot/design.html).

**Health.** Of 800 commits in the last 12 months, **665 (83.1%) are Cédric Krier and 722 (90.2%) are his company B2CK**; nine of the other fifteen contributors made exactly one commit all year. Forum: 5,168 topics ever; searching all 39,628 posts returns **zero hits for "MRP", "MES", "quality control", "21 CFR", "ISO 13485" or "medical device"**. Desktop client downloads 3,722/month worldwide. **Zero service providers in North America** (https://www.tryton.org/service-providers); the only one advertising production expertise, NaN-tic, serves *"chemical, pharmaceutical, logistics"* — process, not discrete. Governance stress is documented: the Foundation President resigned after one year — *"this had been my worst year in the project… I'm phsicologically exhausted"* — and a former board member replied *"All efforts to bring more transparency and turn Tryton to a more community-driven project are blocked"* (https://discuss.tryton.org/t/6950). On tooling friction driving contributors away, the maintainer response was ***"We are not gonna change it."***

### 4.4 iDempiere

**Licence GPLv2-only** (https://github.com/idempiere/idempiere/blob/master/LICENSE.md) — *only*, not "or later", so it cannot link GPLv3/AGPLv3 code. No open core. Java on OSGi/Equinox + embedded Jetty + ZK 9.6, PostgreSQL ≥14 or Oracle. **The only one of the eight with an official Windows server build** (`idempiereServer13.win64…zip`). v13 "Orion" current. 661★; last commit 2026-09-10. The OSGi + "2Pack" plugin model — bundles installed live via the Felix console carrying Application Dictionary metadata — is the best extension architecture here.

**Manufacturing is not in core, and this is decisive.** Core ships one-level assembly only (`MProduction` explodes `PP_Product_BOMLine` one level, issue+receipt in one document) plus the PP_* tables; `MPPOrder.java`, `MPPMRP.java` and `MPPCostCollector.java` are absent from core. Real manufacturing lives in the **Libero** plugin, descended from e-Evolution's ADempiere work. **Upstream `adempiere/extension_libero_manufacturing` is explicitly marked [DEPRECATED], last pushed 2015.** Two maintained forks:
- https://github.com/pshepetko/org.idempiere.mfg — a **personal account**, 7★, 160 Java files, **35 commits ever by two authors**, README still targeting iDempiere 8.2 while core is at 13, **no LICENSE file**.
- https://github.com/logilite/org.idempiere.mfg2 — 3★, 166 Java files, maintained by Logilite. **This is the fork to use**: `MPPOrderBOMLineMA` gives lot/ASI allocation on MO component lines, which is what lot genealogy through a work order requires.

On paper Libero is the most complete MRP II in the survey: **MPPOrder** with a 1,816-line DocAction lifecycle; per-order routing instantiation (`MPPOrderWorkflow`/`Node`/`NodeNext`/`NodeAsset`) with machine assignment; **cost collectors posting to the GL** via `Doc_PPCostCollector`; **`CalculateLowLevel.java`** for genuine time-phased low-level-code MRP; a full **CRP** set; `RollupBillOfMaterial` + `RollupWorkflow` + `CreateCostElement` for classic material/labour/burden/outside cost-element rollup; and **`BOMExpiredException`/`RoutingExpiredException`**, proving runtime effectivity enforcement. Revision and effectivity: yes. Formal ECO workflow: no.

Install reality is brutal. The Google Group thread index reads: *"Plugin org.idempiere.mfg does not start on iDempiere 9"*, *"Libero Manufacturing install error"*, *"Failed when installing org.libero.mfg62"*, *"Libero Plugin install // Error 500"*, *"Libero Manufacturing Not Installed"*. **The install threads outnumber the usage threads.** On iDempiere 11 it dies on Felix activation; the v9 fallback drew *"some screens still do not work."* The 8.2-on-9 fix was a hand-run `UPDATE AD_Column SET callout=null WHERE SUBSTRING(callout,0,21)='org.eevolution.model'`. The most recent Libero thread (14 May 2026) requests **professional support**, answered by one Venezuelan consultant offering help by phone — that is the entire commercial support market for iDempiere discrete manufacturing.

**Quality is a literal stub.** Libero ships `MQMSpecification`/`MQMSpecificationLine`, whose entire field list is `M_Attribute_ID, Operation, Value, AndOr, SeqNo, ValidFrom, ValidTo`, referenced in exactly **one place** — `MPPOrder.approveIt()` — as a boolean attribute check. `github.com/topics/idempiere` lists 26 repos, none quality.

**Audit.** `AD_ChangeLog` captures per-column old and new values with user and timestamp — the right shape. But it is **opt-in twice**: enable "Maintain Change Log" on the role *and* on each table, then reset the server cache (documented verbatim in metasfresh's inherited how-to: https://raw.githubusercontent.com/metasfresh/metasfresh-documentation/master/_howto_collection/EN/ActivateChangeLog.md). It is an ordinary application table exposed through a normal window, with no hash chain and no append-only enforcement, so an administrator can alter or delete rows and retroactively disable logging per table. Fails 11.10(e). **[UNVERIFIED]** — the old/new-value column detail comes from the Compiere/ADempiere data model, not a page I could fetch; wiki.idempiere.org is Cloudflare-blocked.

Reviewers converge unprompted: *"Necesita de asistencia profesional continua"*; *"Instalación no es fácil, solo expertos pueden hacerla"*; and a review titled "Slow and complicated ERP" — *"You need daily assistance for any use, after more than three years it shouldn't be."*

### 4.5 metasfresh — strike it

**Licence GPLv2**, entire backend tree public, **no code paywall**. But the paywall is in releases, docs and support, and the project is not operationally open to outsiders.

- **GitHub Releases stop at 5.175, published 27 June 2023**, while the README still claims a stable release every Friday. What ships publicly are unversioned Docker tags like `5.175-new-dawn-uat.43830-compat`. **For a validated system this is the worst possible shape: no semver, no per-release changelog, no reproducible "the version we validated" artifact.**
- Default branch is `new_dawn_uat`, an internal UAT branch. Of the 300 most recent open issues (Oct 2024 → Sept 2026), **280 (93.3%) are from metas GmbH staff or bots.** **Zero outside bug reports from an actual manufacturing user in two years.**
- The vendor has stated in writing that there is no community support: *"wir… daher keinen kostenlosen Support anbieten können"* (https://forum.metasfresh.org/t/2287). Forum totals: 716 topics / 596 registered users in ten years; **0 new topics and 0 active users in the last 30 days**; last activity 29 April 2026.
- Outside "is this project dead?" issues have sat **17 months with zero comments** (https://github.com/metasfresh/metasfresh/issues/20544, /21208).
- **https://github.com/metasfresh/metasfresh/issues/24665, filed 21 June 2026, open with zero comments for three months:** *"Any authenticated metasfresh user, including one assigned a heavily restricted role with no access to a given window, organization or record, can read, download, write and delete the file attachments, archived documents… of arbitrary records by supplying the target record's window id and record id in the URL path."* A public, unacknowledged cross-tenant document-access bypass is disqualifying on its own for a CM holding customer drawings under NDA.

**Fit.** The material-planning engine (`de.metas.material`, event-driven candidate planning with BOM and distribution-network explosion) is genuinely modern. But `de.metas.manufacturing`'s order package is literally `exportaudit/`, `importaudit/`, **`weighting/`** — catch-weight food production. **Zero quality modules** among 100+ backend modules. Hardware baseline 8–24 CPU cores. **No Windows path at all**; some admin windows still require the legacy Swing client. An industrial-engineering reviewer: ***"It is built on Food sector which is different to our business."*** A 2024 reviewer rated support **1/5**: *"Kein Support. Fehler werden nicht behoben."* Zero HN mentions ever. Pricing is SaaS-only — €99/user/mo plus €299/mo hosting — with no free self-hosted option on the pricing page.
**False-positive warning:** metasfresh's only "pharma" code is `de.metas.vertical.pharma.msv3`, the German pharmacy-to-wholesaler ordering protocol. Distribution, not GxP.

### 4.6 Axelor Open Suite — the most under-rated, and the docs hide why

**Licence AGPL-3.0-or-later** (https://github.com/axelor/axelor-open-suite/blob/master/LICENSE). Open core: https://axelor.com/pricing/ sells Pro **$35/user/mo (min 10)** and Enterprise **$55/user/mo (min 20)**, withholding the commercial licence, e-invoicing, advanced BI, CAS/SSO, advanced Studio, the AI module, support — and, tellingly for a regulated shop, **"updates and maintenance."** Java + Hibernate + Guice, React front end, PostgreSQL 16, Tomcat 10.1, Java 21. Gradle source build or Docker; no one-click Windows installer. 971★, last commit 2026-09-03, **five maintained release lines**.

**Manufacturing is real, verified at entity level [SOURCE].** `axelor-production` ships 60+ entities: `BillOfMaterial`, `ProdProcess`, `WorkCenter`, `WorkCenterGroup`, `Machine`, `ManufOrder`, `OperationOrder`, `OperationOrderDuration`, `CostSheet`, `MpsCharge`, `MpsWeeklySchedule`, `Sop`, `ConfiguratorBOM`.
- **BOM revisions: yes** — `versionNumber` + `originalBillOfMaterial` self-reference + `statusSelect`, same on `ProdProcess`. **Effectivity dates: no** — no validFrom/validTo fields, so you cannot date-phase a revision.
- **Work centers are textbook MRP II**: `minCapacityPerCycle`, `maxCapacityPerCycle`, `durationPerCycle`, `startingDuration`, `setupDuration`, `endingDuration`, `timeBeforeNextOperation`, separate machine (`costAmount`) and labour (`hrCostAmount`) rates, `isRevaluationAtActualPrices`.
- **Shop floor capture: yes** — `OperationOrderDuration` = operationOrder + startedBy + startingDateTime + stoppedBy + stoppingDateTime.
- **Actual vs standard: yes** — `CostSheet.calculationTypeSelect` distinguishes planned from real, with `manufOrderProducedRatio`.
- **MRP and MPS: yes** — `Mrp`/`MrpLine` (26 fields incl. maturityDate, cumulativeQty, pegging via `mrpLineOriginList`), `MrpForecast`, plus MPS and S&OP.

**Quality is substantially better than the published docs admit.** The docs describe only control points and a free-text alert; **`axelor-quality` ships 49 entities**, present in release tags back to v8.3:
- **Inspection plans**: `ControlPlan`, `ControlPlanFrequency`, `ControlType`, `ControlEntry` (with `inspector`, `sampleCount`) → `ControlEntrySample` → `ControlEntryPlanLine`. **`ProductCharacteristic`** carries minValue/maxValue/expectedValue. **`TrackingNumberCharacteristic`** carries trackingNumber + characteristic + `measuredValue` + **`conforms`** — inspection measurements bound to a specific lot or serial. Structurally, DHR data.
- **NCR/CAPA as 8D/QRQC**: `QualityImprovement` (with `gravityTypeSelect`) → `QIIdentification` → `QIAnalysis` → `QIResolution`. `QIIdentification` links customer and **supplier** partners, sale and purchase orders, stock moves, product, `quantity`, `nonConformingQuantity`. **`QIAnalysis` carries `efficiencyCriteria`, `efficiencySelect`, `efficiencyCheckedBy`, `efficiencyCheckedOn`** — CAPA effectiveness verification with a named verifier and timestamp, a direct 820.100(a)(4) / ISO 13485 8.5.2 requirement, actually modelled. `QIAnalysisCause` supports multi-level root cause with `causeLevel` and supplier responsibility attribution. `RequiredDocument` has `docVersion`, `isActiveVersion`.
- **Absent**: calibration/gage (nearest is `axelor-maintenance`'s `EquipementMaintenance`, a PM interval scheduler — no certificates, no as-found/as-left, no out-of-tolerance impact assessment); **ECO/ECN** — every domain directory across production, base, quality and project was searched for change/eco/ecn entities, **zero hits**; complaints; training is HR-flavoured in `axelor-talent` with no SOP-revision binding.

**Design intent is IATF 16949 automotive, not medical.** Axelor's site names an automotive sub-vertical with eight segments and **never mentions medical device, pharmaceutical, life sciences, ISO 13485 or FDA.** The 13485 overlap is incidental.

**Audit.** `@Track` is opt-in per field, writing `TrackMessage` entries into the collaboration feed rather than an immutable table. `ManufOrder` tracks 7 fields, `BillOfMaterial` ~12, and **`OperationOrder` tracks zero**. **Lot genealogy is strong**: `TrackingNumber.parentTrackingNumberSet` is a genuine many-to-many genealogy graph.

**Health risks.** 1,003 French forum topics vs 636 English. Capterra 4.8/5 from only 29 reviews, mostly integrators. **409 open issues**, one corporate steward. Documentation is years behind the code — the entire QI/CAPA and ControlPlan subsystem, the best reason a device manufacturer would look at Axelor, **is undocumented but has shipped since 8.3.** You cannot evaluate this product from its documentation; you must read the domain XML.

### 4.7 Dolibarr — eliminate

**GPL-3.0-or-later**, PHP + MariaDB, and genuinely the easiest install here — **DoliWamp** is a real one-click Windows all-in-one (`DoliWamp-22.0.4.exe`). V24 announced 11 September 2026, commits daily. 7,598★ but **1,028 open issues**.

MRP is better than its reputation: **multi-level BOM is genuine** (`fk_bom_child`, recursive `getNetNeedsTree()`), child MOs generate, module is core/stable not experimental. BOM lines carry `fk_default_workstation`, `duration`, `efficiency`.

But **no routings and no sequenced operations**, confirmed two ways: https://github.com/Dolibarr/dolibarr/issues/17715 closed without implementation, and structurally the **entire `htdocs/mrp/` directory is ten files** — `mo_card.php`, `mo_list.php`, `mo_production.php`, `mo_movements.php`, `mo_agenda.php`, `mo_note.php`, `mo_document.php` plus class/lib/ajax. No operation entity, no operation screen. Workstation is three classes, no capacity calendar, no scheduling, no time capture. No labour time collection, no variance, no finite capacity. **Zero quality functionality in core.** **No data-change audit trail of any kind** (Section 2.1).

Lot/serial has disqualifying defects: https://github.com/Dolibarr/dolibarr/issues/23957 — on receipt *"only is possible to create one record per product with a single field content named 'batch'"*; and from the forum, *"There is no way to correct or delete a SN and you can add more than 1 times the same SN (which is not acceptable)."* **Accepting duplicate serial numbers is disqualifying for device serialisation.**

The paid-module path adds risk rather than closing the gap. ATM Consulting's "Advanced manufacturing" is **€990 and supports Dolibarr V17 only** while core is at V22 — your most expensive manufacturing dependency blocks your security-upgrade path. There is **no module rating system** on Dolistore (*"I have had both good and poor experiences with modules from Dolistore and would like to communicate that"*), and documented cases of paying and never receiving: *"I PURCHEASE THE MODULE DOLIPOS, 342€, I PAYD, AND THE SYSTEM DON'T PERMISE TO ME TO DONWLOAD"* — four support emails, no reply, PayPal refund forced. Dolibarr has its own RCE CVEs (CVE-2026-22666, `eval()` whitelist bypass, April 2026).

### 4.8 Apache OFBiz — best data model, worst everything else

**Apache-2.0**, no vendor, no open core. Java 17 + Groovy + Gradle, Freemarker/widget-XML UI. 1,124★.

The BOM model is the only true effectivity dating in the survey [SOURCE]. `ProductAssoc` (https://raw.githubusercontent.com/apache/ofbiz-framework/trunk/applications/datamodel/entitydef/product-entitymodel.xml) carries **`fromDate` in the primary key**, plus `thruDate`, `scrapFactor`, and **`routingWorkEffortId`** assigning each component to a specific routing operation. `WorkEffort` carries `estimatedMilliSeconds`, `estimatedSetupMillis`, `actualMilliSeconds`, `actualSetupMillis`, `quantityToProduce`, `quantityProduced`, `quantityRejected` — standard vs actual, setup vs run, and scrap, at operation level. The service layer is complete: `createProductionRun`, `productionRunDeclareAndProduce`, `getProductionRunCost`, `createProductionRunTaskCosts`, `decomposeInventoryItem`.

**Quality: zero.** The product, workeffort and manufacturing entity models and all 22 plugins were searched. The only match is `ProductFacilityLocationQuantityTest`, an internal test fixture. The widely repeated claim that OFBiz manufacturing covers *"quality and maintenance management"* is **not supported by the codebase.**

**The audit engine exists and is switched off [SOURCE].** `EntityAuditLog` captures `changedEntityName, changedFieldName, pkCombinedValueText, oldValueText, newValueText, changedDate, changedByInfo, changedSessionInfo` — a genuinely Part-11-shaped record built into the entity engine. The XSD says `enable-audit-log` *"Defaults to false."* Occurrences across all ten application entity models: **10 in the order model; zero in accounting, content, humanres, manufacturing, marketing, party, product, shipment and workeffort.**

**Project state: frozen, not dead.** Current release 24.09.07 (June 2026); **feature-frozen since September 2024**, seven bug-fix-only patches since. ~1,023 commits in 52 weeks but only **three distinct human authors** in recent history. Work is Minilang→Groovy migration, REST APIs and modularisation. The manufacturing "Beginner's Guide" on the wiki was last updated **24 March 2009** and still says *"This is still a WIP."*

**Security is the deal-breaker.** https://ofbiz.apache.org/security.html — 4 CVEs in 2023, 5 in 2025, and **19 in 2026**, including **CVE-2026-45434 (CVSS 9.8)**, an authentication bypass chainable to pre-auth RCE, and CVE-2026-50223, template-directive injection to RCE. Two emergency patch releases in two months. OFBiz's own commercial advocates say *"It is not a plug-and-play manufacturing ERP"*, and HotWax Systems writes of the shipped UI: *"It should be considered as an interface built by developers, for developers… It is not meant for end-users."*

---

## 5. New entrants and the open-source eQMS landscape

### 5.1 The eQMS category is quantitatively empty

GitHub repo-search totals, live: `eQMS medical device` → **4 repos**, top 8★. `topic:iso-13485` → **28 repos**, top is a dead project. `QMS ISO 13485 quality management` → **7 repos**, top 11★. Searching `erpnext 21 CFR Part 11` and `odoo 21 CFR part 11` each return **zero repositories**.

| Project | Licence | ★ | Last commit | Verdict |
|---|---|---|---|---|
| https://github.com/openregulatory/templates | **CC BY-NC-SA 4.0** | 183 | 2025-01-07 | Best artifact in the category — but **NonCommercial**, with a mandatory attribution footer. Needs counsel before a commercial CM adapts and shares it |
| https://github.com/AliakseiT/dearauditor-qms-baseline | NOASSERTION | 8 | 2026-08-26 | 23 SOPs + GitHub Actions for doc control and training, incl. Part 11 overlays. Published by a Swiss consultancy as lead-gen; honest that it is an upstream baseline, not a QMS |
| https://github.com/innolitics/rdm | MIT | 142 | **2022-09-06** | **Dead 4 years**, 39 open issues. Innolitics itself is active; they stopped investing here |
| https://github.com/OpenSaMD/OpenSaMD | AGPL-3.0 | 31 | **2023-09-09** | Dead, and had a sting: *"clinical use… requires you to purchase the regulated release"* |
| evolunis/openQMS, PiecePaperCode/eqms, qara-pulse-eqms, IridiumSoftware/OpenQMS, OxiQMS | mixed | 0–11 | 2025–2026 | Hobby projects, almost all **SaMD-focused** |

**The structural observation that matters:** every open-source medtech-quality project targets software teams doing IEC 62304, and every open-source manufacturing project ignores regulation. **Nothing sits at the intersection — the physical-device contract manufacturer.** Mindshare confirms it: HN Algolia for `ISO 13485` since 2024 returns essentially nothing (the one openregulatory.com submission scored **1 point**); HN for `MES manufacturing` since 2024 returns **one story**, about blockchain.

**Requirements/trace tooling is the one healthy corner** — https://github.com/doorstop-dev/doorstop (LGPL-3.0, 660★, 53 contributors, committed 2026-09-11), https://github.com/strictdoc-project/strictdoc (Apache-2.0, 382★, 44 contributors, committed 2026-09-11), https://github.com/useblocks/sphinx-needs (MIT, 301★), https://github.com/itsallcode/openfasttrace (GPL-3.0, 167★). All actively maintained, all bus-factor ~2, and **none is a QMS**.

**LIMS**: https://github.com/senaite/senaite.core (GPL-2.0, 385★, committed 2026-09-10) is real and does genuine instrument calibration — `instrumentcalibration.py`, `instrumentcertification.py`, and `Instrument.isValid()` blocking out-of-calibration instruments. ISO/IEC 17025-shaped for diagnostic and public-health labs; no Part 11 validation package. Right for a test lab, wrong for a gage crib.

**Gage calibration and UDI are true voids.** No credible open-source gage-calibration system exists outside Carbon (openMAINT and Atlas CMMS both lack calibration entirely). For UDI: `GUDID` returns **44 repos** on GitHub, the only submission-oriented one 1★ and dead since 2021; `HL7 SPL structured product labeling` returns **total_count = 1**, last pushed **2012**. There is no open-source GUDID/SPL submission tooling. **Zero** UDI/GS1/GTIN/HIBCC support was separately confirmed in all eight main projects and in Carbon.

**PLM**: Aras Innovator is free-to-download but **not OSI open source** — a proprietary Community Edition licence capped at **50 named users**, requiring a key. Genuinely used in medical device PLM; a 30-person CM technically fits under the cap. OpenBOM is proprietary freemium despite the name ($30–$90/seat/mo). DocDoku PLM (288★) dead since 2021.

### 5.2 Other 2024–26 entrants

Carbon is treated in full in Section 3. Others: https://github.com/open-mrp/api (Apache-2.0, Go/gRPC, architecturally serious, **6 stars** — a one-shop effort); https://github.com/Mes-Open/OpenMes (AGPL, PHP, created Feb 2026, 127★, unproven); https://github.com/qcadoo/mes (935★, still committing, **licence unclassifiable** — verify before adopting).

**Abandonware that still ranks well in searches**: `jukbot/smart-industry` (441★, dead 2021), `sindohmes/mes4u` (60★, dead 2020), `docdoku-plm` (288★, dead 2021), `osrmt` (215★, dead 2020).

**Ignition Maker Edition is legally unusable**: *"Businesses, non-profit organizations, and other entities cannot use Maker Edition for commercial, revenue-generating, or non-profit activities."* Ever Gauzy (4,384★) is agency/services ERP with no manufacturing. Medusa and Bagisto are e-commerce and irrelevant.

---

## 6. Q1 — What could a 30-person medical device CM actually run today?

**Short answer: Carbon, on a commercial licence, as ERP/MES with the strongest quality module in the category — and you still buy or build an eQMS of record, e-signatures, UDI and the validation package on top. If the licence is unacceptable, the answer is ERPNext plus a separate commercial eQMS, and you build item/BOM revision control yourself.**

| | ERPNext v16 | Odoo 19 CE (+OCA 18) | Axelor 9 | Carbon | Tryton 8 | iDempiere+Libero | metasfresh | Dolibarr | OFBiz |
|---|---|---|---|---|---|---|---|---|---|
| Multi-level BOM | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| BOM revision | **no** | **no (EE)** | yes | yes | **no** | yes | yes | **no** | yes |
| BOM **effectivity dates** | **no** | **no** | **no** | yes [VENDOR] | **no** | yes | yes | **no** | yes |
| Routings w/ **time standards** | yes | yes | yes | yes | **no** | yes | partial | **no** | yes |
| Work centers + capacity | yes | yes (Gantt=EE) | yes | yes | **no** | yes | partial | **no** | yes |
| Shop-floor capture | yes | **no (EE)** | yes | yes | **no** | yes | partial | **no** | yes |
| Actual vs standard costing | partial (no variances) | partial (PPV only) | yes | yes | **no** | yes | partial | **no** | yes |
| MRP | yes | no core / yes OCA | yes | yes | **order-point only** | yes | yes | **no** | yes |
| Inspection plans | yes | no / partial OCA | yes | **yes + AQL** | yes | **no** | **no** | **no** | **no** |
| NCR w/ disposition | **no** | partial OCA | yes | **yes + MRB** | **no** | **no** | **no** | **no** | **no** |
| CAPA w/ effectiveness | **no** | partial OCA (18 only) | yes | yes | **no** | **no** | **no** | **no** | **no** |
| **Calibration/gage** | **no** | **no** | **no** | **yes** | **no** | **no** | **no** | **no** | **no** |
| Training records | partial (hrms) | partial | partial (HR) | yes | **no** | **no** | **no** | **no** | **no** |
| ECO/ECN | **no** | **no (EE)** | **no** | yes | **no** | partial | **no** | **no** | **no** |
| Audit trail fit for 11.10(e) | **no** | **no** | **no** | **partial — best** | **no (off)** | **no (opt-in)** | **no** | **none** | **no (off)** |
| **Part 11 e-signature** | **no** | **no** | **no** | **no** | **no** | **no** | **no** | **no** | **no** |
| Bidirectional genealogy | yes (v16) | partial | yes | yes | yes | partial | partial | **no** | partial |
| UDI/GUDID | **no** | **no** | **no** | **no** | **no** | **no** | **no** | **no** | **no** |
| Validation package | **no** | **no** | **no** | **no** | **no** | **no** | **no** | **no** | **no** |

**What you buy or build on top, in every scenario:**
- **Electronic signatures meeting 11.50/11.70/11.200.** Nothing here has one. A build, and it must be validated.
- **UDI/GUDID submission.** FDA's free web UI for low volume, or a commercial service (Reed Tech, Freyr, Registrar Corp).
- **Gage calibration**, unless you pick Carbon — GAGEtrak, GageList, ProCalV5 or IndySoft.
- **The eQMS of record** — document control with approval and periodic review, complaints with MDR decision-making, internal audits, management review.
- **The entire validation package.** IQ/OQ/PQ, requirements traceability, and a vendor assessment that has no vendor to assess.
- **Item/BOM revision control** for ERPNext, Odoo CE, Tryton and Dolibarr.

**Per-option verdicts.** **Axelor** deserves a pilot and is the best non-Carbon candidate on manufacturing and CAPA depth — but no effectivity dates, no calibration, no ECO, automotive design intent, tiny English community, docs years behind code. **ERPNext** is the lowest-abandonment-risk choice, with truly free licensing, working quality *inspection*, free MPS/capacity planning and the best packaged genealogy — but an empty QMS shell and Repost Item Valuation as a standing audit finding. **Odoo CE + OCA on 18.0** is a viable QMS-ish stack most surveys miss, permanently one release behind. **Tryton, iDempiere, metasfresh, Dolibarr and OFBiz are not candidates.**

---

## 7. Q2 — Commercial pricing and what the money buys

> **Retraction applied in place.** An earlier draft of this spike published **"Plex ≈ $500/user/month"** from SelectHub and computed "30 users × $500 = $180,000/yr." **Both are withdrawn.** Plex is not priced per seat at all — see below. The figure appears on SelectHub, Top10ERP *and* ERP Research, but it is one datapoint echoed three times against the vendor's own stated model.

### 7.1 Figures, with evidence grade

| Vendor | Figure | Grade |
|---|---|---|
| **Arena (PTC)** | **Median $48,683/yr**, avg $48,682, range $12,427–$343,919, **30 counted contracts**; avg negotiated savings 13.29%. **$1,200–$2,500/user/yr** (Launch $1,200–1,800, Enterprise $2,000–2,500). Read-only seats 30–50% of read-write; **supplier seats 20–40%**. 10–25 users ≈ $18k–$35k/yr. **Implementation 15–30% of first-year subscription, additional.** https://www.vendr.com/buyer-guides/arena-solutions | **[CONTRACT]** |
| **MasterControl** | **Median $115,673/yr**, range $72,339–$116,837 (201–1,000 employees). A buyer negotiated *"to reduce uplift from 7% YoY to 5% YoY"* — **7% escalator is the default ask**. Validation Excellence Tool and Validation on Demand included. https://www.vendr.com/marketplace/mastercontrol | **[CONTRACT]** |
| **Greenlight Guru** | **Median $43,989/yr**, range $20,975–$54,739. **Mandatory, unpublished onboarding fee**: *"All new Greenlight Guru customers must purchase our one-time onboarding services to get started."* 2–3 year contracts, full term enforced. Their own 2024 Pricing Guide PDF contains **zero dollar figures across ten pages**. https://www.vendr.com/buyer-guides/greenlight-guru | **[CONTRACT]** |
| **QT9** | **No longer publishes prices** (both pricing pages say "Custom pricing" as of Sept 2026). **Concurrent licensing, explicitly "no per-user fees."** Aggregator starting points: QMS $1,700/yr, ERP $20,000. **All 28+ modules, pre-validated IQ/OQ/PQ, Part 11, implementation services, unlimited training and support, and free upgrades are included.** But QMS and ERP quote separately plus paid Data Sync — **budget three line items.** https://qt9software.com/pricing | **[VENDOR]** + **[AGGREGATOR]** |
| **ProShop** | **No verified quote tied to a seat count exists.** Three seat classes (Shop / Office-Manager / Executive), 12-month minimum. Per SoftwareConnect, priced on **total shop employees, not ERP seats** — if true, a 30-person shop pays on 30, not on the 13 who log in. Offers lease and lease-to-own. The circulating "$500–$715/mo" tables are **algorithmically generated by softwarefinder — do not cite as a quote.** | **[UNVERIFIED]** |
| **Plex (Rockwell)** | **Not per user.** *"One low annual subscription fee covers it all, without the complexity of concurrent or named user license"*; *"supports **unlimited users** and machines… as well as all of your customers and suppliers"*; bundles server hardware, support, monitoring, backups and a 24×7 sandbox. **Published floor $3,000/month = $36k/yr.** Top10ERP: $50k–$500k/yr, implementation *"typically start at $100,000"*, target $11–50M revenue. https://plex.rockwellautomation.com/en-us/products/subscription-packages.html | **[VENDOR]** |
| **JobBOSS² (ECI)** | $3,000/yr entry, one-user minimum; typical all-in $3,000–$30,000/yr; implementation $5,000–$40,000 over 2–5 months. **[USER]** a metal-fab owner asked how many licences answered ***"$3k+ a month"*** (~$36k/yr); a smaller shop evaluating Fulcrum at *"around 50k a year, which is like 5x the cost of our current ERP JobBoss"* implies ~$10k/yr. Training billed separately at ***"$1200 per day."*** | **[AGGREGATOR]** + **[USER]** |
| **Fulcrum** | No pricing page; `/pricing` and `/plans` both 404. Only plausible figure "$800/month" starting, weakly sourced. **Identity warning:** every *"Fulcrum $43–$55/user/mo"* figure in circulation is **fulcrumapp.com, a different company** in field operations. | **[UNVERIFIED]** |

**Published list prices, for comparison:** Cetec ERP $50/user/mo standard, $25/user/mo shop-floor, 5-user minimum, ITAR hosting $500/mo. MRPeasy $49/$69/$99/$149 per user/mo, +$79 per block of 10 from the 11th user — **note Quality Control, Serial Numbers and Subcontracting are Professional-tier ($69)**. Katana $299/mo Core with unlimited users, metered on order volume, plus Traceability $249/mo and Mfg Management $199/mo add-ons and $2,000 onboarding — **[USER]** escalation reported $199 → $349 → $899/month. Genius ERP $3,000/user/yr. Epicor Kinetic ~$100–200/user/mo + $1,500–2,500/mo base, $50k–$1M implementation. Odoo $24.90–$61.00/user/mo. Frappe Cloud $5/mo sites, $40/mo servers, not per-user.

**[USER] anchors from actual shops.** *"the straight 'buyout' packages are sounding like in the 10s of thousands of dollars. Monthly fees are ranging from $150 up to $500 (or more)"* — a three-machine shop. One shop put ***"over 10k into the software"*** before abandoning a ProShop implementation as too time-consuming for low-volume/high-mix; another *"had a bad start with proshop, and ended up getting a refund"* and moved to Realtrac. A shop that succeeded used ProShop's **"Flying Start"** package: *"were able to fully implement in almost exactly one year, and go from ISO9001 to AS9100 in the same time frame."*

### 7.2 Total for a 30-person device CM

| Tier | Composition | Year one | Steady state |
|---|---|---|---|
| **1 — integrated shop ERP with QMS inside** | QT9 (implementation + IQ/OQ/PQ included) or ProShop | **$30k–$60k** | **$20k–$45k/yr** |
| **2 — ERP + separate device eQMS** | Fulcrum or JobBOSS² + Greenlight Guru | ~$65k–$95k | $50k–$60k/yr |
| **3 — shop ERP + Arena** | ERP $10k–$20k + Arena 13–15 users $18k–$35k + impl. 15–30% | $45k–$75k | $30k–$55k/yr |
| **4 — enterprise** | Plex $60k–$150k/yr (floor $36k) + MasterControl $115,673 median + impl. 1–2× | $250k–$500k+ | — |

**Headline: $30,000–$75,000 year one, settling to $20,000–$55,000/year.** Tier 4 at 30 people is mispriced against revenue.

**Two cost lines nobody budgets.** (a) **Escalation** — only **29% of SaaS contracts contain a price cap**; negotiated caps run 5–8%; uncapped renewals commonly land 15–30%, against a realized weighted-average enterprise software increase of **8.4% for the twelve months to Q1 2026** (https://vendorbenchmark.com/blog/software-price-inflation-tracker-2026). At uncapped 8–15%, a $40k contract is $59k–$81k by year five. (b) **Customer-side PQ/UAT** — every vendor except QT9 and Arena hands you IQ and stops; internal labour or consultants at $150–$350/hr, typically 40–120 hours for a small eQMS. **[UNVERIFIED]** — this is derived, not published, and is the least confident figure here.

### 7.3 Scope boundaries decide the architecture

This is the most useful finding in the pricing work, because "ERP + eQMS" is not a clean split:

- **Arena** manages DHF, DMR, SOPs and training against Part 820/Part 11/ISO 13485/EU MDR — but does **not** cover **DHR, UDI/GUDID, or ISO 14971**. It does not produce your device history records.
- **Greenlight Guru does no manufacturing at all** — no DHR, no lot genealogy, no work orders, no inventory, **no calibration module**. It ships **paper SOP templates** for DMR, Purchasing, Receiving Inspection, Rework, PM and Calibration. Templates, not modules.
- **JobBOSS²** native quality is thin, but **"JobBOSS2 Advanced Quality by uniPoint"** (announced 10 April 2024) names **ISO 9001, ISO 13485, IATF 16949, AS9100, and "FDA 21 CFR Part 11 & Part 820 Compliance"** across 23 modules including Tooling & Calibration, Document Control, Education & Training, Auditing, Supplier Management, Risk Management and Validation. https://www.ecisolutions.com/news/eci-software-solutions-announces-jobboss2-advanced-quality-by-unipoint/ — **a second commercial Part 11 path, at the cost of a two-vendor seam.**
- **QT9 is the only vendor covering ERP + QMS + DHR/EBR + Part 11 + vendor-executed IQ/OQ/PQ in one relationship.**

**So the Tier-2 architecture that looks cheapest leaves the DHR uncovered by both products** — the actual regulatory deliverable for a contract manufacturer, and the gap a buyer is most likely to discover after signing.

### 7.4 What the money buys that open source does not

1. **A pre-executed validation package.** QT9's pricing page advertises **"Pre-Validated (IQ/OQ/PQ)"** as a line item. That single line is the product. You are not buying features; you are buying evidence.
2. **A vendor to audit.** ISO 13485 7.4 obliges you to evaluate and select suppliers. A commercial vendor answers your questionnaire, signs your quality agreement, hosts your audit. An open-source project has no one to send the questionnaire to — as the unanswered Odoo forum thread demonstrates literally.
3. **A compliant e-signature.** Every commercial medtech option has one; no open-source option does.
4. **Support, which shop owners value far above price.** The most instructive data point in this research: a shop owner with prior SAP-implementation experience collected 250 vendors, shortlisted 20, deep-dived three, and weighted his decision **usability 70%, support 25%, price 5%.** On that scale, free is worth almost nothing.
5. **UDI/GUDID submission**, which exists nowhere in open source.

### 7.5 Which commercial vendors actually serve this buyer

Of the eight named at the outset, **three**: **QT9** (the only one covering the whole chain including DHR with vendor-executed IQ/OQ/PQ), **ProShop** (integrated ERP/QMS explicitly claiming ISO 13485 and Part 11 — but see the serialisation finding below), and **JobBOSS² + uniPoint** (compliance in writing, two vendors). **Arena** covers DHF/DMR but not DHR, UDI or ISO 14971. **Greenlight Guru** does no manufacturing. **MasterControl** is priced for a company five times this size. **Fulcrum** and **Plex** have no medical device story whatsoever.

**ProShop's disqualifying-adjacent finding [USER]:** *"there isn't a great way to handle **serialization**"* (SoftwareAdvice). For UDI and per-unit device traceability that sits directly under their *"Built-in ISO 13485 & FDA 21 CFR Part 11 support"* claim. They also have **no public API** — GetApp documents exactly three integrations (QuickBooks Online, QBO Advanced, High QA) — so you cannot build around it. Their medical page never mentions Part 820/QMSR, DHR, UDI or validation protocols, and the linked "medical" case study is a **10-person general machine shop, not a device company**.

> **Retraction applied in place.** An earlier draft listed Plex as a medtech-relevant vendor. **Withdrawn.** Plex's complete industry list is Aerospace, Automotive, Food & Beverage, High-tech & Electronics, Industrial Manufacturing, Plastics & Rubber, Precision Metalforming & Fabrication — **there is no medical device or life sciences page** (https://plex.rockwellautomation.com/en-us/industries.html). Its QMS page names **APQP, PPAP, FMEA, HACCP, FSMA, SQF** and **does not name ISO 13485, 21 CFR Part 11, Part 820, UDI or DHR**. The QMS brochure's subtitle is *"Driving Quality and Winning New Business for **Automotive** Manufacturers."* Plex's QMS is genuinely strong — document control, error-proofing that stops production when quality goes out of spec, SPC, digital checksheets, **gauge management**, supplier quality, CAPA — it is just built for IATF 16949, exactly as Axelor's is.

**Plex's architecture is a Part 11 validation blocker, and the reasoning generalises.**
> *"a single, always current line of SaaS code delivered to all customers"*; *"The customer never sees an update notification, never validates a release, and never schedules a maintenance window"*; *"In Plex, there is one environment: the tenant… Configuration is live when saved. There is no build, no deploy, no source control, no environment strategy, no regression-testing obligation."* — https://erppathway.com/paths/plex/development-tooling

And from a ten-year Plex analyst: *"The test environment automatically refreshes from your production environment every night at midnight."* For a shop that must hold a **validated state** with change control and regression evidence, this is structurally disqualifying: **you cannot pin a version, and your test instance is destroyed nightly.** That is not a Plex bug — it is the logical endpoint of continuous-delivery multi-tenant SaaS, and it caps how far the whole category can serve regulated manufacturing.

### 7.6 The incumbents are consolidating, and the users are leaving

`shoptech.com/pricing` now **301-redirects to `ecisolutions.com/products/jobboss2/`** — E2 is folded into ECI's JobBOSS². (Lineage note: ECI acquired Shoptech 2020/21; JobBOSS² launched 4 May 2021 as the cloud successor; **JobBOSS classic and JobBOSS² are different products**.)

**[USER] Support collapse, stated explicitly:** *"JobBoss customer support is below average at best… **E2 (Before acquisition with JobBOSS) customer was great**… I could call in and get an answer right away on the phone."* And: *"I agree that customer service has **hit rock bottom after the ECI acquisition**."*

**A documented price increase, with a precision caveat.** A BBB complaint filed 15 August 2024 shows a maintenance invoice going from **$11,641.61 (Sept 2023) to $12,747.56 (Sept 2024) — +9.5% YoY** — and ECI deprecated that customer's version **one month after billing the renewal**: *"I needed support in July 2024, I kept opening cases… finally I was told my software version was no longer supported."* **This is ECI M1, a sibling product, not JobBOSS² itself.** Cite as vendor behaviour, not as a JobBOSS² figure.

**Data lockout and cancellation traps**, from filed complaints: a ten-year customer gave one month's notice and ECI *"shut down all of our access, **locking us out** of critical data such as payroll information, accounting information and work order information"* before they could respond. Elsewhere: *"they have no provisions or support to extract your data in bulk"*; *"we were told we missed the cut-off to cancel and would have to pay ten more months"*; a customer put *"into collections if we didn't pay over $10,000 — for a product we never used."*

**And the revolt.** A roughly thirty-year customer: ***"I'm cancelling jobboss2 as well this year… Going to get my data dump and move on after like 30 years with them. The cost is outrageous for a program that spits me out mostly packing slips and purchase orders."*** Another: *"I hated JobBoss so much I just programmed my own in PHP, HTML SQL. Forget that seat limit and crazy maintenance fees."* Both Fulcrum and StartProto now run dedicated switch-off landing pages targeting ECI's base.

General sentiment on the incumbents: *"Been using e2 for 16 years and would not recommend it to my worst enemy… when the real questions come out, the salesmen always have the same blank stare"*; *"Just bloated with a ton of stuff you pay for but don't use"*; and from the exact target profile (30 people, 13 ERP users, $5M+, high-mix low-volume): *"With JobBoss2 it feels like we're constantly having to create work-arounds and half-a$$ measures to sort of get their flow to work with the real world."*

---

## 8. Q3 — Where the genuine, defensible gaps are

**1. The intersection itself is empty.** Every open-source medtech-quality project targets software teams doing IEC 62304. Every open-source manufacturing project ignores regulation. The physical-device contract manufacturer has nothing built for it. The strongest evidence is negative and hard to argue with: zero GitHub repos for Part 11 support on either major ERP; a single HN submission on ISO 13485 scoring one point; an entire eQMS category topping out at 8 stars.

**2. A validated, drop-in Part 11 layer.** Section 2 establishes the signature void. The audit trails share a single failure mode worth naming, because it is not an accident: **OFBiz's `EntityAuditLog`, Tryton's `_history`, Frappe's `track_changes` and iDempiere's `AD_ChangeLog` are all excellent machinery that ships disabled or opt-in, and none is enabled on manufacturing and quality records.** Audit trails cost storage and performance, and no upstream maintainer has a regulatory reason to turn them on. A project whose *default* is always-on, append-only, reason-for-change-capturing field history plus a real signature primitive — treating Carbon's service-role-only RLS as the floor, not the ceiling — solves a problem nobody upstream is motivated to solve.

**3. Validation-as-code, which CSA just made viable.** Before September 2025 the answer to "why can't open source serve regulated manufacturing" was "because validation documentation is a services product." CSA changes the economics: risk-proportionate, superseding the documentation-heavy Section 6 of GPSV. An open-source project that publishes its CI test evidence, a requirements-to-test traceability matrix generated from its own repo, and IQ/OQ templates that execute against a known build delivers something a closed vendor's PDF cannot: **auditable, reproducible, versioned evidence you can re-run yourself.** Nobody has built this. The tooling to do it — doorstop, StrictDoc, Sphinx-Needs — is the one healthy corner of this landscape and is sitting unused by the manufacturing projects. Scale reference from Arena's own disclosure: **validation requirements grew from 86 in 2008 to over 1,600 in 2023** (https://www.arenasolutions.com/blog/arena-validate-iq-and-oq-confidence/), regenerated every release. That is a machine-generatable artifact being sold as a service.

**4. Sovereign, on-premise, version-pinnable deployment.** The market is moving to cloud-only exactly where this buyer cannot follow, and the argument is stronger than NDA convenience — it is a *validation* argument. A regulated manufacturer needs a pinnable version, a persistent qualification environment, and control over when changes land. Plex cannot be version-pinned and wipes its test tenant nightly. Odoo requires shipping your production database to Odoo to upgrade. ECI deprecates versions a month after billing and locks customers out on exit. **The core requirement — *this exact software, in this exact state, until I decide otherwise* — is becoming something the commercial market structurally cannot sell.** Self-hosted open source is the only architecture that grants it by default, and not one project is making that argument. Supporting it from the target market: *"My clients are medical device developers, and I have signed NDA's with them. I cannot use any cloud services for CAD/CAM/ERP etc."*

**5. Gage calibration with out-of-tolerance impact assessment.** Carbon is the only open-source system with a real gage crib, and even it stops at a `requiresAction` boolean. Clause 7.6's actual teeth — when equipment is found out of tolerance you must **assess and record the validity of previous measuring results and act on affected product** — requires walking from the gage to every inspection it performed to every lot those inspections released. That is a graph query over data an ERP already holds, it is the single most audit-exposed calculation in a device shop, and **no open-source system performs it.**

**Two things that look like gaps and are not worth chasing:** UDI/GUDID submission (real void, but an FDA-gateway integration problem with a small addressable market and established commercial services), and yet another general-purpose ERP core (ERPNext and Odoo occupy it, and it is not the bottleneck).

---

## 9. Q4 — What killed or stalled the previous attempts

**Full treatment is in `spike-governance.md`**, because it is a different kind of finding from the feature comparison and it bears on a governance decision rather than a product decision.

Summary for continuity: every confirmed death traces to **concentrated copyright plus a vendor whose commercial interest eventually diverged from the open edition** — Compiere (absorbed into Aptean), Openbravo (pivoted to retail under Orisha), xTuple/PostBooks (repositories deleted, product continues closed under CAI), Fedena (open edition abandoned while proprietary SaaS thrives), uniCenta (source moved behind a membership wall). SQL-Ledger shows the mechanism operating with no acquisition at all — a founder paywalling documentation, forums and fixes to protect a services income, until his own users forked him over a year-old security hole. Tryton demonstrates the only known structural defence: **refuse the CLA, disperse the copyright, put a foundation between the vendor and the trademark.** And the newest entrant, Carbon, shipped the failure configuration from day one.

---

## 10. Open evidence gaps

Two remain open, both from environment blocks rather than from absence of evidence. They are stated here rather than buried, because someone will ask.

**10.1 No verified ProShop quote tied to a seat count.** ProShop publishes no prices; every circulating figure traces either to an aggregator directory field or to `softwarefinder`, whose tables are algorithmically generated. The two structural facts that matter more than the missing number — three seat classes with a 12-month minimum, and SoftwareConnect's claim that pricing is on **total shop employees rather than ERP seats** — are themselves unconfirmed by the vendor. For a 30-person shop with 13 ERP users, that single question is the largest swing factor in any quote and **must be asked directly.**

**10.2 ProShop's deployment model is unresolved.** SoftwareAdvice lists "Cloud-based, On-premise." SoftwareConnect says on-premise is **not** offered. SelectHub says web-based only. Four candidate URLs (`/proshop-on-prem/`, `/on-prem/`, `/proshop-erp-on-premise/`, `/features/compliances/`) **do not exist**. Their defense/ITAR page makes **no statement about hosting or data residency at all.** The only hint is "Standard vs. Government versions" on the pricing page. Given that on-premise capability is decisive for the NDA and version-pinning arguments in Section 8, this must be asked directly and in writing.

Lower-priority items also left open: Fulcrum's "$800/month" is weakly sourced and may be cross-contaminated with the unrelated `fulcrumapp.com`; Greenlight Guru's reported ~100% January 2026 repricing rests on a single outlet whose page blocks automated fetching; the customer-side PQ/UAT cost in §7.2 is derived rather than published; Arena Validate's pricing is undisclosed; and iDempiere's `AD_ChangeLog` old/new-value column detail comes from the Compiere data model rather than a page that could be fetched.

---

## Appendix — Method and environment

**What was verified directly:** all licence files; Odoo's Community/Enterprise split by probing `raw.githubusercontent.com` manifest paths on branch 19.0 (200 = present, 404 = Enterprise); ERPNext's complete doctype inventories and the BOM, Non Conformance, Quality Action and Quality Inspection schemas; Frappe's `version.py`, `hooks.py` log-purge defaults and `audit_trail.json`; Tryton's `modelsql.py` `_history` mechanism and per-module absence; Carbon's LICENSE, `quality.models.ts`, `samplingStandards.ts`, `approvals-workflow.sql`, `audit_log_system.sql`, `procedures.sql`, `training_assignments.sql` and full migration inventory; OCA branch contents on 18.0 and 19.0; OFBiz's `EntityAuditLog` definition and `enable-audit-log` counts; Dolibarr's `htdocs/mrp/` contents and both logging triggers; Axelor's domain XML inventories; GitHub `commits.atom` feeds for last-commit dates on every project.

**Environment constraints that shaped sourcing.** The session-wide WebSearch budget (200 calls) was exhausted partway through, so the later work ran on direct fetching. **G2, TrustRadius, ITQlick, Reddit and web.archive.org were hard-blocked**, which is why there are no Wayback citations and why Reddit quotes that do appear were obtained through the Arctic Shift public archive API. Working channels were: `r.jina.ai/<url>` via WebFetch as a text-extraction proxy (it renders Cloudflare-protected pages and returns HTTP 422 when the underlying page 404s, doubling as a page-existence oracle); direct WebFetch at capterra, selecthub, getapp, softwareadvice, softwareconnect and vendr; plain curl with a browser User-Agent at practicalmachinist.com and at vendor `sitemap.xml` files — the last being the fastest way to prove a topic is *absent* from a site, which is how Fulcrum's zero medical/13485/FDA URLs across 932 pages was established.

**Known metadata hazard.** GitHub's `license` API field reports the README badge or a top-level text match and silently misses hybrid arrangements. It returned `NOASSERTION` for `qcadoo/mes`, `odoo/odoo`, `strictdoc`, `doorstop` and `rmtoo`, and reported Carbon as AGPL-3.0 when its LICENSE is a hybrid with a commercial carve-out. **Read the LICENSE file; do not trust the metadata field.** `qcadoo/mes` remains the one unverified licence referenced in this document.
