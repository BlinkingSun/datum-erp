# Sweep 10 — Competitive honesty (adversarial)

Task: `erp` · Slice: PLAN-AUDIT competitive honesty · Date: 2026-09-11
Scope: current product reality 2025–2026. `docs/08-competitive-landscape.md` **does not exist**. This report is the input that lane is gated on. Do not copy `docs/01` §5 into it.

Sources were live-checked. GitHub `frappe/erpnext` `develop` as of this date still contains the cited SLE cancel/repost paths.

---

## 0. One-line verdict

**The differentiation claim in `docs/01` §5 is half-true and currently written as if it were all-true.** An append-only inventory ledger is **not** a market wedge against ERPNext. Kernel Part 11 + eQMS-in-ERP for 10–100 person device shops **is** a wedge, but it is Phase 4, not this kernel. CAD-native estimating is **not** an unfair advantage vs the market; Paperless Parts already owns CAD-to-estimate and is a Phase 7 module anyway. Kernel-only Waves 1–2 have **no demoable beachhead wedge**. Keep Phase 1 inventory+lots **in this build** (PLAN Wave 3 already says so — do not let §10 or the risk table be read as cutting it). Do **not** pull Phase 4 into this build.

---

## 1. True / false per claim

| # | Claim (paraphrase of `docs/01` §5 / §2 / architecture) | Verdict | Why |
|---|---|---|---|
| C1 | vs general OSS ERP: quality is first-class, not a bolt-on | **FALSE as uniqueness; PARTIAL as quality-of-quality** | ERPNext ships Quality Inspection, templates, incoming/in-process/outgoing gates, Non Conformance, Quality Procedure in **core** ([QI docs](https://docs.frappe.io/erpnext/quality-inspection), [NC docs](https://docs.frappe.io/erpnext/non-conformance)). Odoo **Enterprise** has a real Quality app (`quality_control` lives in `odoo/enterprise`). Odoo **Community** quality is OCA (`quality_control_oca`). Tryton has `trytond-quality` 8.0.1 (2026-07). Wicket’s quality is **deeper** (CAPA, DHR, training veto, calibration impact) — that is Phase 4, not a present fact. |
| C2 | Audit trail and e-sign are kernel, not add-on — and OSS ERPs got this wrong | **TRUE as architecture intent; FALSE as “every OSS ERP got it wrong”; FALSE as current uniqueness of *any* trail** | Frappe Version / `track_changes` is framework-level and opt-in per DocType, not grant-level append-only, and Version rows are **deletable** by Administrator ([version.json](https://github.com/frappe/frappe/blob/develop/frappe/core/doctype/version/version.json) grants `"delete": 1` to Administrator). Odoo CE has **no** built-in audit log; OCA `auditlog` is an add-on ([OCA/server-tools/auditlog](https://github.com/OCA/server-tools/tree/17.0/auditlog)). Odoo Sign is **Enterprise-only**. ERPNext Part 11 e-sign is **third-party marketplace** (DSC tokens for Indian tax PDFs, not 21 CFR 11). The *kernel, cannot-forget, grant-level, meaning-bound e-sign* design is a real difference. The sentence “every existing open source ERP got [this] wrong” is fundraising swagger, not a sourced claim. It lives in `docs/01` §2, not in `02-architecture.md`. |
| C3 | Lot/serial genealogy is a designed capability, not a report you write yourself | **FALSE as uniqueness; TRUE as graph-vs-report distinction** | ERPNext v16 ships **Serial No and Batch Traceability Report** with backward/forward walk across Stock Entries, Work Orders, Job Cards ([Zikpro/docs mirror of v16 report](https://zikpro.com/erpnextdocs/serial-no-and-batch-traceability-report/); community batch/serial docs 2025-06-28). Odoo CE and EE both have Lots/Serials + a **Traceability** button that is a chronological **stock.move.line table**, not a genealogy graph. Forum evidence: CE traceability does **not** always show component lots of a finished serial the way EE “upstream/downstream” does ([Odoo forum 119388](https://www.odoo.com/forum/help-1/is-upstreamdownstream-traceability-available-only-in-enterprise-edition-119388)). Wicket’s “graph over postings” is a better *design*; it is not a capability the market lacks as a *report*. |
| C4 | vs commercial regulated suites: OSS, self-hosted, no per-seat, validation pack ships with the release | **PARTIAL** | OSS + self-hosted + no per-seat is true vs Greenlight Guru / MasterControl / Qualio / Arena / Epicor / Infor. It is **not** true vs ERPNext (GPL-3.0, self-host, $0 license) or Odoo Community (LGPLv3, self-host, $0). Validation pack is Phase 6 `validation-pack` — not this build, and not yet written. Commercial eQMS is $5k–$60k+/year for this beachhead (see §5). That price gap is real **if and only if** Phase 4 exists. |
| C5 | CAD-native ERP that ingests STEP to seed an estimate is something no ERP on the market does well; unfair advantage | **FALSE as stated; TRUE only if scoped to “no general OSS ERP does this natively”** | **Paperless Parts already owns CAD-to-estimate** for job shops and medical-device contract manufacturers: geometry interrogation of CAD, BOM extraction from assemblies, routing templates including FAI/traceability cost, integrations into JobBOSS, ProShop, Global Shop, Epicor, Infor Visual/SyteLine ([medical page](https://www.paperlessparts.com/medical-devices/), [JobBOSS case](https://www.paperlessparts.com/case-studies/sweeney-metal-fabricators-grows-revenue-saves-8-hours-every-week-with-paperless-parts-and-jobboss-integration/), [ProShop integration 2021](https://www.businesswire.com/news/home/20211007005938/en/Paperless-Parts-Integrates-With-ProShop-to-Enhance-Its-Functionality-and-Help-Manufacturers-Quote-Faster)). aPriori / Boothroyd Dewhurst DFMA ingest STEP/IGES. CalcuQuote is EMS/BOM quoting, not STEP-to-machine-time. CAD-in-ERP as a **module of the system of record** is still rare; as a **buying criterion** it is already served. Catalog places this in **Phase 7**. Naming it in §5 as “the unfair advantage” will leak into README. |
| C6 | “Every existing open source ERP got [audit trails and immutability] wrong” | **FALSE** | Overclaim. ERPNext has an inventory ledger (`tabStock Ledger Entry`) from which qty is derived; Bin is a cache that is *supposed* to rebuild from SLE. Frappe Version is a change log. Tryton has an audit-trail tradition and period control. What they got wrong, *specifically*, is: (a) trails are opt-in / module-level / deletable, (b) SLE rows are **mutated** on cancel and repost, (c) e-sign is not a kernel primitive bound to record hash + meaning, (d) master-data audit is not 21 CFR 11.10(e) grade. Write that. Do not write “got it wrong.” |
| C7 | AGPL is what ERPNext and Odoo Community use (ADR 0006) — therefore no license wedge | **FALSE on the facts; TRUE on the conclusion** | **ERPNext is GPL-3.0**, not AGPL ([github.com/frappe/erpnext](https://github.com/frappe/erpnext) license metadata; [commit correcting GPLv3 text](https://github.com/frappe/erpnext/commit/a30f38481de3df4350ca67e6a1529a5203b26bec)). **Frappe framework is MIT**. **Odoo Community is LGPLv3** since Odoo 9 (2015); Odoo 8 was AGPL. ADR 0006’s sentence “What ERPNext and Odoo Community use” is **wrong** and must be fixed before it is copied. Conclusion still holds: there is **no license wedge**. If anything LGPL (Odoo CE) is *more* adoption-friendly than Wicket’s proposed AGPL. |
| C8 | Ten-minute install, one binary, is a beachhead buying criterion | **FALSE for the named beachhead; TRUE as a hygiene goal for the unregulated adjacent** | ERPNext: `frappe_docker` `pwd.yml` is a few-minute demo ([frappe/frappe_docker](https://github.com/frappe/frappe_docker)); Frappe Cloud is a form. Odoo: Odoo.sh / Odoo Online / Docker. A 30-person implant shop’s actual first question is “will this survive an FDA investigator / ISO 13485 surveillance,” not “can I double-click it.” Success test 1 in `docs/01` §7 is still a good engineering bar. It is not the wedge. |
| C9 | The append-only ledger is the riskiest **market** bet | **FALSE.** It is the riskiest **implementation** bet. | Honesty test 1. See §2. |
| C10 | This kernel-only build has a demoable wedge | **FALSE** | Honesty test 2. Genealogy mockup cannot be backed by data until Phase 1 lots exist. Quality screens cannot be hidden until they exist. Part 11 cannot be shown to an investigator without records that look like production. |

---

## 2. ERPNext / Frappe — current product reality

### 2.1 Stock Ledger Entry is a ledger. It is not Wicket’s ledger.

**Yes, qty is derived from `tabStock Ledger Entry`.** Official stock-ledger docs: “Inward or outward transactions … are recorded in the Stock Ledger which then is reflected in the Stock Ledger Report” ([docs.frappe.io/erpnext/stock-ledger](https://docs.frappe.io/erpnext/stock-ledger)). Bin is explicitly a cache of the last SLE:

```python
# erpnext/stock/doctype/bin/bin.py (develop)
last_sle = get_last_sle_values(self.item_code, self.warehouse)
self.actual_qty = last_sle.qty_after_transaction
```

`get_last_sle_values` reads `qty_after_transaction` from SLE where `is_cancelled = 0`.

**No, it is not append-only, not zero-sum, and not immutable.**

1. **Cancel mutates history.** `set_as_cancel` in [`erpnext/stock/stock_ledger.py`](https://github.com/frappe/erpnext/blob/develop/erpnext/stock/stock_ledger.py) does `UPDATE tabStock Ledger Entry SET is_cancelled=1, modified=..., modified_by=...`. Then `make_sl_entries` **also inserts new SLE rows with negated `actual_qty`**. Original rows stay, flagged. Wicket ADR 0004 forbids this class of mutation.

2. **Repost rewrites the posting itself.** `process_sle` ends with `frappe.get_doc(sle).db_update()` after recomputing `qty_after_transaction`, `valuation_rate`, `stock_value`, `stock_queue`, `stock_value_difference`. `update_qty_in_future_sle` runs `UPDATE tabStock Ledger Entry SET qty_after_transaction = qty_after_transaction + {qty_shift}` on **future** rows. Running balances live **on the posting** and are rewritten. Wicket stores no running balance on the posting.

3. **Bin desyncs from the ledger in production.** GitHub [erpnext#54528](https://github.com/frappe/erpnext/issues/54528) (2026-04-25, v15.105.0): cancel a Purchase Receipt, SLE balance goes to 0, Bin still shows qty 2 (“ghost stock”). This is *exactly* the failure mode ADR 0004 exists to prevent — “the update path and the logging path are different code.” ERPNext already *intends* ledger-as-truth and still ships the bug.

4. **Concurrent submits corrupt `qty_after_transaction`.** [erpnext#51562](https://github.com/frappe/erpnext/issues/51562) (2026-01-07, v15.93.0): two same-timestamp transactions, no repost created, wrong running balances. Develop now has `sle_processing_gate` (Postgres advisory lock) to serialize (item, warehouse) writers — an admission that the previous design raced.

5. **No zero-sum group, no virtual locations as a constraint.** A receipt is `actual_qty = +N` at a warehouse, not a balanced transfer from `SUPPLIER`. Transfers are two SLE rows (source negative, target positive) by convention, not a deferred DB constraint on a posting group.

**Honesty test 1, answered:** ERPNext already has “the inventory ledger from which qty is derived, with a rebuildable cache.” Wicket is **catching up to the thesis** and **trying to execute it without ERPNext’s mutation/repost/desync**. That is an **implementation bet** (can you do the thing they already do, without the bugs they still have in 2026). It is **not a market bet**. A 30-person implant shop does not buy “deferred zero-sum groups.” They buy “show me the DHR for serial X.” PLAN’s risk row “Ledger design is wrong, and everything depends on it” is correctly an *engineering* risk. Catalog’s “riskiest bet in the architecture” is correct **if read as engineering**. `docs/01` §5 currently lets a reader infer it is a *competitive* bet. That inference is false.

### 2.2 Batch / serial

First-class on the Item (`Has Batch No` / `Has Serial No`). Serial-and-batch bundle is the modern path. Traceability is a **report**, plus stock ledger filtered by batch. Community claim “helps meet [pharma/food] regulations without third-party tools” ([PowerSoft, 2025-06-28](https://www.powersoftsystem.com/post/batch-serial-number-tracking-in-erpnext)) is marketing; it is lot tracking, not 21 CFR 11 / ISO 13485.

Cannot enable both batch and serial on one item in older versions; v15+ serial-and-batch bundle relaxes this. Genealogy as a **graph of what went into what, including operator, gage, inspection** is not a core object. Wicket catalog `genealogy` + `dhr` is the actual gap.

### 2.3 Quality module

Core, not a paid add-on.

- Quality Inspection: incoming / outgoing / in-process; templates; numeric / attribute / formula; can **block submit** of receipt/delivery/job card ([docs](https://docs.frappe.io/erpnext/quality-inspection)).
- Quality Procedure, Quality Goal, Quality Meeting, Quality Feedback, Quality Action.
- Non Conformance: “an observation … against a Quality Procedure,” free-text CAPA fields, status ([docs](https://docs.frappe.io/erpnext/non-conformance)). This is **not** an NCR with containment, MRB disposition (use-as-is / rework / scrap / RTV), lot hold, or signature. It is a quality-meeting notebook.
- No first-class CAPA doctype with effectiveness check. No DHR assembler. No training-veto on Job Card. No calibration-out-of-tolerance impact list.

Frappe forum 2025-06-02, partner answer on “QMS for a Medical Device Company”: ERPNext QC is “suitable … with important caveats”; **not out-of-the-box ISO 13485 or 21 CFR Part 11** ([discuss.frappe.io/t/148023](https://discuss.frappe.io/t/qms-for-a-medical-device-company/148023)). That is the honest competitor sentence.

### 2.4 Manufacturing: Job Card / Work Order

Real. Work Order, Job Card, BOM, routing, subcontracting, shop-floor. Adequate table stakes. Not a wedge either way.

### 2.5 Healthcare vs medical-device

**Healthcare is clinical, not device manufacturing.** Split out of ERPNext into **Marley Health** (patients, appointments, encounters, IPD, lab). Docs: “clinic, hospital, diagnostic center” ([docs.frappe.io/erpnext/frappe-healthcare](https://docs.frappe.io/erpnext/frappe-healthcare)). Citing “ERPNext has healthcare” as covering the beachhead is a category error. Third-party “SigzenPHARMA” / “Medical Compliance Module” apps claim GMP / Part 11 on top of ERPNext; they are **partner products**, not core.

### 2.6 Electronic signature

Not a kernel primitive. Marketplace:

- `e_sign` — hardware DSC USB token, PAdES, **Indian CCA trust store** ([Frappe Cloud marketplace](https://cloud.frappe.io/marketplace/apps/e_sign)).
- `digital_signer` — PFX/USB sign of Sales/Purchase PDFs ([marketplace](https://cloud.frappe.io/marketplace/apps/digital_signer)).

These are **PKI signatures on PDFs**, mostly for tax/invoice law, not 21 CFR 11.200 (two identification components, meaning of the signature, bound to the record so the signature cannot be excised). Workflow “approve” in Frappe is a permissioned state change, not an e-sign.

### 2.7 Audit trail on master data

- **Version** (`tabVersion`): JSON diff of changed/added/removed fields when DocType `track_changes = 1`. Opt-in per DocType. Not produced by a persistence interceptor that a module author cannot skip. Administrator may **delete** Version rows.
- **Activity Log / comments / timeline**: social, not 11.10(e).
- **Transaction Log** DocType (France/Germany regional) was **removed** in 2025 ([frappe#33844](https://github.com/frappe/frappe/pull/33844)); even its authors said it was insufficient for French legal requirements.
- Submitted documents (`docstatus=1`) cannot be edited in place; cancel + amend is the path. That is closer to immutability **for transactions**, and it is **not** applied to Item / BOM / Routing master data, which overwrite.

### 2.8 21 CFR 11 claims

Core ERPNext does **not** claim Part 11. Partner apps do (SigzenPHARMA “21 CFR Part 11 Ready”). Treat those as unverified marketing until a 510(k) holder’s validation package is public. Wicket should not compete with that sentence; it should compete with “audit produced by the persistence layer, e-sign bound to record hash, grant-level append-only.”

### 2.9 Install

Not a differentiator. `frappe_docker` pwd.yml, Frappe Cloud, bench. Heavier than “one binary,” lighter than “we are the only people who can stand this up.”

---

## 3. Odoo Community vs Enterprise

| Capability | Community 19 (2025–26) | Enterprise 19 | Implication for Wicket |
|---|---|---|---|
| MRP (BOM, MO, work orders, work centers) | Yes | Yes + MPS, tablet shop floor, barcode, IoT | Table stakes |
| Lots / serials | Yes, Inventory settings | Same | Table stakes |
| Traceability | Chronological move table on the lot; CE users report missing component-lot walk that EE “upstream/downstream” shows | Stronger upstream/downstream | Report, not graph |
| Quality | **No.** Paid Enterprise `quality_control` / `quality_mrp` / `quality_stock`. CE substitute: OCA `quality_control_oca` + `quality_control_stock_oca` + `quality_control_mrp_oca` ([OCA/manufacture](https://github.com/OCA/manufacture)) | Control points at receipt / WO / picking; alerts; quarantine | **Unregulated shop on CE never sees quality because it is not installed.** That is not “hide quality screens”; it is “you did not buy the SKU.” Wicket’s claim (same binary, modules off, kernel audit still on) is the actual difference vs both editions. |
| Sign | **No** | **Yes**, Odoo Sign | E-sign is a paid document product, not a kernel primitive on every state transition |
| Studio | No | Yes | Customization path |
| Audit log | **No** in core. OCA `auditlog` (create/read/write/delete rules, not grant-level, not automatic) | Better field tracking in EE marketing; still not kernel | Matches Wicket’s “add-on trail is how you get a partial trail” critique **for CE**. EE is better and still not 11.10(e). |
| License | **LGPLv3** | Proprietary OEEL + per-user | CE is *more* permissively licensed than Wicket’s AGPL recommendation |
| Hosting | Self-host | Self-host, Odoo.sh, Odoo Online | Odoo.sh is the “ten-minute” answer for buyers who will pay |

**Can an unregulated shop hide quality screens?**
- Odoo CE: quality is absent. Hidden by not existing.
- Odoo EE: do not install Quality. Hidden by SKU.
- ERPNext: do not open the Quality workspace. Hidden by navigation, not by kernel policy.
- Wicket’s success test 6 (“never encounters a quality screen”) is only a wedge if **kernel audit/e-sign remain on while quality modules are off**, and the UI actually suppresses regulated navigation. That is a product requirement, not a slogan. Waves 1–2 cannot demonstrate it.

**Odoo stock valuation:** historically `stock.valuation.layer` (append-mostly cost layers; remaining_qty is **updated**). A 2025–26 refactor PR ([odoo/odoo#222169](https://github.com/odoo/odoo/pull/222169)) proposes **removing** SVL and storing value on `stock.move`. Qty on hand is **not** “sum of immutable postings with zero-sum groups.” Odoo is not a ledger-thesis competitor; it is a move-table competitor.

---

## 4. Other OSS (only if manufacturing + quality)

| Product | Manufacturing + quality story | Beachhead threat |
|---|---|---|
| **Tryton** | Production + stock lots + official `trytond-quality` 8.0 (control points, inspections; deletion of non-pending inspections blocked since 7.4). Accounting rigor. Tiny ecosystem. | Low. A technically tasteful also-ran. Mention in `docs/08`; do not fear it. |
| **metasfresh** | MRP, lots, quality UI; DACH food/pharma references. | Low in US device shops. |
| **iDempiere** | Libero MFG, lot/serial, CQA plugin, audit-trail plugin. Java/OSGi. No native e-sign. | Low. Too heavy, too old-Compiere. |
| **Apache OFBiz** | Framework, not an app. “Needs heavy config.” | None. |
| **Dolibarr** | Light production. Quality is a **€180 Dolistore add-on** ported from old OCA quality_control. | None for ISO 13485. |

None of these steal the beachhead. Do not spend PLAN risk budget on them.

---

## 5. Where a 30-person implant shop actually spends money today

This is the competitive set `docs/08` must lead with. Not Odoo vs SAP.

### 5.1 Pattern (matches `docs/01` §3 almost exactly — this part of the vision is honest)

Typical stack, all of them bad:

1. **Job shop ERP** (JobBOSS² / E2, Global Shop, ProShop, sometimes Epicor Kinetic or Infor Visual) **plus**
2. **eQMS** (Greenlight Guru, Qualio, QT9, MasterControl) **plus**
3. **Spreadsheets** to reconcile lot/DHR/training **plus**
4. often **Paperless Parts** for quoting.

That seam is the product. Keep §4 of `docs/01`. Tighten §5.

### 5.2 Shop ERPs they will demo

| Product | Why they look | Why they lose / win vs Wicket-as- pitched | Price signal (2026, third-party) |
|---|---|---|---|
| **ProShop** | **Closest competitor.** ERP+MES+QMS, paperless, machine-shop native, claims ISO 13485 + 21 CFR 11, document control, NCR, training, AS9100. Built on a shop floor. | Closed, per-employee pricing, not OSS, vendor holds the records. **This is the product Wicket is trying to be, as open source.** If `docs/08` does not put ProShop in paragraph 1, it is the wrong document. | Quote; third-party ~$500–$1,800/mo for 5–30 heads ([Softwarefinder 2026-09](https://softwarefinder.com/enterprise-resource-planning-software/proshop-erp/pricing)); priced on **headcount not seats** |
| **JobBOSS²** (ECI; JobBOSS + E2 merged) | Largest installed base in small US machine shops. Quoting, jobs, inventory, some quality. | Quality/Part 11 is not why you buy it. Shops add an eQMS. | ~$3k/year entry to ~$200/user/mo |
| **Global Shop** | Custom fab + machining, 10-user minimum | Same pattern: operations yes, eQMS no | ~$1,500/mo at 10 users |
| **Epicor Kinetic** | Real medical-device ERP story 2026 (traceability, CAPA, eMDR) ([Epicor blog](https://www.epicor.com/en-us/blog/technology-and-data/medical-device-erp-in-2026-traceability-capa-and-compliance-readiness/)) | Priced for companies 3× the beachhead. Implementation measured in years. | Quote, well into the $40k–$150k/yr band `docs/01` names |
| **Infor SyteLine / Visual** | Legacy discrete; Visual still in job shops | Same | Quote |
| **E2** | Now inside JobBOSS². Quality “core strength” in analyst blurbs; not Part 11. | | ~$45/user/mo |
| **MRPeasy** | Cheap cloud MRP | No eQMS, no Part 11 | $49/user/mo |
| **Katana** | Inventory-first, light manufacturing | Unregulated | ~$179/mo+ |
| **Cetec** | Electronics CM; pairs with CalcuQuote | Wrong vertical unless the shop is EMS | $50/user/mo |
| **Fishbowl** | QuickBooks inventory | Not regulated manufacturing | Quote |

### 5.3 eQMS they will demo (this is the money)

| Product | Role | 2026 price signal | Threat |
|---|---|---|---|
| **Qualio** | Default small-team life-sciences eQMS | ~$5k–$15k/yr (some reports ~$12k+; price hikes 2025–26) | High. Fast to stand up. No ERP. |
| **QT9** | Mid-market, claims manufacturing integration, Part 11, concurrent licensing | ~$1,200/org/mo or ~$120/user/mo; 10-user 1st year $21k–$53k | High. Closest “we do QMS and a bit of manufacturing.” |
| **Greenlight Guru** | Med-device-only, design controls + QMS | ~$25k–$60k/yr; $600/user/mo anecdotes; 2026 package-separation price shock | High for **design-control** shops. Weak for **contract manufacturers** who do not own the DHF. Wicket correctly defers DHF to PLM. |
| **MasterControl** | Enterprise life sciences | $25k+ basic, 4× others | Low at 30 people |
| **Arena (PTC)** | PLM + QMS | $60k–$150k+ | Low at 30 people; relevant if they already live in Arena for BOM |

`docs/01` §3 item 3 (“Paying $40,000 to $150,000 a year for an integrated commercial suite”) is the **Epicor/Infor/MasterControl** band. The **more common** 30-person spend is **JobBOSS ~few k + Qualio/QT9/Greenlight $10–40k + Paperless Parts**. Total still five figures. The vision’s four-bad-options list should name that combo explicitly.

---

## 6. CAD-in-ERP

| Product | What it actually does | Relationship to Wicket `cad` module |
|---|---|---|
| **Paperless Parts** | Ingests CAD (including assemblies), extracts geometry and BOM tree, costs operations, flags unmanufacturable features, medical-device routing templates (FAI, sampling, traceability). **Not an ERP.** Pushes into JobBOSS / ProShop / GSS / Epicor / Infor. | **Owns the “CAD to estimate” job.** Wicket Phase 7 would be a late, in-ERP clone of a product shops already buy. |
| **ProShop** | Has CAD viewing / some integration; **bought Paperless Parts integration rather than building ingest.** | Evidence that even the closest ERP competitor does not treat CAD-ingest as core. |
| **CalcuQuote** | EMS quoting from **BOMs / Gerbers**, live component pricing. Not STEP-to-cycle-time. | Irrelevant to machined implants. |
| **aPriori, DFMA** | CAD-driven should-cost for OEMs. | Different buyer. |
| **CAD2Quote / generic “quote from STEP” tools** | Exist as point solutions. | Same as Paperless Parts, smaller. |

**Edit required:** `docs/01` §5 “One unfair advantage worth naming” must be demoted to “a later-phase module the founder is unusually qualified to build; Paperless Parts already serves the quoting job and we should integrate before we clone.”

---

## 7. Honesty tests (required)

### Test 1 — ERPNext ledger ⇒ PLAN’s “riskiest bet” is implementation, not market

**Pass this test: say it in PLAN and in `docs/01`.**

ERPNext already has `tabStock Ledger Entry` as the inventory journal and Bin as a (buggy) cache. Odoo has moves + (historically) valuation layers. The market already accepted “inventory is a ledger of movements.” Wicket’s novelties are (i) **true immutability** (no `is_cancelled` update, no `db_update` of qty_after_transaction), (ii) **zero-sum groups with virtual locations**, (iii) **one engine for inventory, cost, and labor**, (iv) **genealogy as graph over those postings**. (i)–(iii) are how you avoid ERPNext’s 2026 ghost-stock and concurrent-qty bugs. They are not why a shop switches. (iv) is why a shop switches, and (iv) needs Phase 1 lots + Phase 3 production + Phase 4 genealogy.

**PLAN risk table today:** “Ledger design is wrong, and everything depends on it → Built first, deep audit, property tests. Wave 2 does not close until the ledger passes.”

That response de-risks the **engine**. It does not de-risk the **domain**. Catalog is explicit: “Phase 1 exists in that position for a specific reason. The append-only ledger is the riskiest bet … and **inventory is the domain that exercises it hardest**.”

### Test 2 — Real wedge is kernel Part 11 + eQMS-in-ERP; Phase 4 is the product; Waves 1–2 are table stakes; kernel-only is not demoable

**Yes.** Closing the ERP/eQMS seam (`docs/01` §4) is the only claim that survives contact with ProShop + Qualio + ERPNext. That seam is Phase 4 modules (`doc-control`, `inspection`, `ncr`, `capa`, `genealogy`, `calibration`, `training`, `change-control`, `dhr`, `dmr`) on a Phase 0 kernel, fed by Phase 1 lots and Phase 3 production.

This build (PLAN §10: Phase 2+ out of scope; Waves 1–2 kernel; Wave 3 = Phase 1 modules + shells):

- **Can** demonstrate: audit interceptor, grant-level append-only, e-sign bound to hash, ledger postings that balance, maybe a stock move of a lot.
- **Cannot** demonstrate: DHR, training veto, investigator-grade genealogy, “unregulated shop never sees quality,” mock FDA inspection (success test 4).

Genealogy **mockup** in the UI gate without Phase 1 lots is a picture of a product you have not built. Do not let `doc-repo` README claim it as shipping capability.

### Test 3 — AGPL vs ERPNext/Odoo also AGPL — no license wedge

**Correct conclusion, wrong premise.** See C7. Fix ADR 0006. There is still no license wedge. AGPL vs GPL vs LGPL is a contributor/legal discussion, not a buyer discussion at 30 people (they self-host; network clause does not bind them).

### Test 4 — Ten-minute install vs “will it pass my FDA audit”

**Install is not the beachhead buying criterion.** Keep the engineering goal. Remove it from the “what makes this different” section, or rank it last and label it as the unregulated-adjacent convenience, not the medical-device reason-to-buy.

---

## 8. The actual wedge (one paragraph)

A 10–100 person ISO 13485 / FDA-registered machine shop making Class I–II implants or instruments today runs a job-shop ERP (very often JobBOSS² or ProShop) and a separate eQMS (Qualio, QT9, or Greenlight Guru) and a quoting tool (Paperless Parts) and spreadsheets across the seam; the auditor’s questions all span that seam. Wicket’s only durable difference is a **self-hosted, no-per-seat system of record in which audit trail, electronic signature, and record immutability are kernel properties that a module author cannot forget, and in which quality, genealogy, training, and calibration are modules that can be switched off for an unregulated shop without turning the kernel trail off.** That is not “we invented the inventory ledger” (ERPNext has one), not “we are AGPL” (Odoo CE is LGPL, ERPNext is GPL), not “ten-minute binary” (Frappe Docker and Odoo.sh exist), and not “CAD-native estimating” (Paperless Parts already sells that to this exact buyer). It is “ProShop’s integrated ERP+QMS thesis, as open source, with Part 11 structural rather than bolted, at Qualio money instead of Epicor money.” Until Phase 4 exists, that sentence is a promise. Until Phase 1 lots exist, it is not even a demo.

---

## 9. PLAN implication

**Catalog vs PLAN is a wording collision, not a secret scope cut — unless someone reads it as one.**

| Document | What it says | How it will be misread |
|---|---|---|
| Catalog Phase 1 | Exists **to de-risk the ledger** in the domain that exercises it | Inventory+lots are load-bearing for the architecture bet |
| PLAN §9 risk | “Waves 1 and 2 are kernel only. No module … until the kernel passes its gate.” | This build is kernel-only |
| PLAN §3 Wave 3 | Covers `wicket-server`, **Phase 1 modules**, web shell, Tauri | Phase 1 **is** in this build |
| PLAN §10 | “Everything in `docs/04` **from Phase 2 onward**” is out of scope | Phase 1 is in; Phase 4 (the wedge) is out |

**Does kernel-only Wave 1–2 still make sense?** As a **gate**, yes: do not fan out thirteen crates on a ledger that cannot balance in property tests. As a **product increment**, no: Wave 2 passing is not “ledger de-risked.” ERPNext’s SLE also balances on the happy path and still desyncs Bin on cancel. The bugs that kill the thesis live at the **item × location × lot × backdated correction** boundary.

**Must Phase 1 inventory+lots land in this build?** **Yes.** PLAN Wave 3 already includes them. Opus should **not** re-scope *up* to Phase 4. Opus should **not** re-scope *down* by treating Wave 3 as optional. Make Phase 1 (`items`, `locations`, `inventory`, `lots`; `valuation` can slip if something must) a **named acceptance gate of this foundation build**, not an afterthought behind a UI gate that might slip.

**Does Wave 3 Phase 1 “exercise the ledger” enough without production?** Partially. Receipts, issues, moves, adjustments, cycle counts, and lot identity are the minimum that exposes virtual locations (`SUPPLIER`, `SCRAP`, `ADJUSTMENT`) and reversing postings. WIP-as-location and genealogy-as-graph need Phase 3. That is acceptable **if** the risk table stops claiming Wave 2 closed the ledger bet.

**Contradiction to flag for the master auditor:** PLAN §10 excludes Phase 2+ (correct — do not build sales/production/quality in the foundation). Catalog says Phase 1 is how you find out the ledger is wrong (correct). PLAN §9 “Waves 1 and 2 are kernel only” is a sequencing sentence that will be quoted as a scope sentence. Fix the sentence.

---

## 10. Recommended sentence-level edits

### 10.1 `docs/01` §2 (the “got wrong” line)

**Delete or replace:**

> “This is the one thing that cannot be retrofitted and the one thing every existing open source ERP got wrong.”

**With:**

> “This is the one thing that cannot be retrofitted. Existing open source ERPs record history — ERPNext’s Stock Ledger Entry is a real inventory journal; Frappe Version and Odoo’s OCA auditlog record field diffs — but the trail is opt-in or module-level, cancellations mutate ledger rows, running balances are stored and rewritten, and electronic signature is not a kernel primitive. A mostly-complete trail is the failure mode. Putting audit, signature, and immutability in the kernel is how we refuse that failure mode.”

### 10.2 `docs/01` §5 — replace the whole short version

`docs/08` does not exist. §5 currently pretends it does. Until it does, §5 must not overclaim.

**Replace the four blocks with:**

> A fuller competitive assessment will live in `08-competitive-landscape.md` after the landscape spike. Until that file exists, these are the claims we are willing to defend:
>
> **Against general open source ERP (ERPNext, Odoo Community).** They already track stock as a ledger of movements, already do lot/serial, already inspect incoming material, already print a traceability report. They do not make audit trail and electronic signature inescapable kernel properties; they do not assemble a Device History Record from the same postings; they do not veto an unqualified operator at clock-on. Quality in ERPNext is a first-class *workspace* with a thin Non Conformance form. Quality in Odoo is an Enterprise SKU (or an OCA add-on). Our difference is structural completeness for 21 CFR 11 and ISO 13485, not the existence of a stock ledger.
>
> **Against commercial regulated suites and the usual two-system stack.** The 30-person implant shop’s real alternatives are ProShop (integrated, closed, per-head) or JobBOSS² plus Qualio/QT9/Greenlight Guru (the seam this project exists to close). Open source, self-hosted, no per-seat, and a validation package that ships with the release are the commercial terms. They are not a reason to skip building the quality modules.
>
> **Against building it yourself in spreadsheets.** It survives the audit. That sentence is only true after Phase 4.
>
> **CAD-native estimating is a later-phase module, not the wedge.** Paperless Parts already turns CAD into a costed estimate for this buyer and already integrates with the ERPs they run. We should not make the core hostile to a future `cad` module. We should not tell funders or README readers that no one does this.

### 10.3 `docs/01` §3 item 3

Add the common case:

> “3. Paying five figures a year for a job-shop ERP and another five figures for an eQMS, plus a quoting tool, and reconciling them by hand. Or paying $40,000 to $150,000 a year for an integrated commercial suite sized for a larger plant.”

### 10.4 PLAN §9 risk table — replace the ledger row and add two

| Risk | Response |
|---|---|
| Ledger **engine** is wrong (groups do not balance, projections drift) | Wave 2 property tests. Wave 2 does not close until they pass. This is an implementation bet; ERPNext already has a movement ledger. |
| Ledger **domain** is wrong (lots, virtual locations, reversing a receipt, backdated correction, Bin-class desync) | **Phase 1 inventory+lots in Wave 3 of this build are the de-risk.** Catalog is explicit. Do not declare the ledger bet won at Wave 2. |
| Differentiation copy in `docs/01` §5 overclaims vs ERPNext/Odoo/Paperless Parts | `doc-landscape` must not paste §5. Gate on `spike-landscape.md` **and** this sweep. `doc-repo` README must not claim CAD-to-estimate, Part 11 completeness, or “no OSS ERP has a stock ledger.” |
| Scope explodes across the catalog | Waves 1–2 kernel. Wave 3 = Phase 1 only. Phase 4 is the product and is **not** this build. |
| Kernel-only is mistaken for a demoable beachhead wedge | It is not. Do not schedule customer demos, mock FDA inspections, or fundraising one-pagers on Wave 2 output. |

### 10.5 PLAN §10 — one clarifying sentence

After “Everything in `docs/04-module-catalog.md` from Phase 2 onward”:

> “Phase 1 (`items`, `locations`, `inventory`, `lots`, and if capacity allows `valuation`) is **in** this build, in Wave 3, because that is how the ledger is exercised. Kernel-only is a gate, not the deliverable.”

### 10.6 ADR 0006 — factual fix (not this slice’s file, but it will leak)

Replace “What ERPNext and Odoo Community use” with: “ERPNext is GPL-3.0 (Frappe framework MIT). Odoo Community is LGPLv3. AGPL is a stricter network-copyleft than either, closer in *spirit* to ERPNext’s copyleft than to Odoo CE.”

---

## 11. EXECUTOR / TIER / opus re-scope

**`doc-landscape` (`docs/08-competitive-landscape.md`)**
- **EXECUTOR:** grok or cursor, **gated on spike** — PLAN already gates this on `_team/reports/spike-landscape.md`. **Keep the gate.** Also require this file (`sweep-plan-competitive.md`) as a MUST-READ so the lane cannot paste `docs/01` §5.
- **TIER:** **deep**, not standard. False sentences here become README and pitch. Standard prose audit will not catch “ERPNext has no ledger.”
- **Must include:** ProShop as primary commercial analogue; JobBOSS²+Qualio as the common two-system stack; ERPNext SLE sourced from `stock_ledger.py` (append-mostly, not append-only); Odoo Quality = Enterprise; Paperless Parts owns CAD-to-quote; ADR 0006 license facts; explicit “what is not a wedge.”
- **Must not include:** “every OSS ERP got immutability wrong”; “no ERP ingests STEP”; “AGPL is what they use.”

**`doc-repo` (README)**
- **TIER:** standard, but with a **conformance check against the rewritten §5**, not the current one. If `doc-repo` runs in parallel with `doc-landscape`, it **will** copy the bad paragraph. Sequence: rewrite §5 (or give `doc-repo` this report) before README.

**Opus re-scope this build to include Phase 1 inventory?**
- **Phase 1: YES — it is already in Wave 3; make that unloadable.** `lots` is “not optional and not deferrable” per catalog; do not let Wave 3 ship `items` without `lots`.
- **Phase 4: NO.** Do not pull eQMS modules into the foundation build. The wedge stays a promise. That is acceptable if README and §5 stop talking as if it had shipped.
- **Phase 7 CAD: NO.** Integrate-later. Do not staff it.
- **Do not cut Wave 3** to “kernel only” in the name of scope control. That would leave the architecture’s actual risky surface (inventory ledger + lots) unexercised and the UI genealogy mockup as fiction.

---

## 12. URL index (live-checked 2026-09-11)

- ERPNext SLE source: https://github.com/frappe/erpnext/blob/develop/erpnext/stock/stock_ledger.py
- ERPNext SLE DocType: https://github.com/frappe/erpnext/blob/develop/erpnext/stock/doctype/stock_ledger_entry/stock_ledger_entry.json
- ERPNext Bin cache: https://github.com/frappe/erpnext/blob/develop/erpnext/stock/doctype/bin/bin.py
- SLE docs: https://docs.frappe.io/erpnext/stock-ledger
- Ghost stock Bin desync: https://github.com/frappe/erpnext/issues/54528
- Concurrent qty_after_transaction: https://github.com/frappe/erpnext/issues/51562
- Quality Inspection: https://docs.frappe.io/erpnext/quality-inspection
- Non Conformance: https://docs.frappe.io/erpnext/non-conformance
- Medical-device QMS caveats: https://discuss.frappe.io/t/qms-for-a-medical-device-company/148023
- Healthcare ≠ device: https://docs.frappe.io/erpnext/frappe-healthcare
- Frappe Version: https://github.com/frappe/frappe/blob/develop/frappe/core/doctype/version/version.json
- Transaction Log removed: https://github.com/frappe/frappe/pull/33844
- ERPNext license GPL-3.0: https://github.com/frappe/erpnext
- frappe_docker: https://github.com/frappe/frappe_docker
- OCA auditlog: https://github.com/OCA/server-tools/tree/17.0/auditlog
- OCA quality: https://github.com/OCA/manufacture
- Odoo CE vs EE quality/sign: https://www.techultrasolutions.com/compare/odoo-community-vs-enterprise
- Odoo CE traceability vs EE upstream: https://www.odoo.com/forum/help-1/is-upstreamdownstream-traceability-available-only-in-enterprise-edition-119388
- Tryton quality: https://pypi.org/project/trytond-quality/
- Paperless Parts medical: https://www.paperlessparts.com/medical-devices/
- Paperless Parts × ProShop: https://www.businesswire.com/news/home/20211007005938/en/Paperless-Parts-Integrates-With-ProShop-to-Enhance-Its-Functionality-and-Help-Manufacturers-Quote-Faster
- ProShop QMS: https://get.proshoperp.com/qms-for-manufacturers
- Epicor medical 2026: https://www.epicor.com/en-us/blog/technology-and-data/medical-device-erp-in-2026-traceability-capa-and-compliance-readiness/
- eQMS 2026 comparison: https://meddeviceguide.com/blog/best-eqms-software-medical-devices-2026-guide

---

## 13. What this slice did not do

- Did not install ERPNext or Odoo and click through. Claims about EE-only upstream/downstream rest on forum + partner docs, not a live EE trial.
- Did not obtain vendor quotes. Prices are third-party 2026 ranges; treat as order-of-magnitude.
- Did not read a Spike landscape report — `spike-landscape.md` is not in `_team/reports/` yet (only `spike-probe.md` = “PROBE OK”).
- Did not edit product files, `docs/01`, PLAN, or ADRs.

END
