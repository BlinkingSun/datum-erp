# HTTP API conventions

Audience: contributor. Status: partial.

**Conforms to:** [ADR 0005](adr/0005-compliance-in-kernel.md), [ADR 0008](adr/0008-single-tenant.md), [ADR 0009](adr/0009-ui-stack.md); `docs/03-module-system.md` §3.4 and §7; `research/decisions/core-quantity.md` (D1).

This is the public HTTP contract every module's routes follow. A third-party module that registers routes under its namespace appears in the generated OpenAPI document and gets a typed client for free (`docs/03-module-system.md` §3.4). The first-party UI is a client of the same surface (ADR 0009). `wicket-server` in Wave 2s implements exactly this, for the slice endpoints in §9.

The cost of a public API from day one is that these conventions freeze before most modules exist. Changing a field name later is a new major version and a year of dual-running. That is cheaper than letting each module invent a JSON dialect.

There is no tenant identifier in any path, header, or body. One installation is one customer (ADR 0008).

---

## 1. Versioning, deprecation, and namespaces

Every public route lives under `/api/v1/`. The version is in the path, not in a header and not in a query parameter (`docs/03-module-system.md` §7). A client that does not send a version is not a client of a default; it is a 404.

The anatomy sketch in `docs/03-module-system.md` §2 (`api.rs` under `/api/calibration`) is a module-layout illustration. The public path is `/api/v1/{module}/...`. Kernel-owned routes that are not a module (`/api/v1/openapi.json`, `/api/v1/audit` as an admin SELECT) still sit under `/api/v1/`.

A module registers routes only under its own namespace. `items` owns `/api/v1/items`. `inventory` owns `/api/v1/inventory`. A module does not register under another module's prefix. Collision at registration is a startup failure, not last-writer-wins.

Once version 1.0 exists (`docs/03-module-system.md` §7):

| Promise | Meaning |
|---|---|
| Path versioning | `/api/v1/...` and, later, `/api/v2/...` are different contracts. |
| Old versions live for at least one full major cycle | `/api/v1` stays up through the whole of `v2.x`. |
| Deprecation | A field, route, or error code is marked deprecated and emits a runtime warning for one full minor cycle before removal. |
| Additive within a major | New optional fields and new routes are allowed. Removing, renaming, or repurposing a field is a major. |
| Database schemas are not public | Reading `app.*` from outside the process is unsupported. The HTTP API and the event stream are the integration surface. |

`Sunset` and `Deprecation` response headers name the removal version. The OpenAPI document marks the same members `deprecated: true`. A client that ignores both still works until the major that removes the member.

---

## 2. Resources, identifiers, lists, and errors

### 2.1 Naming

Collections are plural nouns. Nested actions that are state transitions, not new resources, are verbs on the resource:

```
GET    /api/v1/items/{id}
POST   /api/v1/inventory/receipts
POST   /api/v1/production/work-orders/{id}/release
GET    /api/v1/genealogy/trace
```

JSON object keys are `snake_case`. Enumerations on the wire match the serde default of the Rust type they represent (`"Length"`, `"Count"`, `"Released"`).

### 2.2 Identifiers

Every durable record identifier is a UUID v7, serialized as a lowercase hyphenated string (`CONTRACT-workspace.md` §6.1 `Identifier`). Clients never mint them. The server mints on insert and returns the id. A client-supplied id on POST is a validation error on field `id`.

Human-visible numbers (`MDS-450-M4x12`, `WO-2026-1847`, `LOT-BAR-24-4412`) are attributes, not primary keys. Lot and serial identifiers obey PLAN §6b item 9 at generation: `A–Z`, `0–9`, `-`, at most twenty characters. The API refuses a candidate that fails that rule with `VALIDATION` on `identifier`; it does not coerce case or truncate.

`UnitId` is a JSON number (i64 catalog key). `CurrencyId` is a JSON number (ISO 4217 numeric code). Neither is a UUID.

### 2.3 Pagination

List endpoints are cursor-based. Offset pagination is not offered: an insert during a walk repeats or skips rows, and a shop-floor grid that jumps is an operator error.

```
GET /api/v1/items?limit=50&cursor=<opaque>
```

| Query | Rule |
|---|---|
| `limit` | Optional. Default 50, maximum 200. Values outside that range are `VALIDATION` on `limit`. |
| `cursor` | Opaque. The client sends back exactly the `next_cursor` it received. Interpreting it is unsupported. |
| `sort` | Optional. See §2.5. The cursor is bound to the sort in force; mixing a cursor from one sort with another sort is `VALIDATION` on `cursor`. |

Response envelope for every list:

```json
{
  "data": [],
  "next_cursor": "Aof0...",
  "has_more": true
}
```

`next_cursor` is `null` when `has_more` is false. An empty first page is `"data": []`, `"next_cursor": null`, `"has_more": false` — not 404.

The default sort is `id` ascending. UUID v7 is time-ordered, so that is chronological create order. Every sort, including an explicit one, includes `id` as the last tie-breaker so the cursor is stable.

### 2.4 Filtering

Equality filters are query parameters named after the field: `status=active`, `item_id=01932c5a-8b10-7001-8000-000000000001`. Multiple values for one field are comma-separated and mean OR. Distinct parameters are AND.

Unknown filter names are `VALIDATION` on that name. Filtering on a field the caller cannot read is `FORBIDDEN`, not an empty list: an empty list would teach a client to probe.

Quantity, money, and expiry are not filterable as raw strings. A module that needs "expires in September 2027" exposes `expiry_from` / `expiry_to` as structured query members, not a `DATE`.

### 2.5 Sorting

`sort=number` or `sort=-updated_at`. A leading `-` is descending. Only fields listed for that endpoint in OpenAPI are legal. Anything else is `VALIDATION` on `sort`.

### 2.6 Error envelope

Every error body is this object and only this object:

```json
{
  "error": {
    "code": "VALIDATION",
    "message": "Lot identifier must be uppercase A-Z, digits, and hyphen, at most 20 characters.",
    "field": "identifier",
    "request_id": "01932c5a-8b10-7001-8000-00000000000d"
  }
}
```

| Member | Rule |
|---|---|
| `code` | Machine-stable token. Clients branch on it. It does not change within a major version. |
| `message` | Human, English, specific. Not a copy of `code`. Safe to put on a floor screen. |
| `field` | JSON pointer-style path for validation (`identifier`, `quantity.amount`, `lines/0/lot_id`). `null` when the error is not about one field. |
| `request_id` | UUID v7 of this request. Always present. Echoed in `X-Request-Id`. |

`code` values used in this document: `VALIDATION`, `UNAUTHENTICATED`, `FORBIDDEN`, `NOT_FOUND`, `CONFLICT`, `IDEMPOTENCY_CONFLICT`, `SIGNATURE_REQUIRED`, `SIGNATURE_NO_PROVIDER`, `RATE_LIMITED`, `PAYLOAD_TOO_LARGE`, `TIMEOUT`.

HTTP status tracks the class: 400 `VALIDATION`, 401 `UNAUTHENTICATED`, 403 `FORBIDDEN`, 404 `NOT_FOUND`, 409 `CONFLICT` / `IDEMPOTENCY_CONFLICT` / `SIGNATURE_NO_PROVIDER`, 413 `PAYLOAD_TOO_LARGE`, 429 `RATE_LIMITED`, 504 `TIMEOUT`. `SIGNATURE_REQUIRED` is 401 when re-authentication is missing, 403 when the signer lacks the permission.

A 5xx that is not a timeout still uses this envelope. The message does not leak internals.

---

## 3. Wire shapes

Source: `research/decisions/core-quantity.md` §6. Domain types `Quantity<D>`, `Money`, and `UnitCost<D>` do not cross HTTP. The wire types are `AnyQuantity` and `MoneyWire`. `Decimal` is a JSON **string**, never a JSON number: `serde_json` numbers are `f64` in most clients, and a quantity that became a float is a silent inventory corruption.

### 3.1 `AnyQuantity`

```json
{ "amount": "258000.00000000", "unit": 2, "dimension": "Length" }
```

| Field | Type | Rule |
|---|---|---|
| `amount` | string | Base-10 decimal, scale at most 8 (`QUANTITY_MAX_SCALE`). A JSON number is `VALIDATION` on `quantity.amount`. |
| `unit` | number | `UnitId` (i64). Catalog row, not a code like `"mm"`. |
| `dimension` | string | `DimensionKind`: `Count`, `Length`, `Mass`, `Time`, `Volume`, `Area`. Denormalized so a client or middleware can reject a mismatch without a catalog round-trip (D1 §6). |

A body whose `dimension` does not match the catalog kind of `unit` is `VALIDATION` on `quantity.dimension`. Conversion from an entered unit to the item's stocking unit happens at this boundary (`docs/adr/0004-append-only-ledger.md`); the stocking quantity is what the ledger posts. When the entered unit differs, the request may also carry `entered` as a second `AnyQuantity` (provenance, never summed).

### 3.2 `MoneyWire`

```json
{ "amount": "2034.000000", "currency": 840 }
```

| Field | Type | Rule |
|---|---|---|
| `amount` | string | Scale at most 6 (`MONEY_MAX_SCALE`, D1 §2.5; storage `numeric(24,6)` per D-W1-1). A JSON number is `VALIDATION` on the amount field. |
| `currency` | number | `CurrencyId`, ISO 4217 numeric (840 is USD). |

A currency's minor unit is a settlement scale, reached through `Money::settle`, never a storage or wire scale (D-W1-1). A posting amount may carry sub-minor digits. GL-facing totals settle; this API does not round them on the way in.

### 3.3 Dates and timestamps

Instants are RFC 3339 in UTC with a `Z` suffix: `"2024-10-03T15:41:22Z"`. A timestamp with an offset other than `Z` is accepted and converted; a timestamp without a timezone is `VALIDATION`. Fractional seconds are optional.

Calendar dates that are not expiries are RFC 3339 full-date: `"2024-09-11"`. They are dates, not instants, and they are not converted through a timezone.

No client-supplied timestamp is stored as the time of record (PLAN §6 item 4; ADR 0005; D3 §4). `created_at`, `updated_at`, `posted_at`, `signed_at` are server-stamped. A client value for those fields is ignored if present on write, and is a `VALIDATION` if the field is documented as forbidden on input.

### 3.4 Expiry

Expiry is a structured value, never a bare date (PLAN §6b item 12; `research/background/regulatory.md` §1.0.5: GS1 AI (17) permits `YYMM00`, unspecified day meaning end of month). A `DATE` on the wire would invent a day the UDI module can never recover.

```json
{ "value": "2027-09", "precision": "month" }
```

| `precision` | `value` | Meaning |
|---|---|---|
| `year` | `YYYY` | End of that year. |
| `month` | `YYYY-MM` | End of that month. |
| `day` | `YYYY-MM-DD` | That calendar day. |

A `value` that carries more precision than `precision` claims (a day in a month-only expiry) is `VALIDATION` on `expiry.value`. A month-only expiry round-trips as month-only: the API does not fill in `01` or `28`. PLAN §3 Wave 2s acceptance 12 is this rule, tested on the lot in §9.2.

---

## 4. Mutation semantics

### 4.1 Actor

Every mutating request takes the acting identity from the authenticated session, never from the body (PLAN §6 item 5; D3 §2.1). A body field `actor_id`, `user_id`, `signed_by`, or `posted_by` is `VALIDATION` on that field. Background jobs authenticate as a named service principal; they do not get a blank actor.

A request with no authenticated actor never opens a write transaction (D3 §10 c). The handler returns 401 `UNAUTHENTICATED` and no row is written.

### 4.2 Idempotency

Every POST carries `Idempotency-Key`: a client-generated UUID v7. The key is stored in schema `transient` (D-W1-2: working state, no audit trigger, DELETE expected). Replay of the same key with the same body returns the original status and body. Replay with a different body is 409 `IDEMPOTENCY_CONFLICT`. A missing key on POST is `VALIDATION` on the header.

GET, HEAD, and DELETE (where DELETE exists at all: `transient` only) do not use the key. PUT and PATCH are identified by the resource and its version (§4.3); they may send the key, and the server stores it, but the version is the concurrency control.

### 4.3 Optimistic concurrency

Every mutable record carries an integer `version`, starting at 1, incremented on every successful write. The value is the `RecordRef.version` the signature gate hashes (CONTRACT §6.3).

Writes send `If-Match: "<version>"`. A missing header on PATCH, PUT, or a state-transition POST is `VALIDATION`. A stale version is 409 `CONFLICT` with `field` `"version"`. `If-Match: *` is not accepted on regulated records: it would skip the check this header exists to perform.

List and GET responses include `version`. ETags, if emitted, are that integer in quotes; clients may send either `If-Match` form.

### 4.4 Server time

Time of record is read on the server, inside the audit trigger: `at = now()` (transaction-constant), `stmt_at = clock_timestamp()` (D3 §4). Every row of one request's transaction carries the same `at`. The host clock is the source; this API does not claim trusted time.

Responses echo `created_at` / `posted_at` as RFC 3339 UTC. They will not match a client clock.

### 4.5 Record retirement

Records are retired by state change, not by HTTP DELETE (PLAN §6b item 16 as amended by D-W1-2). A `DELETE` on an `app` resource is 405. Sessions, idempotency keys, and other `transient` rows are the exception, and they are not a public resource.

---

## 5. Electronic signature over HTTP

Source: `research/decisions/audit-persistence.md` §9; ADR 0005 as amended; CONTRACT §6.3 (D-W1-4). Every signing requires all identification components. Wicket does not implement the 11.200(a)(1)(i) continuous-session relaxation: a session cookie on a shared floor tablet is not a component "designed to be used only by the individual" (D3 §9).

### 5.1 Declaration in OpenAPI

A transition that requires a signature is marked on the operation, not discovered at runtime:

```yaml
x-wicket-signature:
  meaning: Released
  permission: production.work_order.release
```

`meaning` is the printed meaning stored on the signature row (PLAN §6b item 15: the name and the meaning are snapshots, not live joins). `permission` is the `PermissionKey` the signer must have held at mint. Operations without the extension are `NotRequired`; the configuration manifest lists both kinds (`docs/03-module-system.md` §8, D-W1-4). A `regulated = true` module cannot register an edge that is neither.

### 5.2 What the client presents

Minting is a separate request, owned by `wicket-esign`, and happens before the transition (CONTRACT §6.3: the gate verifies, it does not mint).

```
POST /api/v1/esign/signatures
Idempotency-Key: 01932c5a-8b10-7001-8000-0000000000e1
```

```json
{
  "meaning": "Released",
  "record": {
    "table": "production.work_order",
    "id": "01932c5a-8b10-7001-8000-000000000006",
    "version": 3
  },
  "identification": {
    "code": "MREYES",
    "secret": "<signing password, not the login password>"
  }
}
```

`identification` is two components every time: the signing identifier and the signing secret, or an IdP step-up token in place of the secret for OIDC shops. The login session is not a component. The signing credential is separable from the login credential (PLAN §6b item 14); using the session password here is `VALIDATION` on `identification`.

The meaning text in the body must equal the `x-wicket-signature.meaning` of the transition that will consume the token. The server stores the printed meaning, the signer, the server time, and SHA-256 of the canonical record bytes at `record.version`.

The transition then carries the minted id:

```
POST /api/v1/production/work-orders/{id}/release
X-Wicket-Signature: 01932c5a-8b10-7001-8000-0000000000e2
If-Match: "3"
```

No signature material belongs in the transition body. The gate loads the row by id, checks meaning, record (including version), content hash, permission snapshot taken at mint, and single-use claim (CONTRACT §6.3). The first failure is the error.

Failed mint attempts are security events, not business audit rows (D3 §9).

### 5.3 What the response records

A successful signed transition includes the manifestation the audit row also carries:

```json
{
  "signature": {
    "id": "01932c5a-8b10-7001-8000-0000000000e2",
    "signer_id": "01932c5a-8b10-7001-8000-00000000000b",
    "printed_name": "M. Reyes",
    "meaning": "Released",
    "signed_at": "2026-03-14T15:02:11Z",
    "record": {
      "table": "production.work_order",
      "id": "01932c5a-8b10-7001-8000-000000000006",
      "version": 3
    },
    "record_content_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  }
}
```

`signed_at` is server time. `printed_name` is the snapshot at mint. `esign_id` on the audit rows of that transaction is `signature.id`.

### 5.4 The slice ships without signatures

`wicket-esign` is Wave 2b (PLAN §3). Until it is bound, the composition root wires `NoSignatures`, which refuses every token (CONTRACT §6.3). A `Required` edge under `NoSignatures` does not skip the check and does not run unsigned: the handler returns 409 `SIGNATURE_NO_PROVIDER`. A release build that enables any `Required` edge while `NoSignatures` is bound fails at startup (D-W1-4). The Wave 2s slice is unsigned **by declaration** (plain-shop enables no `regulated = true` module; regulated-device lists the edge and the gate still refuses until 2b). Unsigned-by-accident is not representable.

`POST /api/v1/esign/signatures` does not exist until Wave 2b. Calling it is 404.

---

## 6. OpenAPI and the TypeScript client

OpenAPI is generated from the Rust route registrations, not written by hand. Hand-written spec drifts; generated spec is what the binary actually serves.

The document is published at a fixed path:

```
GET /api/v1/openapi.json
```

Unauthenticated GET is allowed: a third party generating a client should not need a shop-floor login to read the contract. The document does not include secrets, connection strings, or installation identity.

The generated TypeScript client is the only HTTP client the UI uses (ADR 0009). The UI does not call `fetch` against these routes except through that client. That is how the API stays honest: a breaking change breaks the first-party screens in the same build.

External clients may generate from the same document in any language. They are not privileged; they see the same errors, the same wire types, and the same signature declaration.

---

## 7. Authentication and sessions

Mechanism lives in `wicket-identity`. This section is the HTTP convention that mechanism must satisfy.

| Client | Credential | CSRF |
|---|---|---|
| First-party browser (office and floor) | Session cookie `wicket_session`, `HttpOnly`, `SameSite=Lax`, `Path=/`, `Secure` when the install is on TLS | Required on every mutating request: `X-CSRF-Token` must match cookie `wicket_csrf` (double-submit). Mismatch is 403 `FORBIDDEN`. |
| Machine and third-party | `Authorization: Bearer <token>` | Not applicable. Bearer is not sent automatically by a browser on a cross-site form. |

Cookie vs bearer is a convention, not a product fork. One principal can hold both a session and a token. The actor that reaches `Tx::begin` is the one `wicket-identity` verified for this request.

A LAN install often runs HTTP, not TLS (`docs/02-architecture.md` §6, ADR 0003 as amended). `Secure` cookies then cannot be set; that is a real cost, and it is why CSRF is not optional on cookie-auth and why floor terminals auto-lock (docs/02 §8). Bearer tokens on HTTP are equally visible on the wire. TLS is recommended; it is not assumed.

Login, logout, session lock, inactivity timeout, Argon2id, and optional OIDC are `wicket-identity` routes under `/api/v1/identity/`. This document does not specify their bodies. It does specify:

- The session is not a signing component (§5.2).
- A locked or expired session is 401 `UNAUTHENTICATED` on the next mutating request, with no write.
- There is no anonymous write and no "unknown" actor.

---

## 8. Rate, size, and timeout

This is a LAN server. The threat is a runaway client or a stuck transaction, not a public DDoS.

| Limit | Value | Why |
|---|---|---|
| JSON body | 1 MiB | An item, a receipt, a work-order completion. Larger payloads are attachments, not JSON. |
| Request rate | 60 requests / second / session, burst 120 | Sheds a retry loop. Not a commercial quota. Excess is 429 `RATE_LIMITED`. |
| Handler timeout | 30 s | Then 504 `TIMEOUT`. The transaction aborts. |
| Idle-in-transaction | 15 s | PostgreSQL `idle_in_transaction_session_timeout` (D3 §2.2). |
| Interactive GET p95 | 200 ms | `docs/02-architecture.md` §7. |
| Floor scan-to-confirm p95 | **500 ms** | `docs/02-architecture.md` §7. Slow scanning is the top cause of abandoned data entry. |
| Genealogy trace | 10 s | Same table. Recursive CTE, not a floor scan. |

Floor endpoints — badge-in, lot scan, receipt confirm, work-order complete — are designed against the 500 ms budget as a hard constraint: one round trip, no "please wait for a job." If a completion cannot post and confirm inside that budget on a LAN, the handler is wrong, not the tablet. The budget includes the audit trigger and the ledger deferred checks; those are the cost of the kernel, paid here, not deferred to a spinner.

Attachments (certificates, drawings) use a separate upload route with a larger limit, owned by the documents module (Wave 2b). They are not JSON bodies on floor POSTs.

---

## 9. Worked example — the Wave 2s slice

Identifiers are the canonical set in `PLAN.md` §3, the same set `docs/05-data-model.md` §7 uses, so the two documents can be checked against each other mechanically.

| Human id | UUID v7 | What |
|---|---|---|
| `MDS-450-M4x12` Rev C | `01932c5a-8b10-7001-8000-000000000001` | Cortical bone screw, Ti-6Al-4V ELI, make, stocking unit EA |
| `RM-TI-BAR-12` | `01932c5a-8b10-7001-8000-000000000002` | Titanium bar, stocked in mm, bought in bars |
| `HT-ATI-24-8831` | `01932c5a-8b10-7001-8000-000000000003` | Mill heat, 240 kg, certified 2024-09-11 |
| `LOT-BAR-24-4412` | `01932c5a-8b10-7001-8000-000000000004` | Bar-stock lot, 86 bars, received on `PO-2024-0841` 2024-10-03 |
| `LOT-WO-1847` | `01932c5a-8b10-7001-8000-000000000005` | Finished lot |
| `WO-2026-1847` | `01932c5a-8b10-7001-8000-000000000006` | Work order, 500 pieces, op 20 TURN on `WC-LATHE-03` |
| `WC-LATHE-03` (WIP) | `01932c5a-8b10-7001-8000-000000000007` | Work-centre / WIP location |
| quarantine | `01932c5a-8b10-7001-8000-000000000008` | Receipt location |
| finished goods | `01932c5a-8b10-7001-8000-000000000009` | FG location |
| `SN-450-000134` | `01932c5a-8b10-7001-8000-00000000000a` | First finished serial (through `SN-450-000633`) |
| `M. Reyes` | `01932c5a-8b10-7001-8000-00000000000b` | Operator; never in a request body |
| supplier | `01932c5a-8b10-7001-8000-00000000000c` | Ledger virtual location |

Catalog rows used below (example ids, not a frozen seed): unit `1` = EA (`Count`), `2` = mm (`Length`), `3` = kg (`Mass`), `4` = BAR (`Count`). Currency `840` = USD.

Every request below is sent with a session. `X-Request-Id: 01932c5a-8b10-7001-8000-00000000000d` is shown once and implied thereafter. Mutating POSTs send `Idempotency-Key` and, where the resource already exists, `If-Match`.

### 9.1 Item — `MDS-450-M4x12`

```
GET /api/v1/items/01932c5a-8b10-7001-8000-000000000001
```

```json
{
  "id": "01932c5a-8b10-7001-8000-000000000001",
  "number": "MDS-450-M4x12",
  "revision": "C",
  "description": "Cortical bone screw, Ti-6Al-4V ELI, M4 x 12",
  "type": "make",
  "stocking_quantity": { "amount": "1.00000000", "unit": 1, "dimension": "Count" },
  "lifecycle_status": "active",
  "version": 1,
  "application_version": "0.1.0",
  "configuration_version": "regulated-device.1",
  "created_at": "2024-08-02T11:04:00Z"
}
```

`stocking_quantity` is an `AnyQuantity` whose amount is the unit magnitude in the stocking unit (one each), not a JSON integer. Time of record is server-stamped.

### 9.2 Lot — `LOT-BAR-24-4412`

```
GET /api/v1/lots/01932c5a-8b10-7001-8000-000000000004
```

```json
{
  "id": "01932c5a-8b10-7001-8000-000000000004",
  "identifier": "LOT-BAR-24-4412",
  "item_id": "01932c5a-8b10-7001-8000-000000000002",
  "parent_lot_id": "01932c5a-8b10-7001-8000-000000000003",
  "heat": "HT-ATI-24-8831",
  "expiry": { "value": "2027-09", "precision": "month" },
  "status": "available",
  "version": 2,
  "application_version": "0.1.0",
  "configuration_version": "regulated-device.1",
  "created_at": "2024-10-03T15:41:22Z"
}
```

`parent_lot_id` is the mill heat as a lot entity (PLAN §6b item 10: the tracked entity is a lot from the first posting). `expiry` is month-only; there is no `day` and none is invented (PLAN §3 acceptance 12). Creating a lot with `"identifier": "lot-bar-24-4412"` (lowercase) or twenty-one characters is 400 `VALIDATION` on `identifier`.

The heat itself:

```
GET /api/v1/lots/01932c5a-8b10-7001-8000-000000000003
```

```json
{
  "id": "01932c5a-8b10-7001-8000-000000000003",
  "identifier": "HT-ATI-24-8831",
  "item_id": "01932c5a-8b10-7001-8000-000000000002",
  "parent_lot_id": null,
  "certified_on": "2024-09-11",
  "certified_quantity": { "amount": "240.00000000", "unit": 3, "dimension": "Mass" },
  "expiry": null,
  "status": "available",
  "version": 1,
  "created_at": "2024-09-11T16:00:00Z"
}
```

`certified_on` is a calendar date, not an expiry.

### 9.3 Receipt — 86 bars on `PO-2024-0841`

Floor POST. Designed against the 500 ms scan-to-confirm budget (§8). Actor is the session (`M. Reyes`), not the body.

```
POST /api/v1/inventory/receipts
Idempotency-Key: 01932c5a-8b10-7001-8000-0000000000e0
X-CSRF-Token: <matches wicket_csrf>
```

```json
{
  "item_id": "01932c5a-8b10-7001-8000-000000000002",
  "lot_id": "01932c5a-8b10-7001-8000-000000000004",
  "location_id": "01932c5a-8b10-7001-8000-000000000008",
  "purchase_order": "PO-2024-0841",
  "quantity": { "amount": "258000.00000000", "unit": 2, "dimension": "Length" },
  "entered": { "amount": "86.00000000", "unit": 4, "dimension": "Count" },
  "unit_cost": { "amount": "0.007884", "currency": 840 }
}
```

`quantity` is stocking millimeters (86 bars × 3000 mm). `entered` is what was counted at the dock. `unit_cost` is `MoneyWire` at scale 6, USD. No `posted_at`, no `actor_id`.

Response (201):

```json
{
  "id": "01932c5a-8b10-7001-8000-0000000000f1",
  "status": "quarantined",
  "quantity": { "amount": "258000.00000000", "unit": 2, "dimension": "Length" },
  "entered": { "amount": "86.00000000", "unit": 4, "dimension": "Count" },
  "amount": { "amount": "2034.072000", "currency": 840 },
  "posted_at": "2024-10-03T15:41:22Z",
  "version": 1
}
```

`posted_at` is server time. `amount` is extended money at scale 6, not a float. Material received into quarantine is not available to the work order until a later status posting (PLAN §3 acceptance 4). Replaying the same `Idempotency-Key` with this body returns this body again; changing `quantity` under the same key is 409 `IDEMPOTENCY_CONFLICT`.

### 9.4 Work-order release — `WO-2026-1847`

```
POST /api/v1/production/work-orders/01932c5a-8b10-7001-8000-000000000006/release
Idempotency-Key: 01932c5a-8b10-7001-8000-0000000000e3
If-Match: "2"
```

OpenAPI on this operation (regulated-device profile):

```yaml
x-wicket-signature:
  meaning: Released
  permission: production.work_order.release
```

**Unsigned by declaration (plain-shop / slice until Wave 2b, edge `NotRequired`).** Body empty. Session actor is `M. Reyes`.

```json
{
  "id": "01932c5a-8b10-7001-8000-000000000006",
  "number": "WO-2026-1847",
  "item_id": "01932c5a-8b10-7001-8000-000000000001",
  "quantity": { "amount": "500.00000000", "unit": 1, "dimension": "Count" },
  "status": "released",
  "operation": { "seq": 20, "code": "TURN", "work_center": "WC-LATHE-03" },
  "version": 3,
  "released_at": "2026-03-14T15:02:11Z"
}
```

**Required edge, `NoSignatures` bound (regulated-device before Wave 2b).** Same request, with or without `X-Wicket-Signature`. The gate refuses; the work order stays unreleased:

```json
{
  "error": {
    "code": "SIGNATURE_NO_PROVIDER",
    "message": "This transition requires a signature and no signature provider is bound.",
    "field": null,
    "request_id": "01932c5a-8b10-7001-8000-00000000000d"
  }
}
```

HTTP 409. The row is unchanged. After Wave 2b the client mints per §5.2, sends `X-Wicket-Signature`, and the success body gains the `signature` object in §5.3.

### 9.5 Completion — 500 pieces, finished lot `LOT-WO-1847`

Floor POST, 500 ms budget. Completing a work order is a priced `TRANSFORMATION` group (PLAN §3 acceptance 5). The API does not accept a client-built posting list; the module contributes intents to the sink.

```
POST /api/v1/production/work-orders/01932c5a-8b10-7001-8000-000000000006/complete
Idempotency-Key: 01932c5a-8b10-7001-8000-0000000000e4
If-Match: "3"
```

```json
{
  "quantity": { "amount": "500.00000000", "unit": 1, "dimension": "Count" },
  "finished_lot_id": "01932c5a-8b10-7001-8000-000000000005",
  "serial_from": "SN-450-000134",
  "serial_to": "SN-450-000633",
  "location_id": "01932c5a-8b10-7001-8000-000000000009"
}
```

Response (200):

```json
{
  "id": "01932c5a-8b10-7001-8000-000000000006",
  "number": "WO-2026-1847",
  "status": "completed",
  "quantity": { "amount": "500.00000000", "unit": 1, "dimension": "Count" },
  "finished_lot": {
    "id": "01932c5a-8b10-7001-8000-000000000005",
    "identifier": "LOT-WO-1847",
    "item_id": "01932c5a-8b10-7001-8000-000000000001",
    "expiry": { "value": "2029-03-18", "precision": "day" }
  },
  "serials": {
    "from": "SN-450-000134",
    "to": "SN-450-000633",
    "first_id": "01932c5a-8b10-7001-8000-00000000000a"
  },
  "posted_at": "2026-03-18T09:14:03Z",
  "version": 4
}
```

A serial is a unit within the finished lot (PLAN §6b item 10). `posted_at` is server time of the transformation. The consumption edges that make genealogy total are not in this body; they are ledger facts, read through §9.6.

If this edge is `Required` under `NoSignatures`, the response is the same 409 `SIGNATURE_NO_PROVIDER` as §9.4.

### 9.6 Genealogy trace

Read-only. No idempotency key. Forward from the heat and backward from the finished lot return the same tree (PLAN §3 acceptance 8).

```
GET /api/v1/genealogy/trace?from_lot_id=01932c5a-8b10-7001-8000-000000000003&direction=forward
GET /api/v1/genealogy/trace?from_lot_id=01932c5a-8b10-7001-8000-000000000005&direction=backward
```

Either response:

```json
{
  "root": {
    "lot_id": "01932c5a-8b10-7001-8000-000000000003",
    "identifier": "HT-ATI-24-8831",
    "item_id": "01932c5a-8b10-7001-8000-000000000002"
  },
  "nodes": [
    {
      "lot_id": "01932c5a-8b10-7001-8000-000000000003",
      "identifier": "HT-ATI-24-8831",
      "quantity": { "amount": "240.00000000", "unit": 3, "dimension": "Mass" }
    },
    {
      "lot_id": "01932c5a-8b10-7001-8000-000000000004",
      "identifier": "LOT-BAR-24-4412",
      "quantity": { "amount": "258000.00000000", "unit": 2, "dimension": "Length" }
    },
    {
      "lot_id": "01932c5a-8b10-7001-8000-000000000005",
      "identifier": "LOT-WO-1847",
      "item_id": "01932c5a-8b10-7001-8000-000000000001",
      "quantity": { "amount": "500.00000000", "unit": 1, "dimension": "Count" },
      "serials": ["SN-450-000134"]
    }
  ],
  "edges": [
    {
      "from_lot_id": "01932c5a-8b10-7001-8000-000000000003",
      "to_lot_id": "01932c5a-8b10-7001-8000-000000000004",
      "kind": "split"
    },
    {
      "from_lot_id": "01932c5a-8b10-7001-8000-000000000004",
      "to_lot_id": "01932c5a-8b10-7001-8000-000000000005",
      "kind": "transformation",
      "work_order": "WO-2026-1847",
      "quantity_in": { "amount": "258000.00000000", "unit": 2, "dimension": "Length" },
      "quantity_out": { "amount": "500.00000000", "unit": 1, "dimension": "Count" }
    }
  ]
}
```

Quantities on the tree are `AnyQuantity` with string amounts. The identity seam is visible: millimeters of bar in, each of screw out. Value does not appear here; genealogy is the consumption graph (D2 §5.3), not the trial balance.

---

*Related: `03-module-system.md` (registration and compatibility), `05-data-model.md` (records these routes write), `02-architecture.md` §7 (budgets).*
