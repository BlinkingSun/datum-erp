# datum-server

HTTP API process for Datum. Owns `crates/datum-server/**` from Wave 2s (PLAN §3).
The UI consumes the OpenAPI client generated from `/api/v1/openapi.json` (ADR 0009).
Browsers on the LAN are the v1 clients; server-rendered documents are Wave 2b.

## Binary

```
datum serve --profile <regulated-device|plain-shop> --bind 0.0.0.0:8080
datum migrate
datum db check
datum iq
datum manifest export
```

Configuration is a TOML file (`--config` / `DATUM_CONFIG`) plus environment.
Database URLs are D3's names: `DATUM_DATABASE_URL`, `DATUM_MIGRATE_DATABASE_URL`,
`DATUM_BOOTSTRAP_URL`. There is no bundled database (D5).

## Service files (`dist/`)

The server is a service that survives logout (D3 §11):

| File | Platform |
|---|---|
| `datum.service` | systemd |
| `com.datum.server.plist` | launchd |
| `install-windows-service.ps1` | `sc create` around the binary |

**Datum never owns the database.** PostgreSQL is an external cluster. These unit
files start only the API process.

## TypeScript client

OpenAPI is served at `GET /api/v1/openapi.json`. Generate a typed client (not
executed in this crate):

```
npx openapi-typescript http://127.0.0.1:8080/api/v1/openapi.json -o src/api.d.ts
```

The first-party UI uses that client exclusively (ADR 0009).

## Electronic signature HTTP (D-2b-5 / D-2b-8)

| Condition | `code` | HTTP |
|---|---|---|
| No signing credential at mint (`identification.secret`) | `SIGNATURE_REQUIRED` | 401 |
| Login secret presented as the signing component | `VALIDATION` | 400 |
| Missing token on a `Required` edge | `SIGNATURE_REQUIRED` | 401 |
| Dummy / invalid token while `datum-esign` is bound | `SIGNATURE_REQUIRED` | 403 |
| Signer lacks the snapshotted permission | `SIGNATURE_REQUIRED` | 403 |
| `NoSignatures` bound (`NoProvider`) | `SIGNATURE_NO_PROVIDER` | 409 |
| `Consumed` / `HashMismatch` | `CONFLICT` | 409 |

Regulated-device binds `datum-esign`, so a dummy token is **403 `SIGNATURE_REQUIRED`**, not 409 — 409 is only `NoProvider`. Plain-shop stays `NoSignatures` and enables no `Required` edge. Clients send the minted id as `X-Datum-Signature` (docs/10 §5.2). `POST /esign/signatures` records the idempotency replay in the **same** mint transaction.

## HTTP API (Wave 2s slice)

| Method | Path | Permission |
|---|---|---|
| POST | `/api/v1/inventory/receipts` | `inventory.receive` |
| POST | `/api/v1/inventory/releases` | `lots.release` |
| POST | `/api/v1/inventory/counts` | `inventory.count` |
| POST | `/api/v1/inventory/reversals` | `inventory.adjust` |
| GET | `/api/v1/inventory/on-hand` | `inventory.view` |

`POST /api/v1/inventory/reversals` takes `{ "document_id": "<posted issue id>", "reason": "..." }`.
One `Tx::begin` / one `WriteContext` (R-2s-7, no GUC rebind) calls
`datum_mod_inventory::reverse_posted_issue`. **201** is the issue document plus
`reversal_group_id`; unknown id is **404**; an already-reversed group is **409**.

## `POST .../issue` (R-2s-7)

`issue_wo` is one `Tx::begin` / one `production.issue` action: the handler
calls `production_min::start` with embedded issue lines; inventory postings are
contributed on the transition's bound `PostingSink` via the module hook. The
named test `issue_wo_is_one_transaction` asserts one begin, one `audit.tx_seal`
row, and atomic rollback when start fails.

## Documents HTTP (Wave 3)

SPEC-documents names no HTTP routes: the API is the sealed `Tx` plus a
manifest export. PLAN.md §3 Wave 2b has no documents HTTP exit criterion;
SPEC-server-slice covers the Wave 2s inventory/production slice only. Documents
HTTP is deferred to Wave 3 (interface).

