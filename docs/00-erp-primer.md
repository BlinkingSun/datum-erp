# What an ERP Actually Does

*Written for someone who builds things but has never worked inside an ERP. Read this
first. Everything else in `docs/` assumes the vocabulary introduced here.*

---

## 1. The one-sentence version

An ERP is the system of record for the physical and financial state of a business:
what you have, what you owe, what you promised, what you need to get, what it cost,
and what you can prove about all of it.

That is genuinely all it is. Every module, every screen, every report is an
elaboration of those six questions. When an ERP feels overwhelming, it is because
those six questions are being asked about ten thousand part numbers at once.

## 2. The problem it solves

A small shop runs on a whiteboard, a spreadsheet, and one person who knows
everything. That works until it stops working, and it stops working at predictable
moments.

- **Two people sell the same inventory.** Sales quotes a delivery date against stock
  that production already consumed this morning.
- **Nobody knows what a job cost.** You know the quote was $8,400. You do not know
  whether you made money, because setup ran long on the second operation and nobody
  wrote it down.
- **A customer asks which lot their part came from.** You have travelers in a filing
  cabinet and three hours of work ahead of you.
- **You buy material you already have.** It was in the back of the rack, unlabeled.
- **The auditor arrives.** They ask to see the training record for the operator who
  ran the deburr operation on a specific serial number, on a specific date. You cannot
  produce it in the time available.

An ERP exists because these failures share one cause. The same fact is recorded in
several places, or in no place. The fix is a single shared record that every function
reads from and writes to.

That is also the reason ERP projects fail. The moment the shared record disagrees
with reality on the floor, people stop trusting it, and they go back to the
spreadsheet. **An ERP is only as good as its cheapest data entry moment.** Design
accordingly.

## 3. Follow one part through the whole system

Concrete beats abstract. A customer wants 500 titanium bone screws. Here is every
place that request touches, in order. The bracketed names are the modules that own
each step.

**A customer sends a print and a quantity.**
You record who they are, their terms, their addresses, their quality requirements.
`[parties]`

**You decide what it will cost you to make it.**
You need the raw material cost, the operations it takes, how long each one runs, and
your shop rate. You add margin. You send a number and a lead time. This is
*estimating*, and for a make-to-order shop it is the single most valuable and most
neglected function in the entire system. A shop that estimates badly loses money on
every job it wins. `[sales]`

**They accept. Now it is a promise.**
A sales order is a commitment to deliver a quantity, at a price, on a date. Everything
downstream exists to keep that promise. `[sales]`

**You define what the part is.**
An item master record holds the part number, revision, description, unit of measure,
and whether you buy it or make it. The screw is a *make* item. `[items]`

**You define what it is made of.**
A bill of material: one 12mm titanium bar stock blank per screw, plus a passivation
service. Bills of material are hierarchical. An assembly contains subassemblies which
contain parts. `[bom]`

**You define how it is made.**
A routing is the ordered list of operations. Saw, turn, mill flats, deburr, passivate
at an outside vendor, final inspect. Each operation names a work center, a setup time,
and a run time per piece. **The bill of material is what. The routing is how.**
`[routing]`

**The system works out what you are missing.**
You need 500 blanks. You have 120 in stock and 200 already on order from a mill
arriving in two weeks. So you need to buy 180 more, and you need them before the first
operation is scheduled to start. This netting calculation is **MRP**, Material
Requirements Planning, and it is the oldest and most central algorithm in the field.
`[mrp]`

**You buy the shortfall.**
A purchase order goes to an approved supplier. In a medical shop, approved is a
controlled status that a supplier can lose. `[purchasing]`

**Material arrives.**
You receive it against the purchase order, record the supplier lot number and the
material certification, and quarantine it until incoming inspection passes. Now your
own lot number exists and the material is available. `[inventory]` `[inspection]`

**You release the work.**
A work order authorizes production of 500 screws against that routing and that bill of
material. It reserves the material. It is the thing a person on the floor actually
works against. `[production]`

**The floor does the work.**
The operator scans in at the turning operation, runs parts, scans out. Material is
issued from stock to the work order. Time is logged against the operation. Twelve
parts scrap at the mill operation, so a nonconformance is raised and dispositioned.
`[production]` `[ncr]`

**You find out what it really cost.**
Actual material plus actual labor plus machine burden plus the outside passivation
invoice, compared against what you estimated. The difference is *variance*, and
variance is how a shop learns to quote. `[costing]`

**You ship and invoice.**
A packing list, a certificate of conformance, an invoice. `[sales]`

**Eighteen months later, something goes wrong.**
A hospital reports a fractured screw. You are asked which lot it came from, what bar
stock that lot was made of, which mill heat that bar came from, who else received
screws from that same bar, which operator ran each operation, whether their training
was current, and whether the gage used at final inspection was in calibration that
day.

**You must be able to answer within hours, not weeks.** This is *genealogy*, and in a
medical device shop it is not a feature. It is the reason the software exists.
`[genealogy]` `[dhr]` `[calibration]` `[training]`

If you follow that walk, you understand ERP. The rest is detail.

## 4. Nouns and verbs

Every ERP divides cleanly into two kinds of data, and confusing them is the most
common architectural mistake in the field.

**Master data is the nouns.** Slow-changing definitions. Items, bills of material,
routings, work centers, customers, suppliers, locations, employees, accounts. Master
data answers *what things are*. It is edited rarely and reviewed carefully. In a
regulated shop, changing master data requires an approved change order.

**Transactions are the verbs.** Timestamped facts about something that happened. Fifty
pieces moved from receiving to stock. Two hours logged against an operation. A
purchase order was issued. Transactions answer *what occurred*. They are created
constantly and, critically, **they are never edited or deleted.** A mistake is
corrected by posting an offsetting transaction, not by changing history.

The consequence is the single most important design decision in this project.

> **Balances are never stored. They are derived.**
>
> On-hand quantity is not a number in a column that gets updated. It is the sum of
> every inventory transaction for that item and location. Job cost is not a field. It
> is the sum of postings against the work order.

This is double-entry bookkeeping applied to physical goods, and it is five hundred
years old because it works. It gives you a complete audit trail as a byproduct rather
than as a feature you bolt on. It makes the question *how did we arrive at this
number* answerable by construction. For a system that must satisfy FDA audit trail
requirements, any other approach is a slow-motion disaster.

Stored balances are an optimization. They are a cache, they are rebuildable from the
ledger, and they are never the truth.

## 5. The loops

A manufacturing business runs five loops that share one set of master data. Most ERP
confusion comes from not knowing which loop you are looking at.

| Loop | Flow | Answers |
|---|---|---|
| **Demand** | Quote, order, ship, invoice, collect | What did we promise, and did we get paid? |
| **Supply** | Need, requisition, order, receive, inspect, pay | What do we need, and where is it? |
| **Production** | Plan, release, make, complete | What are we building right now? |
| **Financial** | Everything, cost, post to books | Did we make money? |
| **Compliance** | Everything, record, prove | Can we demonstrate it to an auditor? |

That last loop is where medical device manufacturing lives, and it is the one every
general-purpose ERP treats as an afterthought. It is also the reason this project
exists. See `06-regulatory.md`.

## 6. The modules, in plain language

What each does and why you would care. The full catalog with dependencies and build
phases is in `04-module-catalog.md`.

### Master data

- **Items.** The part number catalog. Units of measure and the conversions between
  them. Whether each item is purchased, manufactured, or a service.
- **Bills of material.** What goes into what, at every level, with revision control
  and effectivity dates so you know which version was used on which date.
- **Routings.** The ordered operations, the work center each runs at, setup and run
  times, and required tooling.
- **Work centers.** Machines and cells, their hourly rates, their available capacity.

### Inventory

Where everything physically is, in what quantity, under what lot or serial number, at
what cost, and in what state: available, quarantined, rejected, or allocated. Handles
receipts, issues, moves, adjustments, and cycle counts. In a regulated shop, lot and
serial tracking is not optional and cannot be added later.

### Purchasing

Suppliers, approval status, requests for quote, purchase orders, receiving, and the
three-way match that confirms the order, the receipt, and the invoice all agree before
anyone pays.

### Sales

Customers, quotes and estimates, sales orders, shipments, and invoices. For a job
shop, the estimating half of this module is where the business lives or dies.

### Production

Work orders, material issue, labor collection from the floor, operation completion,
scrap and rework. This is the module the shop floor actually touches, which means its
user experience matters more than any other module. A screen an operator will not use
produces no data, and a production module with no data is worse than nothing, because
everyone downstream believes it.

### Planning

- **MRP** nets demand against supply and tells you what to buy and make, and when.
- **Scheduling** decides what runs on which machine in what order.
- **Capacity planning** tells you whether the schedule is physically possible.

### Costing

Standard cost, meaning what it should cost, against actual cost, meaning what it did,
and the variance between them. Feeds both pricing and the financial statements.

### Quality

Inspection plans and sampling, nonconformance reports, corrective and preventive
action, calibration of gages, supplier quality, complaint handling, training records,
and change control. In most ERPs this is a thin bolt-on. Here it is a first-class
citizen.

### Financial

General ledger, accounts payable, accounts receivable, period close. **This project
deliberately does not build these first.** See `adr/0007-defer-general-ledger.md` for
the reasoning. Full accounting is a regulated, high-liability, low-differentiation tar
pit, and every shop already runs QuickBooks or Xero. Export to them instead.

## 7. What an ERP is not

Knowing the boundaries keeps scope from exploding. Each of these is adjacent, often
integrated, and frequently confused with ERP.

| System | Owns | Overlap with ERP |
|---|---|---|
| **PLM** | Product design, CAD files, engineering change | Shares the bill of material. PLM owns the engineering version, ERP owns the manufacturing version. |
| **MES** | Real-time machine and operation control on the floor | Overlaps production. MES works second by second, ERP works transaction by transaction. |
| **eQMS** | The quality system: procedures, audits, corrective action, training | Heavily overlaps quality. In medical, usually a separate purchase. |
| **WMS** | Warehouse mechanics: putaway, picking, slotting | Overlaps inventory. Only matters above a certain size. |
| **CRM** | Sales pipeline before an order exists | Feeds sales. |
| **CAD and CAM** | Geometry and toolpaths | Feeds items and routings. |
| **Accounting** | The books | Consumes costing output. |

For a thirty-person medical device shop, the honest observation is that ERP and eQMS
overlap so heavily that buying both means paying twice and reconciling two systems by
hand. Closing that seam is one of this project's central bets. See
`01-vision-and-scope.md`.

## 8. Vocabulary

Terms you will meet constantly. Skim now, return later.

| Term | Meaning |
|---|---|
| **BOM** | Bill of Material. What a thing is made of. |
| **Routing** | The ordered operations to make it. |
| **Work center** | A machine or cell where an operation runs. |
| **Work order** | Authorization to build a quantity of something. Also job, or shop order. |
| **WIP** | Work in process. Material and labor consumed but not yet finished. |
| **MRP** | Material Requirements Planning. The netting engine. |
| **MPS** | Master Production Schedule. The plan MRP explodes against. |
| **Lead time** | How long between ordering and having. |
| **Lot or batch** | A quantity produced or received together, tracked as a unit. |
| **Serial** | A unique identifier for a single unit. |
| **Genealogy** | The lot and serial tree. What went into what, and where it went. |
| **Traveler** | The paper that follows a job through the shop. Also router, or work packet. |
| **Backflush** | Deducting material automatically on completion rather than by scan. |
| **Burden** | Non-labor cost allocated to a job, usually per machine hour. Also overhead. |
| **Variance** | Actual minus standard. |
| **NCR** | Nonconformance Report. Something is wrong with the product. |
| **CAPA** | Corrective and Preventive Action. The investigation that stops recurrence. |
| **ECO or ECN** | Engineering Change Order or Notice. A controlled change to a design. |
| **CoC** | Certificate of Conformance. A statement that goods meet specification. |
| **DHR** | Device History Record. Proof of what was actually built. |
| **DMR** | Device Master Record. The specification of how to build it. |
| **DHF** | Design History File. The record of how it was designed. |
| **UDI** | Unique Device Identification. The FDA barcode scheme for devices. |
| **AVL or ASL** | Approved Vendor List, or Approved Supplier List. |
| **MOQ** | Minimum Order Quantity. |
| **Effectivity** | The date or serial number from which a revision applies. |

## 9. Why manufacturing ERP is hard

Worth stating plainly before designing anything.

1. **Everything is connected.** Change a bill of material and you affect costing,
   planning, purchasing, and quality. There are no isolated features.
2. **Units of measure are a minefield.** You buy bar stock by the foot, issue it by
   the inch, and scrap it by the pound. Conversions are item-specific and lossy.
   Getting this wrong corrupts inventory quietly for months.
3. **Time is not simple.** A routing has setup, run, queue, move, and wait time. Shop
   calendars have shifts, holidays, and planned downtime. Scheduling is NP-hard in the
   general case, and every real system approximates.
4. **Costing has no single right answer.** Standard, average, FIFO, LIFO, and actual
   all give different numbers for the same physical inventory, and all are defensible.
   The system must be explicit about which it uses and must never silently mix them.
5. **The floor will route around bad software.** If scanning in takes four taps too
   many, operators batch it up at the end of the shift from memory, and your data
   becomes fiction. This is the leading cause of failed implementations, and it is a
   design problem rather than a training problem.
6. **The data migration is the project.** Getting ten thousand existing part numbers,
   bills of material, and open orders in from spreadsheets and a twenty-year-old
   system is usually harder than any feature.
7. **In regulated shops, the software itself is subject to inspection.** It must be
   validated, and the audit trail must survive scrutiny by someone whose job is to
   distrust it.

---

*Next: `01-vision-and-scope.md`, which covers what this project specifically is, who
it is for, and what it deliberately refuses to do.*
