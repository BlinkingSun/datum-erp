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

## Known gap — `POST .../issue` (R-2s-7)

`issue_wo` (issue material + start the work order) is one HTTP mutation and
must be one `Tx::begin` / one `WriteContext` (SPEC ADDENDUM 1 item 3, docs/10
§4). A `Tx` binds one action and cannot rebind: `issue_material` requires
`inventory.issue` and `start` requires `production.issue`.

Until follow-up **`2s4-onetx`** (production_min start hook so the server calls a
single WO `start` transition), this crate keeps the live path as **two audited
transactions**. That is a ruled gap (R-2s-7), not a fake one-Tx. The named
test `issue_wo_is_one_transaction` asserts the SPEC intent (exactly one
`Tx::begin`, one `audit.tx_seal` row, atomic rollback of the issue if start
fails) and is `#[ignore]` with that reason — never weakened to `begins >= 1`.
