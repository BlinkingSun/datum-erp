# Module Catalog

*The full intended surface of the system, what each piece does, what it needs, and
roughly when it gets built. Sizes are rough: S is days, M is weeks, L is a month or
two, XL is a quarter or more, for one experienced developer.*

---

## Build phases

Each phase ends at something a real shop could use, or at something that de-risks the
architecture. Nothing is built purely because it is next in an outline.

| Phase | Theme | Ends with |
|---|---|---|
| **0** | Kernel | Nothing user-visible. Everything depends on it. |
| **1** | We know what we have | Stock tracking with full lot and serial traceability |
| **2** | We know what we buy and sell | Purchase-to-receive and quote-to-ship |
| **3** | We know what we build and what it cost | **First release worth running a shop on** |
| **4** | We can prove it | **First release worth switching to from a commercial suite** |
| **5** | We can plan it | Material and capacity planning |
| **6** | Full regulated coverage | Complete quality system |
| **7** | Connected | Accounting, carriers, machines, CAD, EDI |

Phase 1 exists in that position for a specific reason. The append-only ledger is the
riskiest bet in the architecture, and inventory is the domain that exercises it hardest.
Building it first means finding out early if the idea is wrong.

---

## Phase 0 — Kernel

Not modules. Not optional. Present from the first commit because none of it can be
retrofitted. See `02-architecture.md` section 2.

| Component | Size | Notes |
|---|---|---|
| Identity, authentication, RBAC | M | Argon2id, sessions, optional OIDC, permissions declared by modules |
| Audit trail | M | Written by a database trigger attached at table creation, so module code never writes one and cannot forget or falsify it |
| Electronic signature | M | Bound to a record version by hash, captures meaning, re-authenticates |
| Ledger engine | L | Postings, groups, the balance-to-zero constraint, projections, rebuild |
| State machine engine | M | Declarative states and transitions, uniformly audited and signable |
| Documents and revisions | M | Revision control, approval, effectivity. Used by everything. |
| Numbering | S | Gap-free sequences per document type |
| Units of measure | M | Item-specific conversion, explicit precision and rounding |
| Custom fields | M | Typed, validated, audited. Not a JSON blob. |
| Event bus | S | Typed, at-least-once, idempotent subscribers |
| Background jobs | S | Durable queue, runs as a named service principal |
| Reporting and print | M | Server-side PDF for travelers, certificates, labels |
| Module registry | M | Install, enable, disable, upgrade, configuration manifest |

---

## Phase 1 — We know what we have

| Module | Size | Does | Depends on |
|---|---|---|---|
| `items` | M | Part master. Part number, revision, description, type (make, buy, service, phantom), units of measure, classification, lifecycle status. | kernel |
| `locations` | S | Warehouses, areas, bins, and the virtual locations the ledger needs: supplier, customer, scrap, adjustment, WIP. | kernel |
| `inventory` | L | The inventory ledger. Receipts, issues, moves, adjustments, cycle counts. On-hand, allocated, available. Status: available, quarantined, rejected, on hold. | kernel, items, locations |
| `lots` | M | Lot and serial identity, expiry and shelf life, supplier lot cross-reference, material certifications. **Not optional and not deferrable.** Retrofitting lot awareness into an existing ledger means rewriting every posting. | kernel, inventory |
| `valuation` | M | Cost layers on the inventory ledger. Standard, moving average, FIFO. One method per item class, never silently mixed. | kernel, inventory |

**Phase 1 delivers:** a shop can track what it has, where, under what lot, in what
state, with a complete and immutable history of every movement.

---

## Phase 2 — We know what we buy and sell

| Module | Size | Does | Depends on |
|---|---|---|---|
| `parties` | M | Customers, suppliers, contacts, addresses, terms, tax profiles. One party can be both. | kernel |
| `purchasing` | L | Requisitions, requests for quote, purchase orders, blanket orders and releases, acknowledgements, expediting. Supplier approval status enforced at order time. | kernel, items, parties |
| `receiving` | M | Receipt against a purchase order, over- and under-receipt tolerance, supplier lot capture, certification capture, routing to quarantine. | kernel, inventory, purchasing, lots |
| `sales` | L | Customer quotes and estimates, sales orders, order acknowledgement, pricing, discounts, contract pricing. **Estimating is the highest-value part of this module for a job shop and deserves disproportionate attention.** | kernel, items, parties |
| `shipping` | M | Picking, packing, packing lists, certificates of conformance, shipment posting, backorders. | kernel, inventory, sales |
| `ap-ar-lite` | M | Invoices in and out, three-way match, aging. Not a general ledger. Feeds `gl-export`. | kernel, purchasing, sales |

---

## Phase 3 — We know what we build and what it cost

**This phase produces the first release a real shop would run.**

| Module | Size | Does | Depends on |
|---|---|---|---|
| `bom` | L | Multi-level bills of material. Revisions, effectivity by date and by serial, phantom assemblies, scrap and yield factors, alternates and substitutes, where-used, mass change. | kernel, items |
| `work-centers` | M | Machines and cells, capacity calendars, shifts, labor and burden rates, setup and run rate defaults. | kernel |
| `routing` | L | Operation sequences, work center assignment, setup and run times, queue and move time, tooling and gage requirements, outside-processing operations. | kernel, items, work-centers |
| `production` | XL | Work orders, release, material allocation and issue, backflush, operation start and complete, scrap and rework, split and merge, close. The core of the system. | kernel, inventory, bom, routing |
| `shop-floor` | L | The terminal operators actually touch. Barcode scan, clock on and off an operation, report quantity and scrap, view drawings and instructions. **Touch-first, glove-friendly, under fifteen seconds per interaction, and it must work when the network is flaky.** More product value per hour of effort than anything else in the system. | kernel, production |
| `costing` | L | Job costing. Actual material, labor, burden, and outside processing against standard. Variance by element. Work-in-process valuation. Feeds quoting so the shop learns from every job. | kernel, production, valuation |
| `labor` | M | Employees, shifts, skills, clock-in, indirect time, attendance. Feeds costing and, later, training verification. | kernel |

---

## Phase 4 — We can prove it

**The medical wedge. This is what a commercial suite charges five figures a year for.**

| Module | Size | Does | Depends on | Regulated |
|---|---|---|---|---|
| `doc-control` | L | Controlled procedures, work instructions, forms, and drawings. Revision, review and approval workflow, effectivity, periodic review, read-and-understood acknowledgement. Built on the kernel document primitive. | kernel | yes |
| `inspection` | L | Inspection plans and characteristics, sampling schemes including ANSI/ASQ Z1.4, incoming, in-process, and final inspection, variable and attribute results, first article inspection with AS9102 forms. | kernel, items, inventory | yes |
| `ncr` | M | Nonconformance reports. Detection, containment, disposition (use as is, rework, repair, scrap, return to vendor), material review board approval with signature. | kernel, inventory, inspection | yes |
| `capa` | L | Corrective and preventive action. Investigation, root cause, action plan, implementation, effectiveness verification, closure. Trending across nonconformances and complaints to trigger escalation. | kernel, ncr | yes |
| `genealogy` | L | Forward and backward traceability. Given a lot or serial, the full tree of what went into it and everywhere it went. Recall simulation with an impact list. **Reads the existing inventory ledger rather than maintaining a parallel structure.** | kernel, inventory, lots, production | yes |
| `calibration` | M | Gages and measuring equipment, calibration schedules, certificates, out-of-tolerance impact assessment that identifies affected product. Blocks operations requiring an overdue gage. | kernel, items | yes |
| `training` | M | Training records against controlled procedures, qualification per operation, expiry and requalification. Vetoes an operator clocking onto an operation they are not current on. This hook is the single clearest demonstration of why ERP and quality belong in one system. | kernel, labor, doc-control | yes |
| `change-control` | L | Engineering change requests, orders, and notices. Impact assessment, approval routing with signatures, effectivity, and controlled release of bill of material and routing changes. | kernel, bom, routing, doc-control | yes |
| `dhr` | L | Device History Record. Assembles, from records already captured, the proof of what was actually built: material lots, operators, equipment, inspection results, deviations, and labeling. Renders as a signed, archivable document. | kernel, production, genealogy, inspection | yes |
| `dmr` | M | Device Master Record. The controlled index tying a device to its specifications, bill of material, routing, inspection plans, labeling, and packaging. | kernel, bom, routing, doc-control | yes |

---

## Phase 5 — We can plan it

| Module | Size | Does | Depends on |
|---|---|---|---|
| `mrp` | XL | Requirements netting. Explodes demand through bills of material against on-hand, on-order, and safety stock, and produces planned orders with dates. Pegging, so every planned order traces to the demand that caused it. Exception messages: expedite, defer, cancel. | kernel, items, bom, inventory, sales, purchasing, production |
| `mps` | M | Master production schedule. Firm plan MRP explodes against. Available-to-promise. | kernel, mrp |
| `forecast` | M | Demand forecasting and consumption against forecast. | kernel, sales |
| `capacity` | L | Rough-cut and detailed capacity requirements against work center calendars. Load versus available by period. | kernel, routing, work-centers, mrp |
| `scheduling` | XL | Finite capacity scheduling and sequencing. Dispatch lists per work center, drag-and-drop adjustment, what-if. Genuinely hard, and every commercial answer is an approximation. | kernel, production, capacity |

---

## Phase 6 — Full regulated coverage

| Module | Size | Does | Depends on | Regulated |
|---|---|---|---|---|
| `supplier-quality` | M | Approved supplier list, qualification and requalification, scorecards on quality and delivery, source inspection, supplier corrective action requests. | kernel, parties, purchasing, ncr | yes |
| `complaints` | L | Complaint intake, investigation, the reportability decision tree, and adverse event reporting (MDR in the United States, vigilance in the European Union). | kernel, capa, genealogy | yes |
| `udi` | M | Unique Device Identification. Device and production identifier construction, GS1 and HIBCC formats, label generation, GUDID submission files, and EUDAMED equivalents. | kernel, items, lots | yes |
| `internal-audit` | M | Audit schedule, checklists, findings, and linkage into corrective action. | kernel, capa | yes |
| `risk` | M | Risk management file support under ISO 14971. Hazard analysis and FMEA linked to product and process. | kernel, items | yes |
| `validation-pack` | L | Generates the customer's installation and operational qualification package: configuration manifest, requirement traceability matrix, and executable test protocols with results. **Turns our own test suite into the customer's validation evidence.** | kernel, module registry | yes |

---

## Phase 7 — Connected

| Module | Size | Does | Depends on |
|---|---|---|---|
| `gl-export` | M | Journal export to QuickBooks, Xero, and Sage, with a mapping configuration and a reconciliation report. **Built instead of a general ledger, and deliberately early in this phase.** | kernel, costing, ap-ar-lite |
| `barcode` | M | Label design and printing, scanner configuration, label formats for lots, serials, locations, and UDI. | kernel, inventory |
| `carriers` | M | Rate shopping, label generation, and tracking for the major parcel carriers. | kernel, shipping |
| `edi` | L | Electronic data interchange for purchase orders, acknowledgements, advance ship notices, and invoices. | kernel, sales, purchasing |
| `maintenance` | L | Preventive maintenance schedules, work requests, downtime capture, spare parts. Feeds capacity. | kernel, work-centers, inventory |
| `machine-data` | L | MTConnect and OPC UA collection for run time, cycle counts, alarms, and overall equipment effectiveness. Reduces manual data entry on the floor, which is the single best way to improve data quality. | kernel, work-centers, production |
| `cad` | L | **The differentiator.** Ingest STEP and mesh files, extract volume, bounding box, surface area, and features, and use them to seed an estimate. Attach models to items as controlled documents. Derive flat profiles for sheet and laser work. Nothing on the market does this well, and the tooling to do it already exists in adjacent projects. | kernel, items, sales, doc-control |
| `portal` | L | Customer-facing order status, drawings, certificates, and quote requests. | kernel, sales |
| `analytics` | M | Dashboards for on-time delivery, quality escape rate, quote-to-win, margin by customer and part, and equipment effectiveness. | kernel, most modules |

---

## Deliberately out of scope

| Not building | Why | Instead |
|---|---|---|
| General ledger, full accounting | Regulated, high liability, zero differentiation, and every shop already has one | `gl-export` |
| Payroll | Jurisdictional nightmare, solved commodity | Export hours |
| Design controls, Design History File | That is PLM, and a different product | Integrate |
| Real-time machine control | That is MES, with latency requirements we would not meet | `machine-data` consumes, never controls |
| Full warehouse management | Only matters above our target size | `inventory` with bins |
| CRM pipeline | Adjacent, well served, not our problem | Integrate |

---

## How to read this list

It is long, and that is the point of showing it. **No one builds all of this, and
nobody should try.** Phases 0 through 3 are a real product. Phase 4 is the reason
anyone would choose it over the alternatives. Everything past that is optional
elaboration that can happen in any order, driven by whoever shows up wanting it.

The realistic first milestone is a shop running phases 0 through 3 in production and
being honest about what hurts.
