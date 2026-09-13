# Wave 2b (remaining kernel crates) — rulings record

Companion to `w2-rulings.md` and `w2s-rulings.md`. D-2b-1..9 were ratified by the decision authority on 2026-09-12 for `wicket-esign`; the full text is promoted here verbatim.

## D-2b-1 — The signature record

Schema `esign`, class `app` (`CONTRACT:468`: no DELETE, no TRUNCATE, no CASCADE). Three tables:

`esign.signature` (one row per signing, insert-only):
`signature_id uuid PK` (= `SignatureId`, the nonce — `traits-profiles.md:212-215`) · `signer_id uuid NOT NULL REFERENCES identity.principal(id)` · `signer_printed_name text NOT NULL` (snapshot at mint; inv 15, `docs/05:363`; never a live join) · `signer_username text NOT NULL` (snapshot) · `meaning text NOT NULL` · `reason text NULL` · `signed_at timestamptz NOT NULL` (`now()`, server time, D3 §4; never a bound parameter) · `signed_at_zone text NOT NULL` (signer's IANA zone; 11.50(a)(2) + preamble 101, `docs/06:225-231`) · `record_table text NOT NULL` / `record_id uuid NOT NULL` / `record_version bigint NOT NULL` · `doc_type text NOT NULL` (display/manifestation only) · `record_content_hash bytea NOT NULL CHECK (octet_length = 32)` · `record_snapshot jsonb NOT NULL` (the canonical bytes hashed) · `permission_snapshot text[] NOT NULL` (effective set at mint) · `credential_kind text NOT NULL CHECK IN ('signing_password','idp_step_up')` · `components_used text[] NOT NULL` · `signing_session_id uuid NULL` · `login_session_id uuid NULL` · `source_device text NULL` / `source_ip inet NULL` · `expires_at timestamptz NOT NULL` · `consumed_at timestamptz NULL` · `consumed_xid xid8 NULL` · `application_version`/`configuration_version` (inv 17).
Immutability: `wicket_app` holds `SELECT, INSERT` and `UPDATE (consumed_at, consumed_xid)` only — a `BEFORE UPDATE` trigger refuses any other column change; no DELETE grant anywhere. `consumed_at` is the single mutable fact and it is monotone (NULL → once).

`esign.meaning_policy` (`meaning PK, requires_reason bool, permission_hint text`) — the reason-text policy: a mint whose meaning has `requires_reason` and no `reason` is `VALIDATION`, mirroring `audit.reason_policy` (`audit.up.sql:91-94`) rather than duplicating it.
`transient.signing_session` (class `transient`, DELETE allowed) — D-2b-3.

**Audit link.** Exactly one audit row per signature, and nobody writes it: the `zz_audit_row` trigger attaches at `CREATE TABLE` and emits the `INSERT` on `esign.signature` (`docs/06:189-196`). The chain is the existing one — that row lands in `audit.event` (`audit.up.sql:45-79`) and its transaction is sealed into `audit.tx_seal` (`:106-117`). The consuming transition sets `wicket.esign_id` from `WriteContext.esign_id` (`wicket-db/src/lib.rs:45,123`), so every audit row of the transition carries `esign_id = signature_id` (`docs/10:297`) — that is the 11.70 technology link in the trail, and the content hash is the link on the record.

**Rejected:** a `signatures` array column on the business record (excisable by an UPDATE, unqueryable, and it puts 11.70 in module hands); and a bespoke esign hash chain (a second chain to validate — `audit.tx_seal` already seals the signature row's transaction).

## D-2b-2 — Manifestation read (11.50) — exact wire shape

Extends `docs/10:275-297` verbatim and adds only what 11.50/preamble 101 force:

```json
{ "signature": {
  "id": "01932c5a-…-e2", "signer_id": "01932c5a-…-0b", "printed_name": "M. Reyes",
  "meaning": "Released", "reason": null,
  "signed_at": "2026-03-14T15:02:11Z", "signed_at_zone": "America/New_York",
  "signed_at_local": "2026-03-14T11:02:11-04:00",
  "record": { "table": "sm.instance", "doc_type": "production.work_order",
              "id": "01932c5a-…-06", "version": 3 },
  "record_content_hash": "e3b0c442…b855",
  "credential_kind": "signing_password", "components_used": ["code","secret"],
  "superseded": false } }
```

`signed_at_local` is derived, not stored. The three 11.50(a) items are `printed_name`, `signed_at*`, `meaning` — all snapshots. **Rejected:** returning the live display name (preamble 102, `docs/06:263-265`).

## D-2b-3 — Minting: two components, always; the relaxation is defined and off

**Ruling: v1 ships `components = ["code","secret"]` on every signing.** D3 §9 (`:907-946`), `docs/06:273-280` and `docs/10:221` are not overturned — a session cookie on a shared work-centre tablet is not a component "designed to be used only by the individual", and tightening after a customer validates the loose behaviour invalidates their validation. What I *do* rule now is the shape the relaxation takes when a customer validates it, so enabling it is a flag and not a schema change:

- **Key.** `[signature_gate_binding] continuous_session = "off" | "on"`, `idle_timeout_secs = 300`, `max_window_secs = 900` — sub-keys of frozen SPEC-profiles **key 4**, not a twelfth root key (D-W1-5, `traits-profiles.md:371-374`). Both shipped profiles carry `"off"`.
- **"Continuous session" =** a `transient.signing_session` row `(id, principal_id, login_session_id, device_fingerprint, boot_epoch, opened_at, last_signed_at, closed_at, close_reason)`, opened by a full two-component signing. It is continuous while `now() < last_signed_at + idle_timeout_secs` **and** `now() < opened_at + max_window_secs` **and** `closed_at IS NULL`. It closes — recorded, never deleted before its `audit.log_event` — on logout, `login_session_id` change, device fingerprint or IP change, any failed signing attempt, a credential reset (`identity.credential_reset`), principal deactivation, a `boot_epoch` change, and any profile/config change.
- **First signing in the window uses both components; a continuation uses the signing secret** — the component executable only by the individual. Never the code alone, never the session. `components_used` on the row is the evidence, so an inspector can tell the two apart per signature.
- **State lives in `transient`**, because it is working state with no history (`CONTRACT:468`); the audited facts live on `esign.signature` and `audit.event`, which is where history belongs.

**Separable credential, no server dependency.** esign calls `wicket_identity::reauth_signing(tx, principal, secret)` (`session.rs:137-147`) → `verify_signing` (`credential.rs:67`), which reads `identity.signing_credential` — a different column family from `login_credential` (inv 14, `docs/05:362`). esign depends on identity (`PLAN.md:338`); `wicket-server` only transports the request. Using the login secret at `identification.secret` is `VALIDATION`, not a fallback.

**Token binding (D-W1-5 / D-W1-4).** `record_content_hash` = SHA-256 over the canonical JSONB of a module-registered projection of the business record **concatenated with** the `sm.instance` triple `(doc_type, doc_id, state, version)` read in the mint transaction. Both halves are needed: `sm.instance` is the version the executor checks (`exec.rs:170-180`), and it does not bump when the document body changes (`exec.rs:295-303`), so a body-only edit after minting must still refuse. The bytes hashed are stored in `record_snapshot`. `expires_at = signed_at + max_window_secs` (900 s default). Single use is **per signature** (`traits-profiles.md:219-228`), claimed inside the transition's transaction — see D-2b-4.

**Rejected:** minting inside the transition (CONTRACT §6.3 "verify, never mint"); hashing only `sm.instance` (defeats 11.70); a token-nonce column (the uuid v7 `SignatureId` *is* the nonce).

## D-2b-4 — How a synchronous gate claims a row inside the transition's Tx

`SignatureGate::verify` is `&self`, synchronous, and takes no `Tx` (`signature.rs:132-137`) — frozen. The resolution is a **per-transaction prepared gate**, not a singleton: `esign::prepare(tx, token, doc) -> PreparedGate` is called by the composition root inside the transition's transaction, immediately before `Engine::transition` (`kernel.rs:354`). `prepare` does, in that Tx: `SELECT … FROM esign.signature WHERE signature_id = $1 FOR UPDATE`, then the claim `UPDATE … SET consumed_at = now(), consumed_xid = pg_current_xact_id() WHERE signature_id = $1 AND consumed_at IS NULL RETURNING`, then recomputes the content hash from the live projection. `PreparedGate::verify` is then a pure in-memory comparison that reports the **first** failure in the frozen order. Atomicity is exact: a refusal aborts the transition, the rollback un-claims the row, and a commit commits signature-claim + `sm.instance` + audit rows as one transaction. Concurrency is the row lock: the loser sees `consumed_at` set and gets `Consumed`.

`bind_signature_gate` (`kernel.rs:631-635`) therefore returns a **factory**, not a gate: `GateBinding::NoSignatures` → the existing singleton; `GateBinding::WicketEsign` → the prepared-gate factory. **Rejected:** a task-local "current Tx" read inside a sync `verify` (`block_on` inside a runtime worker); claiming on a second connection (loses atomicity); amending core to an async `verify_in_tx` (the surface is frozen and thirteen lanes copied it).

## D-2b-5 — What each frozen check reads, and its error

| # | Check | Reads | Error on failure | Audited? |
|---|---|---|---|---|
| 0 | provider bound | the binding | `NoProvider` | silent (config, not misuse) |
| 1 | row | `esign.signature` FOR UPDATE: exists; `expires_at > now()`; signer `PrincipalStatus::Active`; `token.signer` == `signer_id` | `Invalid("no such signature" \| "expired" \| "signer inactive" \| "signer mismatch")` | **yes** |
| 2 | meaning | row `meaning` vs `required.meaning` (and vs `token.meaning`) | `MeaningMismatch` | **yes** |
| 3 | reference | row `(record_table, record_id, record_version)` vs the executor's live `RecordRef` (`exec.rs:174-178`); v5 token at v6 is here | `RecordMismatch` | **yes** |
| 4 | hashes | `token.record_content_hash` vs row vs the hash recomputed from the live projection in-Tx | `HashMismatch` | **yes** |
| 5 | permission snapshot | `required.permission.0 ∈ permission_snapshot` (no live RBAC read — CONTRACT:405-408) | `SignerNotPermitted` | **yes** |
| 6 | single-use claim | the conditional `UPDATE`'s row count | `Consumed` | **yes** |

A missing token on a `Required` edge is `Invalid("missing token")` (`exec.rs:175`) — audited. Audited failures are written by the composition root **after** the rollback, on a fresh transaction, via `audit.log_event` (`audit.up.sql:340`) as security events, because rows written inside an aborted transaction do not survive (D3 §9: failed signing attempts are security events, not business audit rows). Silent: `NoProvider` and `Unimplemented` only — those are a misconfigured build, caught at startup, and per-attempt logging of them is a flood vector.

## D-2b-6 — Profile binding

The key is `[signature_gate_binding] gate` (`profile.rs:287-288`), values `"NoSignatures"` | `"wicket-esign"` (`profile.rs:100-113`). Plain-shop keeps `gate = "NoSignatures"` (`plain-shop.toml:70-71`) and stays honest because it enables no `regulated = true` module (`plain-shop.toml:63-68`). Regulated-device flips to `gate = "wicket-esign"` at 2b.1 (`regulated-device.toml:71-72`); the startup guard stands — any `Required` edge in the enabled set with `NoSignatures` bound fails at startup and CI asserts no release profile binds it (`kernel.rs:563`, `traits-profiles.md:371-374`).

**A `Required` edge when esign is bound but the principal has no signing credential:** the transition is refused, never skipped. The refusal happens at **mint**, not at the gate — `POST /esign/signatures` returns 401 `SIGNATURE_REQUIRED` with `code = "SIGNATURE_REQUIRED"`, `field = "identification.secret"` (`docs/10:124-126`), and the attempt is an `audit.log_event` security event. With no token, the transition returns `Invalid("missing token")` → 401 `SIGNATURE_REQUIRED`. Seeding a role bundle that grants `calibration.approve` to a principal with no signing credential is therefore a configuration defect the first signing attempt surfaces loudly; esign does not mint a "credential-less" signature under any flag.

## D-2b-7 — Revocation, expiry, supersession

Deactivating a principal is a status change, never a delete (inv 13, `docs/06:236-242`). **Past signatures stay valid and stay readable**: manifestation renders from the snapshot columns, so it never joins a live principal row and never changes when the signer leaves. **Unconsumed** signatures of a deactivated principal refuse at check 1 (`Invalid("signer inactive")`), and their open signing sessions close with `close_reason = 'principal_deactivated'`. Expired unconsumed rows are **kept** with `consumed_at IS NULL` — nothing is deleted; expiry is read from `expires_at`, not from absence. A signature on a superseded version reads back unchanged with `"superseded": true` when the live `sm.instance.version > record_version`, plus `superseded_by_version`; the archival bundle always renders `record_snapshot`, never the live row — that is what makes a five-year-old DHR print reproduce.

## D-2b-8 — Published API surface

For `wicket-print` (`PLAN.md:341`), reads through `ReadPool` (legitimate under `CONTRACT:201`):
`esign::manifestation(&ReadPool, SignatureId) -> Result<Manifestation>` (the D-2b-2 struct) ·
`esign::archival_bundle(&ReadPool, SignatureId) -> Result<ArchivalBundle { manifestation, record_snapshot: serde_json::Value, record_content_hash: [u8;32], audit_event_ids: Vec<Uuid>, seals: Vec<SealRef { seq, xid, hash, prev_hash, sealed_at }>, anchor: Option<AnchorRef> }>` ·
`esign::verify_bundle(&ArchivalBundle) -> BundleVerification { hash_ok, chain_ok, anchored }` — pure, no database, so an exported bundle verifies on a separate machine (`docs/06:302-321`).

For `wicket-server` (shape only; the server implements transport, esign owns semantics):
`POST /api/v1/esign/challenges` → `{ components_required: ["code","secret"] | ["secret"], signing_session_expires_at, credential_kind }` ·
`POST /api/v1/esign/signatures` (Idempotency-Key) → the D-2b-2 body, per `docs/10:243-273` ·
`GET /api/v1/esign/signatures/{id}` → manifestation · `GET …/{id}/bundle` → archival bundle, permission `esign.bundle.read`.
Error codes are the frozen set (`docs/10:124`): `SIGNATURE_REQUIRED`, `SIGNATURE_NO_PROVIDER`, `VALIDATION`, `CONFLICT`. No new code is minted for `Consumed`/`HashMismatch` — both are 409 `CONFLICT` with a specific `message`, because a client must not branch on which misuse it committed.

## D-2b-9 — Named tests SPEC-esign must require (both profiles unless noted)

`two_component_first_signing_is_required` · `one_component_continuation_refused_when_relaxation_off` (v1 default; the positive `one_component_continuation_accepted_when_on` runs only with `continuous_session = "on"`) · `session_expiry_forces_two_components` (idle and max-window, two cases) · `signing_session_closes_on_device_change` · `token_is_single_use` · `concurrent_claim_one_wins_one_consumed` · `content_hash_mismatch_refuses` (body-only edit, instance version unchanged) · `meaning_mismatch_refuses` · `record_version_mismatch_refuses` · `permission_snapshot_not_live_rbac` (revoke after mint → still verifies; never held → `SignerNotPermitted`) · `transition_signature_and_audit_row_share_one_tx` (same `xid`, one `audit.tx_seal` row, `esign_id` on every audit row) · `refused_transition_rolls_back_the_claim` · `manifestation_wire_shape_is_exact` · `printed_name_is_a_snapshot_not_a_join` · `deactivated_signer_reads_back` (+ `deactivated_signer_cannot_consume_open_token`) · `superseded_version_reads_back_with_snapshot` · `signature_row_is_insert_only` (UPDATE of any column but `consumed_at` refused; no DELETE grant) · `login_secret_is_not_a_signing_component` · `failed_mint_is_a_security_event_not_a_business_row` · `required_edge_with_no_signing_credential_refuses_at_mint` · `plain_shop_binds_nosignatures_and_enables_no_regulated_module` · `regulated_device_startup_fails_if_required_meets_no_signatures` · `archival_bundle_verifies_offline` · `every_esign_table_is_audited` · `hash_and_secret_columns_are_redacted_in_audit` · `writes_go_through_tx` (the `CONTRACT:201` fence test) · `migrate_down_then_up` · `esign_tables_owned_by_wicket_owner`.

DECISION: Ratified as D-2b-1..9 — an insert-only `esign.signature` row carrying the printed-name/zone/permission/component snapshots and audited by the standard trigger into the existing seal chain, two identification components on every signing with the continuous-session relaxation fully specified but shipped `off` under SPEC-profiles key 4, a content hash binding the business projection *and* the `sm.instance` triple, and single use claimed by a per-transaction prepared gate inside the transition's own transaction so that the frozen synchronous `SignatureGate` never needs to change.

## D-2b-10 — One canonical order, one entry point (2026-09-13)

`CANONICAL_ORDER` in `wicket-module` is the source of truth and extends `KERNEL_ORDER` to every migrating crate: `wicket-db, wicket-audit, wicket-identity, wicket-numbering, wicket-uom, wicket-events, wicket-jobs, wicket-ledger, wicket-statemachine, wicket-esign, wicket-customfields, wicket-documents, wicket-print, wicket-module`, then `wave_2s1_order()` (`items, locations, lots`), then `slice_migrators()` (`inventory, production-min, genealogy, server`). It must be proved a topological sort of the CONTRACT §4 edge set by `is_topological_sort`, exactly as `KERNEL_ORDER` is today. Nothing outside `wicket-module::order` may name a migrator list.

## D-2b-11 — The event trigger is UP for the whole install

`install_privileged` runs **once**, immediately after `wicket-audit`, and is never dropped mid-run. Every table a migration creates must be in exactly one of three lawful states: (a) **audited** — the schema carries `ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA <x> GRANT TRIGGER ON TABLES TO wicket_owner` *before* the first `CREATE TABLE`, and the migration **also** ends with an explicit idempotent `SELECT audit.attach('<x>.<t>'::regclass)` per table; (b) **exempt** — an `audit.exempt` row inserted *before* the `CREATE TABLE`; (c) **transient/audit/wicket class** — skipped by the trigger. Manual attach is **required, never forbidden**: `audit.attach` early-returns when both triggers exist, so it is a no-op under the trigger and the only coverage without it. A migration that writes rows to an attached table must carry the six-GUC migration preamble. Every audited table needs a PK.

## D-2b-12 — Two kernel fixes make D-2b-11 honest

(1) `audit.attach_new_tables` must derive its skip set from `wicket.schema_class` (`class IN ('transient','audit')` or `nspname = 'wicket'`), not the hardcoded literal list — the literal `'transient'` misses every `<module>_transient` schema R-2s-1 mandates. (2) `wicket_db::migrate::run` must set the six `wicket.*` migration GUCs on its own connection for the whole run, so `wicket.schema_history` can be attached and a **later** `run` (runtime module install) still records history.

A transient-class schema (`server_transient`, `inventory_transient`, …) must never be offered `GRANT TRIGGER ON TABLES TO wicket_owner`. That grant is what made `inventory_transient.idempotency` acquire `zz_audit_row` on one order and not the other.

## D-2b-13 — No harness may hand-roll an order

`tests/common/mod.rs` calls one published entry point — `wicket_module::order::install_upto(&migrate_pool, &bootstrap_pool, "<crate>")`, which runs `CANONICAL_ORDER` through the crate named and returns. `wicket-db` and `wicket-audit` are the bootstrap pair and install themselves (they *are* `MIGRATE_PREFIX`); every crate from `wicket-identity` on installs through the graph, adding `wicket-module` as a dev-dependency (the `wicket-module` ↔ `wicket-mod-items` dev-dep cycle already proves Cargo permits this). `attach_kernel_audit`/`attach_slice_audit`/`KERNEL_AUDIT_RELS`/`SLICE_AUDIT_RELS` become a belt-and-braces assertion, not the coverage mechanism: under D-2b-11 every crate attaches its own.

## D-2b-14 — The enforcing tests

(1) The matrix lives in **`crates/wicket-module/tests/migrate.rs`**, named exactly **`every_crate_migrates_in_both_trigger_states`** — `wicket-module` is the only crate that can see every migrator. For each entry of `CANONICAL_ORDER`: fresh DB, predecessors applied, then the crate applied **twice over two databases** — once at its canonical position with `audit_attach` up, once as the last crate on a DB where the trigger was installed after the predecessors — and it asserts both runs green **and** that the resulting `(nspname, relname)` set carrying `zz_audit_row` is *identical* between the two, which is what D-2b-12(1) actually buys. (2) **`migrate_down_then_up` stays per-crate**, name unchanged, but its harness now reaches the crate's position via `install_upto`, so down-then-up is exercised with the trigger up. `wicket-customfields` and `modules/locations` gain the test where missing.

## D-2b-15 — Fail-class wording for `docs/11-module-common-rules.md`

**R-2b-1 one install order (2026-09-13).** `wicket_module::order::CANONICAL_ORDER` is the only install order. A crate's migrations must run green **both** at their canonical position and as the last crate on a fresh database, with the `audit_attach` event trigger installed and never dropped, and must leave an **identical** set of audit-attached relations either way. A migration therefore declares `ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA <x> GRANT TRIGGER ON TABLES TO wicket_owner` before its first `CREATE TABLE`, ends with an idempotent `SELECT audit.attach(…)` for every audited table, registers any exemption in `audit.exempt` before the table exists, and carries the six-GUC migration preamble if it writes an attached row. A test harness that hand-rolls a migrator list instead of calling `install_upto`, a migration that requires the trigger to be absent, and an install order that changes which relations end up audited are each **Fail-class**.

## R-2b-lock — Lockfile churn is never fail-class (2026-09-13)

**Ruling: a `Cargo.lock` diff is never a fail-class finding against a lane.** Audits and adjudications must not fail a lane on lockfile churn. **CONTRACT:** lanes never commit `Cargo.lock`; the integrator regenerates it once on the integrate branch (`cargo generate-lockfile`) at landing.

Origin: ruled by the orchestrator 2026-09-13 01:11 after `audit-wicket-documents-r2-x2` failed a rework leg partly on lockfile churn; applied to the wicket-documents adjudication charter; recorded in `_team/reports/CLOSURES.md`.
