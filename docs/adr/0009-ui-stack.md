# 0009. TypeScript and React for the UI, Tauri for the desktop shell

Audience: contributor. Status: absent.

Status:   **Accepted** (2026-09-15)
Date:     2026-09-11, accepted 2026-09-15
Decider:  project owner

## Context

The product has three users who cannot share a layout. An office user lives in dense
records and data grids for hours. A shop-floor operator is standing, often gloved,
and will route around anything slower than a scan and a confirm. A quality or
planning user reads traces and exception lists. `DESIGN.md` requires that we share
tokens and never layouts.

`docs/02-architecture.md` section 5 already asserts a stack: TypeScript, React,
TanStack Query / Table / Router, one application, Tauri v2 as the desktop shell,
and server-rendered PDF for documents that must archive and must render identically
everywhere. That stack has never been recorded as a decision with its costs.

Two constraints landed after the assertion.

The install decision (ADR 0003 as amended, `research/decisions/install-story.md`)
withdrew bundled PostgreSQL. The Tauri shell's load-bearing job was carrying that
cluster and supervising it. It no longer has a database to supervise. A browser on
the LAN is a complete client for v1. Scanners present as keyboards. The
architecture's own topology already says the floor tablet is a browser.

21 CFR 11.10(b) and the regulatory spike section 1.1 item 10 require a deterministic
record-rendering service. The UI is not a renderer. Travelers, certificates, labels,
and packing lists must archive, and a screen change must not change a historical
document.

## Acceptance note (2026-09-15)

Accepted by the owner. The document-level status stays **absent** because no interface
crate exists yet; acceptance settles the decision, not the delivery.

What acceptance binds:

- The wrapper is a **client of the HTTP API and nothing else**. No wrapper links a kernel
  crate. This is the same rule ADR 0010 applies to the router: one surface, no second path.
- **One generated TypeScript client** is the wrapper contract. It is generated from the
  OpenAPI document, which is generated from the capability table, which `just ci` now pins
  to a committed fixture (T-44). A wrapper on any platform consumes that client.
- **One application, three interaction modes** — office, shop floor, quality/planning are
  route trees over one component library and one token set, not three codebases.
- Tauri v2 carries the same application to macOS, Linux, Windows, Android and iOS. A browser
  on the LAN remains a complete client for v1, so no platform shell is load-bearing.

What acceptance does **not** unblock: interface work waits on Goal 2. A wrapper can only be
honest about "every function has an API" once the capability table actually covers the
engine. Today it does not — `wicket-identity` exposes login and logout and nothing else,
and numbering, units of measure, the module registry and the ledger are unreachable
(`TODO.md` T-31..T-42, gate T-43).

## Decision

**TypeScript and React**, with **TanStack Query, TanStack Table, and TanStack
Router**, as the UI stack.

**One application, three interaction modes.** Office, shop floor, and
quality/planning are three route trees over one component library, one generated
TypeScript client, and one token set. They are not three codebases.

**Documents that must archive are server-rendered to PDF.** The application never
prints the DOM. Templates are versioned independently of the UI. A traveler
produced on Tuesday and a traveler reproduced on Friday from the same record
version are the same bytes.

**Tauri v2 is the desktop shell technology, and it is not required to use the
product.** After the install amendment it does not start, stop, or upgrade a
database. Production clients are a browser for the office and a browser in kiosk
mode on a tablet for the floor. The shell's remaining jobs are wrapping the same
application when a shop wants an icon and a window that is not browser chrome,
locking a floor terminal down more tightly than a kiosk tab, and, later, bridging
to hardware a browser cannot reach: a serial-port gage, a direct label-printer
driver, or an offline floor cache. Naming those jobs keeps Tauri from being rebuilt
as a cluster supervisor, and keeps it from being re-litigated as Electron.

The UI consumes the public OpenAPI surface and the generated TypeScript client.
That is how the API stays honest.

## Consequences

**What this buys.**

- Office grids get virtualization, persisted column layout, grouping, and inline
  edit from a library that exists to do that, rather than from a pile of table
  markup we would end up writing JavaScript for anyway.
- The three modes share types, authentication, and the API client. A work-order
  identity is one type on the floor and in the office.
- The public API has a paying in-house customer from the first screen, which is
  the contributor story ADR 0002 claimed.
- Archival documents do not silently change when a stylesheet does.
- Tauri, when a shell is actually needed, is small, uses the system webview, and
  produces native installers without shipping Chromium onto a shop machine that is
  already running a database and a server.

**What this costs.**

- A second language and a second toolchain. Contributors who can write a kernel
  crate now also need Node, TypeScript, and the React ecosystem to touch a screen.
  ADR 0002 already paid this once in the other direction. This is the other half
  of that bill, and it is the larger half for anyone who came to write business
  logic and found a bundler.
- Client-side state for screens that are mostly request and response. Cache
  invalidation, stale closures, and a second source of truth that is not the
  ledger are now ours. An ERP does not need most of what a single-page application
  is good at.
- React and TanStack version churn is a recurring tax. A validated customer pins
  an application version; the UI dependencies still age underneath the next
  release we ask them to requalify.
- Two renderers. The screen and the PDF can disagree, and when they do the PDF is
  the record and the screen is a preview. Keeping them close enough that a user
  trusts the preview is ongoing work. `DESIGN.md` already requires that tokens
  feed both.
- Tauri across three operating systems is still three webviews: WebView2,
  WKWebView, and webkitgtk. Behavioral differences are real. Paying that packaging
  cost before a shop needs a shell is how this decision would waste a wave. That
  is why the shell is not a v1 requirement to run the product.
- Shop-floor performance now depends on a JavaScript bundle loading on a cheap
  tablet. The 500ms scan-to-confirm budget in `docs/02-architecture.md` is tighter
  against that than against an HTML form posted to the server.

## Alternatives considered

**Server-rendered HTML with htmx.** The strongest alternative, and the one that
deserves to win if the grid argument is bluff. Most of an ERP is forms and tables.
The shop floor is a short sequence of posts: badge in, scan traveler, log time,
report scrap. The client-state argument is weaker than it first appears, because
the server is already the source of truth, the ledger is already transactional,
and an optimistic update on a regulated record is a defect. Odoo, ERPNext, and
Tryton all ship some version of this and it works.

It lost on the office grid, and it lost on one-application-three-modes, and those
are different losses.

The office item master, the inventory ledger view, and the work-order list are not
forms. They are large, filterable, column-customizable, inline-editable grids with
server-side sort and pagination, and they have to feel native to someone who lives
in them all day. htmx can render a table. Virtualization, persisted column layout,
grouping, and inline edit are JavaScript widgets bolted onto that table. At that
point we have a second, worse UI stack inside the first.

The shop floor does not need any of that. If the floor were the whole product,
htmx would be the right call, and this record would say so. The product is one
application serving three modes. Splitting into htmx for the floor and React for
the office is two UI stacks, two client generators, and two ways to render a lot
number. `DESIGN.md` forbids sharing layouts. It does not permit two type systems.

The grid requirement therefore decides the office surface. It does not decide the
floor. The one-application constraint decides the rest.

**Svelte or Vue with the same TanStack table.** Comparable. Smaller overlap with
the rest of the TypeScript world, and a weaker grid ecosystem than React plus
TanStack Table. Rejected on library depth for the thing this product is made of,
not on ideology.

**Electron.** A known desktop shell. Rejected because it ships Chromium on a shop
machine that is already running a database and a server, and because the install
story is the opposite of small. Tauri lost its original job and still beats
Electron on the jobs that remain.

**Native UI in Rust (egui, GPUI) or in Qt/GTK.** One language with the backend, if
the choice is Rust. Rejected because it does not give us a browser client, and the
topology is browsers on the LAN. A native floor terminal would be a second
application, which is the cost this decision exists to avoid.

**Print from the browser.** Rejected. A browser print is not an archival document.
It changes with CSS, with the webview, and with the printer driver. 11.10(b) wants
a copy that remains accurate after the program changes. Server-rendered PDF with
versioned templates is that copy. The UI may preview. It may not be the record.

**A separate mobile application for the floor.** Rejected. Scanners are keyboards.
Tablets run a browser. A second client is a second validation surface.

## Revisit if

- A real office grid ships and operators still prefer a printed workbench, in
  which case we over-bought the client stack and htmx for the non-grid surfaces
  should be reconsidered before the floor terminal hardens.
- A shop needs a serial-port gage or a direct label printer in v1. That is the
  condition that pulls Tauri into the production path, not a general desire for
  an icon.
- The generated TypeScript client plus React turns out to be where contribution
  stalls. Then the public API is not the contributor story ADR 0002 claimed, and
  this stack is part of why.
