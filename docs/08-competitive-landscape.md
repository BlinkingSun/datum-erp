# Competitive Landscape

Conforms to: `docs/adr/0004-append-only-ledger.md`, `docs/adr/0005-compliance-in-kernel.md`,
`docs/adr/0006-license.md` (Accepted 2026-09-12), `docs/adr/0007-defer-general-ledger.md`,
`docs/adr/0008-single-tenant.md`.

This is a comparison, not a pitch. License, price and feature claims cite a `research/`
path that holds the primary-source quote. Counted-contract medians are the Vendr figures
in `research/background/competitive-landscape.md` §7.1 (the same numbers the landscape
spike recorded; cite that research path). Sample-size caveats sit
next to the numbers. Where a figure is aggregator, vendor list, or a single-shop
anecdote, it is labeled as such.

A shop owner comparing options should be able to tell what already exists, what it
costs, and which Wicket claims are present facts versus unbuilt phases. A contributor
should be able to tell what not to rebuild.

---

## 1. Who the beachhead actually buys from

The 10-to-100-person ISO 13485 / FDA-registered machine shop making Class I–II implants
or instruments does not start from Odoo versus SAP. It starts from a job-shop ERP, a
separate eQMS, a quoting tool, and spreadsheets across the seam
(`research/audits/slice-competitive.md` §5.1; `docs/01-vision-and-scope.md` §3–§4).

Typical stack:

1. **Job-shop ERP** — very often JobBOSS² or ProShop, sometimes Epicor Kinetic or Infor
   Visual (`research/audits/slice-competitive.md` §5.1).
2. **eQMS** — Qualio, QT9, or Greenlight Guru (same section).
3. **Spreadsheets** to reconcile lot, Device History Record, and training across the
   two systems.
4. Often **Paperless Parts** for quoting (`research/audits/slice-competitive.md` §5.1, §6).

Closing that seam is the product (`docs/01-vision-and-scope.md` §4). Until Phase 4
modules exist, it is a promise (`research/audits/slice-competitive.md` §8).

**ProShop is the closest commercial analogue:** integrated ERP + MES + QMS, paperless,
machine-shop native, claiming ISO 13485 and 21 CFR 11
(`research/audits/slice-competitive.md` §5.2). It is closed, priced on headcount rather
than seats according to third-party directories, and the vendor holds the records.
Wicket is trying to be that product as open source, with audit and signature as kernel
properties rather than a QMS add-on. That sentence is the claim. It is not true of the
kernel-only foundation build.

---

## 2. Open source ERPs

No open source ERP surveyed in September 2026 provides an electronic signature meeting
21 CFR Part 11 Subpart C (meaning, re-authentication at signing, non-excisable record
linkage, on manufacturing and quality records)
(`research/background/competitive-landscape.md` §2, §0.1). Several have approval
workflows and attribution fields. An auditor knows the difference.

### 2.1 ERPNext (Frappe)

| | |
|---|---|
| License | ERPNext **GPL-3.0**. Frappe framework **MIT**. No Enterprise edition and no paywalled module (`research/background/competitive-landscape.md` §4.1). |
| Monetisation | Hosting and warranty. Frappe Cloud sites from $5/month, servers from $40/month, not per-user (same section). |
| Manufacturing | Real: Work Order, Job Card, BOM, routing, subcontracting, shop floor (`research/audits/slice-competitive.md` §2.4). BOM has no revision, no effectivity dates, no ECO; the BOM Update Tool can rewrite submitted parent BOMs (`research/background/competitive-landscape.md` §4.1). |
| Quality | Quality Inspection is core and real: incoming / in-process / outgoing, templates, can block submit (`research/audits/slice-competitive.md` §2.3; `research/background/competitive-landscape.md` §4.1). Non Conformance is a thin form with free-text CAPA fields and no disposition, MRB, lot hold, or signature (`research/audits/slice-competitive.md` §2.3). No CAPA doctype with effectiveness check, no DHR, no training veto, no calibration impact. A Frappe partner answer on medical-device QMS: ERPNext QC is suitable with caveats; not out-of-the-box ISO 13485 or 21 CFR Part 11 (`research/audits/slice-competitive.md` §2.3). |
| Audit / signature | `Version` is a JSON field diff, opt-in per DocType (`track_changes`), and Administrator may delete Version rows (`research/audits/slice-competitive.md` §2.7). Activity Log is purged at 90 days by default (`research/background/competitive-landscape.md` §4.1). Electronic signature is marketplace PKI on PDFs for Indian tax law, not 21 CFR 11.200 (`research/audits/slice-competitive.md` §2.6). |
| Genealogy | v16 ships a Serial No and Batch Traceability Report with backward/forward walk (`research/audits/slice-competitive.md` §C3; `research/background/competitive-landscape.md` §4.1). It is a report, not a graph that includes operator, gage, and inspection. |
| Ledger | Quantity is derived from `tabStock Ledger Entry`; Bin is a cache. Cancel flags SLE rows and inserts negations; repost rewrites `qty_after_transaction` on the posting and on future rows (`research/audits/slice-competitive.md` §2.1). Ghost-stock Bin desync and concurrent-qty bugs are open in 2026 (same section). This is an inventory journal. It is not append-only, not zero-sum, and not immutable. |
| Healthcare | Clinical (patients, encounters), split into Marley Health. Not device manufacturing (`research/audits/slice-competitive.md` §2.5). |

ERPNext is the lowest-abandonment-risk general OSS ERP: GPL-3.0, no open-core split,
working incoming inspection, packaged genealogy (`research/background/competitive-landscape.md`
§6). It is not an eQMS, and it is not Part 11.

### 2.2 Odoo Community

| | |
|---|---|
| License | Community **LGPLv3** since Odoo 9 (2015). Enterprise is proprietary OEEL-1.0 (`research/audits/slice-competitive.md` §C7; `research/background/competitive-landscape.md` §4.2). Odoo 8 was AGPL; current Community is not. |
| Monetisation | Odoo SA's published split: 80% open to attract users, 20% Enterprise to improve revenue, with modules chosen because a niche is easy to monetize (`research/background/open-source-governance.md` §3.4; `research/background/competitive-landscape.md` §4.2). List: Standard $24.90–$31.10/user/month; Custom $49.00–$61.00. On-premise requires Custom (`research/background/competitive-landscape.md` §4.2). |
| Manufacturing | MRP (BOM, manufacturing orders, work orders, work centers) is in Community (`research/audits/slice-competitive.md` §3). Shop-floor terminal, MPS, barcode, and BoM versioning / ECO (`mrp_plm`) are Enterprise (`research/background/competitive-landscape.md` §4.2). Community BoMs have no version, revision, or effectivity, and remain editable after product is built (same section). |
| Quality | **Not in Community.** Enterprise `quality_control` / `quality_mrp` / `quality_stock`. Community substitute: OCA `quality_control_oca` and related modules (`research/audits/slice-competitive.md` §3, C1). An unregulated shop on Community never sees quality because the SKU is not installed. |
| Audit / signature | Community has no built-in audit log. OCA `auditlog` is an add-on (`research/audits/slice-competitive.md` §3). A cryptographic hash chain exists on posted accounting journal entries only, for EU anti-fraud tax law; DHR, lots, NCR and inspections get none of it (`research/background/competitive-landscape.md` §4.2). Odoo Sign is Enterprise and signs rendered PDFs, not live records; Odoo's docs cite eIDAS / ESIGN, not 21 CFR Part 11 (`research/background/competitive-landscape.md` §2.1). |
| Genealogy | Chronological `stock.move.line` table. Community users report missing the component-lot walk that Enterprise "upstream/downstream" shows (`research/audits/slice-competitive.md` §C3). |
| Governance | Relicensing AGPL to LGPL at v9 and moving Quality, PLM and shop floor behind OEEL is a completed event, not a risk (`research/background/open-source-governance.md` §2, §3.4). The Odoo Community Association reimplements withheld modules and is permanently one major version behind: 232 / 212 / 200 / 174 open migration trackers for 19.0 / 18.0 / 17.0 / 16.0 (same §3.4). |

LGPL Community is more adoption-friendly than Wicket's AGPL-3.0-or-later
(`docs/adr/0006-license.md`). There is no license wedge
(`research/audits/slice-competitive.md` §C7).

### 2.3 Tryton

| | |
|---|---|
| License | **GPL-3.0-or-later** throughout, no open core (`research/background/competitive-landscape.md` §4.3). |
| Manufacturing | Production, routing, work, outsourcing, stock lots exist. Routing is name + steps; no setup/run time, yield, or effectivity. Work-center picker ends in `random.choice()`. No standard costing, therefore no variances. MRP is order-point (`research/background/competitive-landscape.md` §4.3). |
| Quality | Official `trytond-quality` since 6.8 (8.0.1 as of 2026-07): control points, inspections, failed inspection can block the document; deletion of non-pending inspections blocked since 7.4 (`research/audits/slice-competitive.md` §4; `research/background/competitive-landscape.md` §4.3). No NCR/MRB, CAPA, gage, training, complaints, ECO, or signature. |
| Audit / signature | `ModelSQL._history` is the cleanest audit architecture in the survey and is off for production, stock.move, product and party (`research/background/competitive-landscape.md` §4.3). Attribution fields on Inspection are not signatures (`research/background/competitive-landscape.md` §2.1). |
| Genealogy | `stock_lot` Lot Trace: upward and downward traces as a tree, through production (`research/background/competitive-landscape.md` §4.3). |
| Governance | No CLA, by policy. Copyright deliberately dispersed (`research/background/open-source-governance.md` §1). 83% of last-year commits from one person; zero North American service providers; forum archive has no hits for MRP, quality control, 21 CFR, ISO 13485, or medical device (`research/background/competitive-landscape.md` §4.3). |

Tryton is a technically careful also-ran for this beachhead
(`research/audits/slice-competitive.md` §4). Mention it. Do not rebuild it. Do not
fear it.

### 2.4 Other open source the research covers

None of these steal the medical-device beachhead
(`research/audits/slice-competitive.md` §4). Carbon is the exception that needs a
paragraph, not a row.

| Product | License | Manufacturing + quality | Why it is not the beachhead |
|---|---|---|---|
| **Carbon** | Hybrid. README and GitHub metadata say AGPL-3.0; the LICENSE file forbids internal production use unless modifications are published or a commercial license is bought, and carves out `packages/ee` (`research/background/competitive-landscape.md` §3.1–§3.2). | Strongest OSS quality module: ANSI/ASQ Z1.4 sampling, MRB dispositions linked to job operations, gage crib with as-found/as-left, versioned procedures, training assignments, ECO notices. Audit log is the best in the survey and off by default. **No Part 11 signature** (zero migrations contain "signature"; approvals enum is purchase order and quality document only). No UDI. No out-of-tolerance impact walk (`research/background/competitive-landscape.md` §2.1, §3.3). | For a contract manufacturer under NDA, "publish your modifications" is usually impossible, so Carbon prices as commercial software with an open core (same §3.2). Study the quality model. Do not treat the badge as the license. |
| **Axelor Open Suite** | AGPL-3.0-or-later; Pro/Enterprise withhold updates, SSO, Studio (`research/background/competitive-landscape.md` §4.6). | Real MRP II, BOM revisions (no effectivity dates), shop-floor capture, NCR/CAPA modelled as 8D/QRQC with effectiveness check. Design intent is IATF 16949 automotive, not ISO 13485. No calibration, no ECO, no Part 11 signature (same §4.6, §2.1). | Best non-Carbon candidate on manufacturing and CAPA depth. Tiny English community, docs years behind code, one corporate steward. |
| **iDempiere** | GPLv2-only (`research/background/competitive-landscape.md` §4.4). | Discrete manufacturing lives in the Libero plugin, not core. Upstream Libero is deprecated (2015); maintained forks are 3–7 stars. Quality is a stub. No signature (same §4.4, §2.1). | Install threads outnumber usage threads. Not a candidate. |
| **metasfresh** | GPLv2, no code paywall (`research/background/competitive-landscape.md` §4.5). | Food/catch-weight manufacturing. Zero quality modules. Releases stopped publishing at 5.175 (June 2023). | Unversioned Docker tags, 93% of recent issues from vendor staff, a public unacknowledged document-access bypass. Strike it. |
| **Dolibarr** | GPL-3.0-or-later (`research/background/competitive-landscape.md` §4.7). | Light MRP, no sequenced operations, no quality in core. Accepts duplicate serial numbers. | Disqualifying for device serialization. Quality on Dolistore is a paid add-on (`research/audits/slice-competitive.md` §4). |
| **Apache OFBiz** | Apache-2.0 (`research/background/competitive-landscape.md` §4.8). | Best BOM effectivity model in the survey (`ProductAssoc.fromDate` in the primary key). Quality: zero. Audit engine exists and is off. Feature-frozen since September 2024. 19 CVEs in 2026. | Governance protects the license and does not protect the project (`research/background/open-source-governance.md` §3.14). |

The open-source eQMS category is empty for a physical-device contract manufacturer:
GitHub `eQMS medical device` returned four repos, top eight stars, and every
medtech-quality project targets IEC 62304 software teams
(`research/background/competitive-landscape.md` §5.1). Gage calibration and UDI
submission are voids outside Carbon's crib (still missing clause 7.6 impact) and
FDA's own GUDID web UI.

---

## 3. Commercial medical-device QMS and shop ERP

### 3.1 Counted-contract eQMS / PLM (Vendr)

These are counted contracts, not list prices. Treat them as order-of-magnitude. The
sample is whatever Vendr had, not a random sample of 30-person shops
(`research/background/competitive-landscape.md` §7.1).

| Product | Median / range | Sample caveat | What the money buys, and what it does not |
|---|---|---|---|
| **Arena (PTC)** | Median **$48,683/year**, average $48,682, range $12,427–$343,919. 10–25 users about $18k–$35k/year. Implementation 15–30% of first-year subscription, extra (`research/background/competitive-landscape.md` §7.1). | **30 counted contracts.** Strongest pricing evidence in the set. | DHF, DMR, SOPs, training against Part 820 / Part 11 / ISO 13485 / EU MDR. Does **not** cover DHR, UDI/GUDID, or ISO 14971 (`research/background/competitive-landscape.md` §7.1, §7.3). |
| **Greenlight Guru** | Median **$43,989/year**, range $20,975–$54,739. Mandatory unpublished onboarding. 2–3 year contracts, full term enforced. Own pricing PDF has zero dollar figures (`research/background/competitive-landscape.md` §7.1). | Vendr range; not a seat-count quote. Anecdotes of $25k–$60k/year also appear (`research/audits/slice-competitive.md` §5.3). | Med-device design controls + QMS. **No manufacturing:** no DHR, no lot genealogy, no work orders, no inventory, no calibration module. Paper SOP templates for DMR, receiving inspection, rework, PM and calibration (`research/background/competitive-landscape.md` §7.3). Strong for a design-control shop; weak for a contract manufacturer who does not own the DHF (`research/audits/slice-competitive.md` §5.3). |
| **MasterControl** | Median **$115,673/year**, range $72,339–$116,837. Default ask includes a 7% yearly uplift (`research/background/competitive-landscape.md` §7.1). | Range is for **201–1,000 employees**, five times the beachhead. Do not quote this as a 30-person price. | Enterprise life sciences. Validation tools included. Low threat at 30 people (`research/audits/slice-competitive.md` §5.3). |

`docs/01-vision-and-scope.md` §3 item 3 names these three medians. That band is the
integrated commercial suite. The more common 30-person spend is a job-shop ERP plus a
separate eQMS plus a quoting tool, still five figures
(`research/audits/slice-competitive.md` §5.3).

### 3.2 Job-shop ERP tier

No counted-contract median exists for ProShop, JobBOSS², or Epicor in the research.
Figures below are aggregator, vendor, or identifiable-user reports. They are labeled.

| Product | Price signal | Grade | Why a shop looks, and why it is not Wicket's wedge by itself |
|---|---|---|---|
| **ProShop** | No verified quote tied to a seat count. Third-party directories claim pricing on **total shop employees**, not ERP seats, three seat classes, 12-month minimum. Circulating "$500–$1,800/month" tables are algorithmically generated; do not cite as a quote (`research/background/competitive-landscape.md` §7.1, §10.1; `research/audits/slice-competitive.md` §5.2). | Unverified | Closest analogue: ERP+MES+QMS, claims ISO 13485 and 21 CFR 11. Reviewer report: serialization is weak. No public API. Medical case study is a 10-person general machine shop (`research/background/competitive-landscape.md` §7.5). Deployment model (cloud vs on-prem) is unresolved in the research (§10.2). |
| **JobBOSS² (ECI)** | Aggregator: ~$3,000/year entry to ~$3,000–$30,000/year typical; implementation $5,000–$40,000. User: one fab owner "$3k+ a month"; another shop implied ~$10k/year by comparing Fulcrum at ~$50k. Training $1,200/day (`research/background/competitive-landscape.md` §7.1). | Aggregator + user | Largest installed base in small US machine shops. Native quality is thin; "JobBOSS2 Advanced Quality by uniPoint" is a second vendor claiming ISO 13485 and Part 11 (`research/background/competitive-landscape.md` §7.3). Shops add an eQMS (`research/audits/slice-competitive.md` §5.2). Support collapse after the ECI acquisition is documented in user quotes (§7.6). |
| **Epicor Kinetic** | Quote. Placed in the $40k–$150k/year band `docs/01` names (`research/audits/slice-competitive.md` §5.2). | Not counted | Real 2026 medical-device ERP story (traceability, CAPA, eMDR). Priced and implemented for companies larger than the beachhead. |
| **Paperless Parts** | Not an ERP. No counted price in the research. | — | Owns CAD-to-estimate for this buyer: geometry interrogation, BOM from assemblies, medical-device routing templates (FAI, sampling, traceability), integrations into JobBOSS, ProShop, Global Shop, Epicor, Infor (`research/audits/slice-competitive.md` §6; `research/background/competitive-landscape.md` §3.2 lists `paperless-parts` inside Carbon's `packages/ee`). |

QT9 no longer publishes prices (both pages say "Custom pricing" as of September 2026).
It is concurrent-licensed, claims ERP + QMS + DHR/EBR + Part 11 + vendor-executed
IQ/OQ/PQ in one relationship, and quotes QMS and ERP separately plus paid Data Sync
(`research/background/competitive-landscape.md` §7.1, §7.3). Aggregator starting
points (QMS $1,700/year, ERP $20,000) are hints, not quotes.

Plex is **not** a medical-device vendor. An earlier internal figure of ~$500/user/month
was withdrawn. Plex is not per-seat; published floor $3,000/month; QMS is IATF 16949
automotive; no ISO 13485 / Part 11 / UDI / DHR page
(`research/background/competitive-landscape.md` §7.1, §7.5). Do not resurrect it.

### 3.3 Combined spend, with the caveats attached

From `research/background/competitive-landscape.md` §7.2, derived from the figures
above, not from a new sample:

| Tier | Year one | Steady state |
|---|---|---|
| Integrated shop ERP with QMS inside (QT9 or ProShop) | $30k–$60k | $20k–$45k/year |
| Shop ERP + separate device eQMS (JobBOSS² + Greenlight Guru) | ~$65k–$95k | $50k–$60k/year |
| Shop ERP + Arena | $45k–$75k | $30k–$55k/year |
| Enterprise (Plex + MasterControl) | $250k–$500k+ | mispriced at 30 people |

Headline the research is willing to defend: **$30,000–$75,000 year one, settling to
$20,000–$55,000/year**, for a 30-person device contract manufacturer. Two lines shops
under-budget: uncapped SaaS escalation (only 29% of SaaS contracts contain a price cap),
and customer-side PQ/UAT labor, which is derived rather than published and is the
least confident figure in that section.

The Tier-2 architecture that looks cheapest often leaves the DHR uncovered by both
products (`research/background/competitive-landscape.md` §7.3). That is the
deliverable a contract manufacturer is audited on.

What the money buys that open source does not, today
(`research/background/competitive-landscape.md` §7.4): a pre-executed validation
package; a vendor to send the ISO 13485 7.4 questionnaire to; a compliant
e-signature; support (one shop owner weighted usability 70%, support 25%, price 5%);
UDI/GUDID submission.

---

## 4. Honest differentiation table

Rows are the claims in `docs/01-vision-and-scope.md` §5 as reconciled. "What is
actually different" is never a silent future. If the difference is an unbuilt phase,
the cell says so.

| Claim | Who already has something comparable | What is actually different | Phase that delivers it |
|---|---|---|---|
| Quality is deeper than what already ships: CAPA, a Device History Record assembled from the same postings, training veto at clock-on, calibration out-of-tolerance impact (`docs/01-vision-and-scope.md` §5) | ERPNext ships Quality Inspection and Non Conformance in core. Odoo Enterprise ships a Quality app (Community does not; OCA is the substitute). Tryton ships `trytond-quality`. Axelor ships 8D/QRQC CAPA with an effectiveness check. Carbon ships MRB, Z1.4 sampling, gage crib, training, ECO. ProShop and QT9 sell integrated QMS. Qualio / Greenlight Guru / MasterControl sell the eQMS half (`research/audits/slice-competitive.md` §C1, §2.3, §3, §4; `research/background/competitive-landscape.md` §3.3, §4.6, §6). | Depth, not presence. ERPNext's Non Conformance is not an NCR with containment and MRB. No surveyed OSS ERP vetoes an unqualified operator at clock-on, assembles a DHR from the same postings as inventory, or walks a gage found out of tolerance to the lots it released (`research/background/competitive-landscape.md` §8 item 5; `research/audits/slice-competitive.md` §2.3). **That depth is a Phase 4 promise, not a present fact.** Quality modules already exist in ERPNext, Odoo Enterprise and Tryton; they are not what distinguishes Wicket today. | **Phase 4**, unbuilt: `capa`, `dhr`, `training`, `calibration` (`docs/04-module-catalog.md`). Not this foundation build (`PLAN.md` §10). |
| Audit trail is a kernel property, not an add-on (`docs/01-vision-and-scope.md` §5; `docs/adr/0005-compliance-in-kernel.md`) | Frappe `Version` (opt-in, Administrator may delete). OCA `auditlog` (add-on, Beta). Odoo Community hash chain on accounting journals only. Tryton `_history`, off on manufacturing records. iDempiere / metasfresh `AD_ChangeLog`, opt-in twice and editable. OFBiz `EntityAuditLog`, off. Carbon's per-company audit log is the strongest OSS trail and defaults to off (`research/audits/slice-competitive.md` §2.7, §3; `research/background/competitive-landscape.md` §2.1, §3.3, §4, §8 item 2). | The trail is produced by a database trigger attached when the table is created, append-only at grant level, and the application cannot write it (`docs/adr/0005-compliance-in-kernel.md`). Module authors cannot forget it. That is structural. It is not "Wicket invented history"; ERPNext already journals stock. It is "the trail cannot be a module you decline." **Kernel interceptor: this foundation build (Phase 0). Investigator-grade records that look like production: Phase 4, unbuilt.** | **Phase 0** (kernel). Waves 1–2 of this build implement the interceptor. Usable on quality records when Phase 4 exists. |
| Electronic signature is a kernel primitive, bound to record hash and meaning, not a PDF product (`docs/01-vision-and-scope.md` §5; `docs/adr/0005-compliance-in-kernel.md`) | **No open source ERP surveyed meets 21 CFR 11.50 / 11.70 / 11.200** (`research/background/competitive-landscape.md` §2). Odoo Sign is Enterprise PDF signing. ERPNext marketplace DSC signs tax PDFs. Carbon approvals are not signatures. Commercial eQMS (Greenlight Guru, MasterControl, QT9, ProShop) advertise signatures (`research/audits/slice-competitive.md` §2.6; `research/background/competitive-landscape.md` §7.4). | Meaning, two identification components at every signing, bound to the hash of the exact record version, declared on the transition so a module cannot skip it (`docs/adr/0005-compliance-in-kernel.md`). That primitive does not exist in OSS ERP. **The primitive is unbuilt as a usable signing product until Wave 2b (`wicket-esign`); signatures on NCR, DHR and controlled documents are unbuilt until Phase 4.** This foundation build's kernel-only waves cannot show an investigator a signed DHR. | **Phase 0** primitive; **Wave 2b** minting in this build; **Phase 4** on quality records. Until Phase 4, a promise. |
| Lot and serial genealogy is a graph over the ledger, not a report you write yourself (`docs/01-vision-and-scope.md` §5) | ERPNext v16 Serial No and Batch Traceability Report. Tryton Lot Trace tree. Odoo chronological move table (Enterprise upstream/downstream stronger). Axelor `TrackingNumber` parent set. Carbon lineage RPCs (`research/audits/slice-competitive.md` §C3; `research/background/competitive-landscape.md` §4.1, §4.3, §4.6, §3.3). | Wicket's design is a graph over immutable postings and consumption edges, later including operator, gage and inspection (`docs/adr/0004-append-only-ledger.md`; `docs/04-module-catalog.md` `genealogy`). That is a better *design*. It is not a capability the market lacks as a *report*. A shop already gets a traceability walk from ERPNext v16. The graph that answers "which gage, which operator, which heat lot went into this bone-screw serial" needs Phase 1 lots, Phase 3 production, and Phase 4 genealogy. **Unbuilt.** | **Phase 4** `genealogy`, fed by **Phase 1** `lots` and **Phase 3** `production`. Phase 1 is in this build's Wave 3; Phase 3 and 4 are not (`PLAN.md` §10). |
| Open source, self-hosted, no per-seat pricing, and the shop holds its own quality records (`docs/01-vision-and-scope.md` §5) | ERPNext: GPL-3.0, self-host, $0 license, Frappe Cloud not per-user. Odoo Community: LGPLv3, self-host, $0 license. Tryton: GPL-3.0-or-later, self-host. **Not a difference against those three.** Against Greenlight Guru, MasterControl, Arena, Epicor, ProShop, JobBOSS²: yes (`research/audits/slice-competitive.md` §C4; `research/background/competitive-landscape.md` §4.1, §4.2, §7). | Against commercial regulated suites, the terms are real: no per-seat, no vendor holding records, a pinnable version the shop controls (`docs/adr/0008-single-tenant.md`; `research/background/competitive-landscape.md` §8 item 4). Against general OSS ERP, they are not a reason to skip building quality modules (`research/audits/slice-competitive.md` §C4). Self-host as a *validation* argument (pin the version, keep the qualification environment) is the part commercial SaaS is structurally bad at: Plex cannot pin a version and wipes its test tenant nightly (same §7.5). | **Phase 0** (license, tenancy, install). True of any self-hosted build from the first release. Does not by itself close the ERP/eQMS seam. |
| The validation package ships with the release, rather than a five-figure consulting engagement (`docs/01-vision-and-scope.md` §5) | QT9 advertises pre-validated IQ/OQ/PQ as a line item. Arena Validate regenerates 1,600+ requirements per release. No open source project offers a validation pack (`research/background/competitive-landscape.md` §7.4, §8 item 3). | Generating IQ/OQ from the project's own tests, a requirements-to-test matrix, and executable protocols (`docs/04-module-catalog.md` `validation-pack`). FDA CSA guidance (final 24 September 2025, revised 3 February 2026) makes this newly viable (`research/background/competitive-landscape.md` §1, §8 item 3). **The pack is unbuilt.** Until Phase 6 it is a promise, and it is not this foundation build. | **Phase 6** `validation-pack`. Unbuilt. Out of scope for this build (`PLAN.md` §10). |
| Against building it in spreadsheets: it survives the audit (`docs/01-vision-and-scope.md` §5) | Spreadsheets, paper QMS beside an ERP, binders (`docs/01-vision-and-scope.md` §3). | A single system of record whose kernel trail and Phase 4 modules can answer an investigator. **Only true after Phase 4 exists** (`docs/01-vision-and-scope.md` §5). The kernel-only foundation cannot demonstrate a mock FDA inspection (`research/audits/slice-competitive.md` §C10, Test 2). | **Phase 4**. Unbuilt. |
| CAD-native estimating: ingest STEP, pull features and volume, seed an estimate (`docs/01-vision-and-scope.md` §5) | **Paperless Parts already does this** for job shops and medical-device contract manufacturers, and already integrates with JobBOSS, ProShop, Global Shop, Epicor and Infor. aPriori / DFMA ingest STEP/IGES for OEM should-cost. ProShop bought the Paperless Parts integration rather than building ingest (`research/audits/slice-competitive.md` §C5, §6). | Nothing that is a buying-criterion difference. In-ERP CAD ingest as a module of the system of record is still rare; as a job the beachhead already pays for, it is served. Integrate before cloning. The founder is unusually qualified to build the module later. **Not a present or near-term difference.** | **Phase 7** `cad`. Unbuilt. Out of scope for this build. Do not staff it now (`research/audits/slice-competitive.md` §11). |

The sentence that survives contact with ProShop, Qualio, ERPNext and Paperless Parts
(`research/audits/slice-competitive.md` §8):

A 10–100 person device shop today runs a job-shop ERP and a separate eQMS and a
quoting tool and spreadsheets across the seam. Wicket's durable difference is a
self-hosted, no-per-seat system of record in which audit trail, electronic signature
and record immutability are kernel properties a module author cannot forget, and in
which quality, genealogy, training and calibration are modules that can be switched
off for an unregulated shop without turning the kernel trail off. That is ProShop's
integrated ERP+QMS thesis, as open source, with Part 11 structural rather than
bolted. Until Phase 4 exists, that sentence is a promise. Until Phase 1 lots exist, it
is not a demo.

What is not a difference, and must not be restated as one:

- An inventory ledger from which quantity is derived. ERPNext has one
  (`research/audits/slice-competitive.md` §2.1). Wicket's append-only, zero-sum groups
  are an implementation bet against ERPNext's mutation and Bin desync, not a market
  wedge (same section, honesty test 1).
- A quality workspace. ERPNext, Odoo Enterprise and Tryton already ship one
  (`research/audits/slice-competitive.md` §C1).
- AGPL. ERPNext is GPL-3.0; Odoo Community is LGPLv3
  (`research/audits/slice-competitive.md` §C7).
- A ten-minute install. Frappe Docker and Odoo.sh exist; a 30-person implant shop's
  first question is whether the system survives an investigator
  (`research/audits/slice-competitive.md` §C8).
- CAD-to-estimate. Paperless Parts sells it to this buyer
  (`research/audits/slice-competitive.md` §C5).

---

## 5. Why projects in this category died, and what ADR 0006 does

Full case files: `research/background/open-source-governance.md`. The mechanism is
one mechanism with two triggers, not two patterns.

**A software license can only be changed by whoever holds the copyright.** A
Contributor License Agreement is how one company accumulates the rights to relicense
without asking the people who wrote the code (`research/background/open-source-governance.md`
§1). Open-core erosion and acquisition-kills-the-open-edition are that mechanism
firing. AGPL restrains everyone except the party that can change it (same section).

Confirmed deaths and completed relicensings, compressed:

| Project | What happened | What the community could do |
|---|---|---|
| Compiere | Single holder, then Consona, then Aptean. Brand gone; `compiere.com` redirects to Aptean (`research/background/open-source-governance.md` §3.1). | Fork GPLv2 (ADempiere). Could not keep name, channel, or maintainers. ADempiere then stalled (last commit 2023-12-11; `adempiere.net` dead) because the fork copied source and not decision-making. iDempiere survived by rebuilding governance. |
| Openbravo | Pivoted to retail; absorbed by Orisha. The company no longer sells ERP (`research/background/open-source-governance.md` §3.2). | Forked the POS. No ERP fork of meaning survived. |
| xTuple / PostBooks | Hybrid license from the start; acquired by CAI; GitHub repos 404; product continues closed (`research/background/open-source-governance.md` §3.3). | Almost nothing. The open edition was a lead-generation channel. |
| OpenERP → Odoo | CLA, single holder. v9 (October 2015): AGPL to LGPLv3, Quality / PLM / shop floor to OEEL (`research/background/open-source-governance.md` §3.4). | Form the OCA; reimplement modules; never catch the vendor's cadence. |
| SQL-Ledger | No acquisition. Founder paywalled docs, forums and fixes. CVE-2006-4244 sat ~a year (`research/background/open-source-governance.md` §3.5). | Fork (LedgerSMB). Paywalling the project to protect a services income produced a competitor from the user base. |
| Fedena, JFire, Neogia, opentaps, uniCenta | Open edition abandoned, vendor liquidated, or source moved behind a login while the LICENSE file stayed unchanged (`research/background/open-source-governance.md` §3.7–§3.12). | Permission is not a community. Nobody forked most of these. |
| Carbon | Eighteen months old; single corporate copyright; `packages/ee` already in the tree; internal production use conditional (`research/background/open-source-governance.md` §3.15; `research/background/competitive-landscape.md` §3). | The configuration that took Compiere fourteen years and Odoo nine was Carbon's starting position. |

Apache OFBiz is the other failure mode: ASF charter forbids proprietary relicensing,
and the project is feature-frozen with three active human committers and 19 CVEs in
2026 (`research/background/open-source-governance.md` §3.14). Perfect governance
protected the license and did not protect the project.

**What ADR 0006 does about this** (`docs/adr/0006-license.md`, Accepted 2026-09-12):

- **AGPL-3.0-or-later** for the code. A shop self-hosting for internal use triggers
  nothing. A vendor offering a hosted version must publish modifications. That is the
  network clause doing the work it can do.
- **Developer Certificate of Origin, no Contributor License Agreement.** Contributors
  keep copyright. Relicensing later requires every contributor. That is the Tryton
  defense (`research/background/open-source-governance.md` §1): refuse the CLA so that
  one party cannot change the terms.
- Open-core, with regulated modules paywalled, is rejected in the same ADR: the
  regulated modules are the point of the project.

**What ADR 0006 does not yet do.** Copyright is held by the project owner as an
individual until a foundation or company is deliberately chosen
(`docs/adr/0006-license.md` Decision). Concentrated copyright is the load-bearing
risk in every death above. Dispersed copyright is the defense; it is not in force
while one person holds the whole thing. The ADR names that choice as still open and
cheapest before outside contributions arrive. AGPL in one person's hands is a
business model, not a safeguard (`research/background/open-source-governance.md` §1).
A foundation between the vendor entity and the trademark is the second Tryton layer
and is not decided here.

ADR 0006's options section still attributes AGPL to ERPNext and to Odoo Community.
That attribution is false: ERPNext is GPL-3.0, Odoo Community is LGPLv3
(`research/audits/slice-competitive.md` §C7). The ADR's *decision* (AGPL + DCO) does
not depend on the attribution. This document does not treat it as fact. Fixing the
ADR is not this file's ownership.

---

## 6. What Wicket should not compete on

Per `docs/adr/0007-defer-general-ledger.md`, `docs/01-vision-and-scope.md` §5–§6, and
the research.

**Accounting.** Do not build a general ledger, accounts payable/receivable as books,
or period close. Every target shop already runs QuickBooks, Xero, or Sage and will
not migrate (`docs/adr/0007-defer-general-ledger.md`). Build `gl-export` (journal
export, mapping, reconciliation) and `ap-ar-lite` (invoices, three-way match, aging
as operational questions). A minimal general ledger is a trap: it is adequate until
the accountant asks for something it cannot do. Accounting is saturated, high
liability, and undifferentiated. Opportunity cost is a year of quality and
traceability work.

**CAD-to-quote.** Paperless Parts already owns CAD-to-estimate for this buyer
(`research/audits/slice-competitive.md` §6). Do not clone it in the core. Do not tell
a shop owner or a README reader that no ERP does this. Phase 7 `cad` may ingest STEP
to seed an estimate inside Wicket; the buying-criterion job is already served.
Integrate first.

**E-commerce.** Medusa and Bagisto are e-commerce and irrelevant to a 30-person
implant shop (`research/background/competitive-landscape.md` §5.2). Storefronts,
carts and customer storefront SEO are not the product. Customer-facing order status
belongs later in `portal` (Phase 7), which is not a store.

Also out, and already named in `docs/01-vision-and-scope.md` §6 and
`docs/04-module-catalog.md`: Design History File / design controls (that is PLM;
integrate); real-time machine control (that is MES; consume, do not control);
multi-tenant SaaS (`docs/adr/0008-single-tenant.md`); a general-purpose ERP core to
rival ERPNext and Odoo on breadth (`research/background/competitive-landscape.md` §8,
last paragraph).

---

*Sources: `research/background/competitive-landscape.md`,
`research/audits/slice-competitive.md`, `research/background/open-source-governance.md`,
`docs/01-vision-and-scope.md` §5, `docs/04-module-catalog.md`, `docs/adr/0004` through
`0008`.*
