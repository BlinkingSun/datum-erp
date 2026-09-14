# Plan-audit slice 2 — Audit trail in the persistence layer (ADR 0005) vs SQLx

Task: `erp` · Slice: `audit-sqlx` · Date: 2026-09-11 · Researcher: grok-4.6 (adversarial)

Sources: ADR 0005, ADR 0002, ADR 0003, `docs/02-architecture.md` §§2/5/8, PLAN.md §§5–6 invariants 3–5, SQLx 0.7/0.8/0.9 docs and README, PostgreSQL 15–18 docs (`set_config`/`current_setting`, COPY, event triggers, `session_replication_role`), 21 CFR 11.10(e) and FDA 2003 Scope & Application guidance, 2ndQuadrant/wiki audit-trigger limitations.

---

## Verdict (read this first)

**SQLx does not have a clean interception point.** There is no Unit-of-Work, no ActiveRecord callback, no query middleware, and no executor wrapper that can see every `INSERT`/`UPDATE`/`DELETE` and attach an audit row. `query!` / `query_as!` compile to ordinary prepared statements executed on whatever `Executor` the caller hands them. Pool lifecycle hooks (`after_connect` / `before_acquire` / `after_release`) are connection-lifetime, not statement-lifetime. The `Executor` trait is explicitly “not for general use,” is **not dyn-compatible**, and as of 0.7 no longer even implements on `Transaction`/`PoolConnection` (callers pass `&mut *tx`).

Therefore ADR 0005’s sentence *“audit records are produced by the persistence layer … a module author cannot forget to write one and cannot write a false one”* is **not expressible in SQLx**. It is expressible in PostgreSQL (row triggers + fail-closed session context + `SECURITY DEFINER` insert). A sealed `Write` trait, a proc-macro, or a CI grep is house style: every Wave 2 crate can violate it with one `sqlx::query!`.

**The PLAN is missing a mechanical enforcement lane.** Wave 1 `workspace` must ship roles, grants, two connection URLs, the session-context helper, the audit-attach event trigger, and a lint that is allowed to fail the build. Wave 2 `wicket-db` / `wicket-audit` cannot invent this per crate.

**Opus DECISION is required** before any of those crates is typed. Recommended decision (not a substitute for opus): **both** — triggers are the guarantee; the sealed trait is the ergonomic way to set context. See §9.

A second, independent failure: ADR 0005 grants the application role `INSERT` on the audit table. That grant is exactly the ability to write a false audit row. The ADR contradicts itself.

A third: grant-level append-only is real against `wicket_app` and is theater against the actual self-hosted threat (shop admin / cluster owner / `psql` / backup restore / `DISABLE TRIGGER`). PLAN invariant 3 as written overclaims. 21 CFR 11.10(e) does **not** require a hash chain; it also is not satisfied by “the app role cannot UPDATE” on a box where the same Windows user is superuser.

---

## 1. What SQLx actually is (0.7 / 0.8 / 0.9)

ADR 0002 picks “Rust, with Axum for HTTP and SQLx for database access” and does not pin a version. Current docs.rs `latest` is **0.9.0** (2026-07-20). 0.8.x is the line most 2025/early-2026 code still cites. None of 0.7, 0.8, or 0.9 adds an interceptor.

Facts that bind the design:

| Claim | Reality |
|---|---|
| ORM / UoW / callbacks | SQLx README: **“SQLx is not an ORM.”** No identity map, no `before_save`, no dirty tracking. |
| Compile-time queries | `query!`, `query_as!`, `query_scalar!`, `query_file!` talk to Postgres at compile time (or read `.sqlx/` offline) and expand to a `Query`/`Map` that `.execute(executor)` / `.fetch_*`. The SQL string is a literal. Nothing in the expansion writes a second statement. |
| Runtime queries | `sqlx::query()`, `query_as()`, `QueryBuilder`, `raw_sql()` (0.8+). Same execution path. |
| `Executor` | Building-block trait bound. Implemented for `&Pool` and `&mut Connection`. **Not dyn-safe.** Wrapping it does not wrap callers who still have the inner `PgPool`. |
| 0.7 breakage | `Transaction` and `PoolConnection` **lost** their `Executor` impls. Canonical call is `query!(...).execute(&mut *tx)`. A wrapper around `Transaction` is extra friction every Wave 2 crate will work around by taking `impl Executor`. |
| Pool hooks | `PoolOptions::after_connect`, `before_acquire`, `after_release`. Documented use: `SET application_name`, `SET search_path`. They cannot see subsequent DML. |
| Query tracing crates | `sqlx-tracing` wraps a pool to emit spans. Observability, not mutation. It does not rewrite or pair statements. |
| SQLite-only hooks | `SqliteConnection::set_update_hook` / `set_preupdate_hook`. **Postgres driver has no equivalent.** ADR 0003 forbids SQLite as the system of record. |
| COPY | `PgConnection::copy_in_raw` / `PgCopyIn`. Bypasses `query!`. Postgres **does** fire row triggers on `COPY FROM` (PG docs: “COPY FROM will invoke any triggers and check constraints … it will not invoke rules”). |
| Nested tx | `Connection::begin` on an active tx is a savepoint (`SAVEPOINT _sqlx_savepoint_N`). |
| Migrations | `sqlx::migrate!` / `sqlx-cli` run `.sql` files as the connected role. No separate owner role unless we invent one. |

There is no SQLx feature, open PR, or adjacent crate that gives “run this on every write.” Anyone claiming otherwise is describing Diesel (`#[diesel(belongs_to)]` / `insert_into`), SeaORM (`ActiveModel` hooks), or Rails.

### Why an Executor wrapper is a trap

A house `struct Auditing<E>(E)` that implements `Executor` by inspecting `Execute::sql()` and injecting an audit insert would:

1. See the SQL **text**, not OLD/NEW row images. You cannot log prior values without a `SELECT` you don’t have.
2. Miss `COPY`, `TRUNCATE`, `sqlx::raw_sql` batches, and any caller that still holds `&PgPool`.
3. Fight the macros: `query!` is happy to take `&pool` directly. The wrapper is opt-in.
4. Parse SQL. `QueryBuilder`, CTEs, `INSERT…SELECT`, `MERGE` (PG 15+), and `UPDATE … FROM` will be wrong.
5. Not be object-safe, so it cannot be stuffed in request extensions as `dyn Executor`.

This is not a persistence layer. It is a leaky decorator.

---

## 2. Interception design — which of (a)/(b)/(c) actually satisfies ADR 0005

ADR 0005 wants two properties at once:

- **Cannot forget** — a write without an audit row is impossible, including for modules not yet written and for third-party modules.
- **Cannot write a false one** — a module cannot insert an audit row that did not happen, or omit one that did.

Plus: same transaction; server time; attributable actor; business intent (reason / document / action) that a trigger cannot see by looking at OLD/NEW.

### (a) Database triggers + session context

This is the **only** mechanism that can satisfy “cannot forget” for every write that reaches PostgreSQL: application DML, `COPY FROM`, `psql`, a hurried Wave 2 crate using `query!`, a third-party module, a future plugin that talks SQL.

Sketch (normative, not yet in any SPEC):

```sql
-- Fail closed. Actor is not optional.
CREATE OR REPLACE FUNCTION audit.row_change() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = audit, pg_temp
AS $$
DECLARE
  actor text := current_setting('wicket.actor_id', true);
  reason text := current_setting('wicket.reason', true);
  source_type text := current_setting('wicket.source_type', true);
  source_id text := current_setting('wicket.source_id', true);
  action text := current_setting('wicket.action', true);
BEGIN
  IF actor IS NULL OR actor = '' THEN
    RAISE EXCEPTION 'wicket: write without actor (SET LOCAL wicket.actor_id)'
      USING ERRCODE = '42501';
  END IF;
  INSERT INTO audit.event (
    at, actor_id, schema_name, table_name, op,
    row_pk, old_row, new_row, reason, source_type, source_id, action
  ) VALUES (
    clock_timestamp(),  -- wall; now() is tx-start. Pick one and freeze it.
    actor,
    TG_TABLE_SCHEMA, TG_TABLE_NAME, TG_OP,
    CASE WHEN TG_OP = 'DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END -> 'id',
    CASE WHEN TG_OP IN ('UPDATE','DELETE') THEN to_jsonb(OLD) END,
    CASE WHEN TG_OP IN ('INSERT','UPDATE') THEN to_jsonb(NEW) END,
    reason, source_type, source_id, action
  );
  RETURN NULL; -- AFTER trigger
END;
$$;

CREATE TRIGGER audit_row
  AFTER INSERT OR UPDATE OR DELETE ON <table>
  FOR EACH ROW EXECUTE FUNCTION audit.row_change();

CREATE TRIGGER audit_truncate
  AFTER TRUNCATE ON <table>
  FOR EACH STATEMENT EXECUTE FUNCTION audit.stmt_truncate(); -- must exist
```

Attach automatically so modules not yet written cannot skip it:

```sql
CREATE EVENT TRIGGER audit_attach_on_create
  ON ddl_command_end
  WHEN TAG IN ('CREATE TABLE')
  EXECUTE FUNCTION audit.attach_if_in_audited_schema();
```

Non-audited tables (ADR 0005 last cost bullet) are a **catalog decision**, not a per-write flag: e.g. `audit.exempt(schema, table, reason, decided_by)` written by `wicket_migrate`, itself audited as DDL. An event trigger that skips `audit.exempt` is the implementation of “declaration is a reviewed, recorded decision.”

**This is the structural half.** It does not by itself supply business intent. That is the GUC protocol in §3.

**“Cannot write a false one” requires a grant the ADR currently refuses to make.** If `wicket_app` has `INSERT` on `audit.event`, any module can insert a fake row. The trigger function must be `SECURITY DEFINER`, owned by a NOLOGIN owner role, and `wicket_app` must have **SELECT only** on `audit.event`. The trigger owner has `INSERT`. That is the only way “cannot write a false one” is a grant, not a vibe.

ADR 0005 as written: *“The application role has insert and select. It does not have update or delete.”* That sentence must be amended. INSERT-on-app-role is how you get a forged trail.

### (b) Sealed `Write` trait in `wicket-db` + lint forbidding `sqlx::query` in module crates

A repository/UoW you write yourselves:

```rust
// wicket-db, public, sealed
pub struct Tx<'a> { /* PgTransaction + Actor already SET LOCAL */ }

impl Tx<'_> {
    pub async fn begin(pool: &AppPool, actor: Actor) -> Result<Self>;
    // the ONLY legal write surface
}

pub trait Write: Sealed { /* not implemented outside wicket-db */ }
```

This is the **ergonomic** way to set GUC context and to keep `query!` inside kernel crates. It does **not** satisfy ADR 0005 by itself:

- `sqlx::query!` is a macro. Clippy cannot ban it with `disallowed_macros` unless you list every `sqlx::query*` path, and even then `conn.execute("INSERT …")` and `QueryBuilder` and `raw_sql` and `copy_in_raw` remain.
- CI `rg` is a process control. Wave 2 crates in isolated worktrees can merge a violation that a later grep catches — that is “correct if remembered,” which ADR 0005 rejected.
- Third-party / future runtime plugins (deferred, but the ADR’s whole point is modules not yet written) will not use the trait.
- `psql`, restore, and `COPY` will not use the trait.

A lint is **defense in depth**, mandatory, and insufficient. PLAN must still have it (see §7). Do not sell it as the guarantee.

### (c) Proc-macro on entities

This is ActiveRecord callbacks with extra compile times. It only fires if every write goes through generated `insert`/`update`. Bulk SQL, `UPDATE … SET status = $1 WHERE id = ANY($2)`, ledger postings (many rows, no entity), numbering, and jobs will not. It also invites the false-audit problem: the macro can write whatever the caller puts in `#[audit(action = "whatever")]`.

Reject as the enforcement mechanism. A derive that *emits* `query_as!` against the sealed `Tx` is fine as sugar.

### Combination that actually satisfies the ADR

| Property | Mechanism |
|---|---|
| Cannot forget the row image | AFTER ROW trigger + event trigger on `CREATE TABLE` + TRUNCATE statement trigger |
| Cannot write a false audit row | `wicket_app` has **no INSERT** on `audit.event`; only `SECURITY DEFINER` trigger function inserts |
| Same transaction | Trigger runs in the writer’s tx; rollback drops both |
| Server time | `clock_timestamp()` or `statement_timestamp()` in the trigger; never a bound parameter |
| Attributable actor | Fail-closed `current_setting('wicket.actor_id', true)` |
| Business intent (reason/source/action) | GUC / SET LOCAL, set by `wicket_db::Tx::begin`; required per-table where the catalog says so |
| Modules cannot bypass in-app | CI lint + `disallowed_methods`/`disallowed_macros` on `sqlx::query*` outside `wicket-db`/`wicket-audit` + deny `copy_in_raw` in module crates |
| Writes from outside the app | Triggers still fire; actor missing → statement aborted (fail closed). Migrate role is the exception, logged. |

**(a) is the guarantee. (b) is how application code sets context without remembering SET LOCAL on every handler. (c) is optional sugar. (b) without (a) is the thing ADR 0005 exists to forbid.**

Trigger-only, as the ADR already says, is “partially adopted” because OLD/NEW is not intent. That is correct. The missing half is not a SQLx interceptor. It is a **fail-closed session-context protocol** plus a helper that is the only way `wicket_app` is allowed to start a write transaction.

---

## 3. Session-context protocol and its failure modes

### Protocol (normative sketch)

Custom GUCs with a dotted name need **no** `postgresql.conf` entry since PG 9.2 (`custom_variable_classes` is gone). `SET wicket.actor_id = '…'` just works as a placeholder GUC.

**Always `set_config(..., is_local := true)` inside an open transaction. Never `SET` (session).** `SET LOCAL` / `set_config(…, true)` is the SQL equivalent.

```sql
-- NOT valid: SET LOCAL does not take $1 bind parameters
-- SET LOCAL wicket.actor_id = $1;

-- Valid, and what SQLx must emit:
SELECT set_config('wicket.actor_id', $1, true);
SELECT set_config('wicket.actor_kind', $2, true);     -- 'user' | 'service'
SELECT set_config('wicket.reason', $3, true);
SELECT set_config('wicket.source_type', $4, true);
SELECT set_config('wicket.source_id', $5, true);
SELECT set_config('wicket.action', $6, true);
SELECT set_config('wicket.request_id', $7, true);
```

`wicket-db` owns this. `Tx::begin(pool, ctx: WriteContext) -> Tx` runs `BEGIN` then the `set_config` batch. There is no `pool.begin()` in module crates.

Trigger reads with `current_setting(name, true)` (missing_ok). Unset → NULL → `RAISE`. Empty string is also illegal.

Time: do not read a GUC for time. `clock_timestamp()` in the trigger. (Decision: `now()`/`transaction_timestamp()` is stable for the whole tx; `statement_timestamp()` is per statement; `clock_timestamp()` is wall. For “time of record” of a multi-statement tx, `now()` is the honest stamp. Freeze this in the DECISION.)

### Failure modes (adversarial)

| Mode | What happens | Required mitigation |
|---|---|---|
| **Connection-pool GUC leakage** | `set_config(..., false)` or `SET wicket.actor_id` (session) survives `COMMIT` and is still set when SQLx returns the conn to the pool. Next HTTP request on that conn audits as the previous operator. SQLx does **not** `DISCARD ALL` on release. | Ban session-level SET for `wicket.*`. Only `is_local=true` after `BEGIN`. Optional belt: `after_release` runs `DISCARD ALL` / `RESET ALL` — but `DISCARD ALL` cannot run inside a tx, so it is a release-path thing. `Tx::begin` must still SET LOCAL. |
| **Forgotten SET LOCAL** | If trigger is fail-open, you get an audit row with NULL actor — ADR 0005 (“no anonymous system change”) is dead. If fail-closed, the business write aborts. | Fail closed. The helper makes this the default; the trigger makes it unskippable. |
| **Prepared statements** | `SET LOCAL name = $1` is a syntax error. People will try it. | Only `set_config`. Document in `wicket-db`. `query!` handles `set_config` fine. |
| **Nested transactions / savepoints** | SQLx inner `begin` = `SAVEPOINT`. `SET LOCAL` is transaction-scoped; `ROLLBACK TO SAVEPOINT` reverts GUC to the outer tx’s values (PG GUC rollback). `RELEASE SAVEPOINT` keeps inner SET LOCAL. PL/pgSQL `BEGIN…EXCEPTION` is a subtransaction: `set_config(is_local)` **inside** the exception block is discarded on error (known footgun). | Set context in the Rust `Tx::begin`, not inside PL/pgSQL exception blocks. Triggers only **read**. |
| **COPY** | `COPY FROM` fires row triggers (PG docs). It does **not** set GUCs. Fail-closed trigger aborts the COPY unless the session already has context. SQLx `copy_in_raw` is a second write path with no `query!`. | Allow COPY only on `wicket_migrate` (seed/restore) or after `Tx::begin`. Lint-ban `copy_in_raw` in module crates. |
| **TRUNCATE** | Row triggers do not fire. Statement-level `AFTER TRUNCATE` must exist or truncates are silent. `TRUNCATE` is also a separate privilege. | Always attach a TRUNCATE trigger. Do not `GRANT TRUNCATE` to `wicket_app`. |
| **Migrations** | `sqlx::migrate!` executes as the connected user. `wicket_app` cannot `CREATE TABLE`. If migrations run as superuser, they also skip the two-role story unless the last statements `GRANT`/`REASSIGN`. Event trigger on `CREATE TABLE` will try to attach audit triggers during migrations — good, unless the migrate role cannot insert into `audit.event` and the event trigger is not `SECURITY DEFINER`. | Wave 1: `DATABASE_URL` for migrate ≠ app. Event-trigger function `SECURITY DEFINER`. |
| **`session_replication_role = replica`** | User triggers **do not fire**. Superuser-only to SET by default (context = superuser). `wicket_app` must not have `SET` on this GUC. Owner/superuser can. This is the standard “bypass audit triggers” switch. | `REVOKE SET ON PARAMETER session_replication_role FROM wicket_app` (PG 15+ parameter ACLs). Still does not bind the owner. |
| **`ALTER TABLE … DISABLE TRIGGER`** | Table owner can disable the audit trigger. Then writes are silent. | Owner is `wicket_owner` NOLOGIN. App and migrate are not owners. Still does not bind superuser. |
| **PgBouncer** | Transaction pooling: session `SET` leaks across clients; `SET LOCAL` is OK. Statement pooling: `SET LOCAL` does not survive to the next statement. | If we ever pool externally: transaction or session pooling only. Bundled desktop PG has no pgbouncer; still write the constraint down. |
| **GUC is text, attacker-settable** | `wicket_app` can `set_config('wicket.actor_id', 'someone-else', true)` and impersonate. The trigger trusts the GUC. | The sealed `Tx::begin` takes a kernel `Actor` already authenticated by `wicket-identity`, not a string from the handler. Lint so modules cannot call `set_config` themselves (`disallowed_macros` / grep `set_config` / `current_setting`). This is house style on top of (a). Without it, “cannot write a false one” fails for **actor**, even if the row image is true. |
| **CTE / `set_config` in unused WITH** | An unreferenced CTE is not executed; `set_config` in it never runs. | Do not set context in the same SQL as the DML. Set it as prior statements on the tx. |
| **Background jobs** | ADR 0005: named service principal. A job that opens a pool connection and writes without `Tx::begin` dies fail-closed. | `wicket-jobs` must go through `Tx::begin(Actor::service("wicket-jobs"))`. PLAN crate graph does not give `wicket-jobs` an `audit` or `identity` edge (only `core db events`). **That is a demanded edge or a `Actor` in `wicket-core` plus `Tx` in `wicket-db`.** Flag for slice 3. |
| **`after_connect` SET timezone / datestyle** | Safe and required. Server time is timezone-dependent if we store `timestamptz` vs `timestamp`. | `after_connect`: `SET timezone = 'UTC'`, `SET extra_float_digits`, `SET application_name`. Not actor. |

Transaction-local session **table** (`CREATE TEMP TABLE wicket_tx_ctx ON COMMIT DROP`) is the alternative to GUC. Temp tables in a pool leak if not `ON COMMIT DROP`; they also need `INSERT` that can itself recurse if that table is audited. **GUC SET LOCAL is the less wrong tool.** Do not do both.

---

## 4. Grant / role design — and yes, this is a missing Wave 1 subtask

PLAN.md invariant 3: *“The audit table is append-only at the grant level. The application role holds insert and select and does not hold update or delete.”*

PLAN.md never defines an application role. Wave 1 `workspace` owns `dev/` and migrations stubs and does not mention `CREATE ROLE`. ADR 0003 cites “table-level and column-level grants” as a reason to pick Postgres and then does not name the roles either.

**Two roles is the minimum. Three is the one that matches the ADR’s words.**

| Role | Login | Owns | May |
|---|---|---|---|
| `wicket_owner` | NOLOGIN | all tables, trigger functions | nothing at runtime; `REASSIGN OWNED` target |
| `wicket_migrate` | LOGIN | member of a migrate group; `SET ROLE wicket_owner` for DDL | `CREATE`/`ALTER`/`GRANT`; **not** the app pool URL |
| `wicket_app` | LOGIN | nothing | DML as granted: `SELECT, INSERT, UPDATE, DELETE` on business tables; **`SELECT` only** on `audit.event`; `USAGE` on sequences; **no** `TRUNCATE`, **no** `TRIGGER`, **no** `SET session_replication_role` |
| `wicket_audit_writer` | NOLOGIN | `audit.row_change` | `INSERT` on `audit.event` (used only via `SECURITY DEFINER`) |

`sqlx::migrate!` **must** use `postgres://wicket_migrate@…`. The app pool **must** use `postgres://wicket_app@…`. Two env vars. Wave 1 `dev/` and CI compose/testcontainers must create both.

DDL vs append-only: **workable**. Migrations never connect as `wicket_app`. The app role never needs DDL. The tension the question poses (“grants vs migrations”) is only a problem if a single role does both. That is the PLAN gap.

Initial migration (owned by `workspace` or `wicket-db`, not by a Wave 2 afterthought):

```sql
CREATE ROLE wicket_owner NOLOGIN;
CREATE ROLE wicket_audit_writer NOLOGIN;
CREATE ROLE wicket_migrate LOGIN;
CREATE ROLE wicket_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT wicket_owner TO wicket_migrate;
-- after CREATE TABLE audit.event:
ALTER TABLE audit.event OWNER TO wicket_owner;
REVOKE ALL ON audit.event FROM PUBLIC;
GRANT SELECT ON audit.event TO wicket_app;
GRANT INSERT ON audit.event TO wicket_audit_writer;
REVOKE UPDATE, DELETE, TRUNCATE, TRIGGER ON audit.event FROM wicket_app;
```

Is this a missing Wave 1 workspace subtask? **Yes.** Roles, `pg_hba` for the bundled cluster, two URLs, and a migrate-vs-app smoke test belong in Wave 1 because every Wave 2 crate’s `sqlx::test` and every `query!` compile will need `DATABASE_URL`. If Wave 1 ships a single superuser URL, Wave 2 will write every test against superuser and invariant 3 will never be exercised.

---

## 5. Threat against the owner role — is grant-level append-only theater?

**Yes, against the threat the architecture actually named.**

`docs/02` §8: *“The realistic threat model for a shop floor system, which is mostly insider and accident rather than nation-state.”* ADR 0003: bundled PostgreSQL in the application data directory, installer initializes the cluster, user never learns it is there. The Windows/macOS account that runs Wicket **owns PGDATA** and can:

- `psql -U postgres` (local trust/`ident`/peer on a bundled cluster is the default people ship)
- `UPDATE audit.event SET old_row = '{}'` as superuser
- `ALTER TABLE audit.event DISABLE TRIGGER ALL`
- `SET session_replication_role = replica`
- `DROP TRIGGER`
- stop the postmaster and copy files
- restore a backup from last Tuesday and delete the file
- `pg_dump` / edit / restore

PostgreSQL wiki (Audit trigger, limitation that “seems to get overlooked”): *you cannot use in-database auditing to securely track the actions of a superuser, the role that owns the audited table, or the role that owns the audit table.*

Table-owner `BEFORE UPDATE OR DELETE` triggers **do** reject owner DML until the owner disables them. Event triggers **cannot** reject DML; they are DDL-only (`ddl_command_start/end`, `sql_drop`, `table_rewrite`). An event trigger can refuse `DROP TABLE audit.event` and `ALTER TABLE audit.event DISABLE TRIGGER`. Superuser can `ALTER EVENT TRIGGER … DISABLE`.

`session_replication_role` is superuser-context. Parameter ACLs (PG 15+) can stop `wicket_app`. They cannot stop the cluster owner.

### Hash chain / pgcrypto / “even the owner”

| Control | Stops `wicket_app` | Stops owner `psql` | Stops restore-from-backup | Stops “edit PGDATA on disk” |
|---|---|---|---|---|
| GRANT no UPDATE/DELETE | yes | no | no | no |
| BEFORE UPDATE/DELETE RAISE | yes | until DISABLE TRIGGER | no | no |
| Event trigger on DROP/ALTER | yes | until disabled | no | no |
| Hash chain in-row (SHA-256 of prev) | detects app-level rewrite | **detects** if you verify; owner can rewrite the chain | rewrite both table and chain | same |
| pgcrypto signature, key on the box | theater | theater | theater | theater |
| Signature with key **off box** / HSM / printed hash in the DHR | detects | detects | detects if the off-box copy exists | detects |
| WORM / object-lock export | n/a | n/a | depends | depends |

A hash chain is **tamper-evident, not tamper-proof**. On a self-hosted box it is worth doing if and only if something **off the box** (customer IQ printout, nightly signed export to a USB in the QA cage, optional cloud attest) verifies it. A chain whose verification key lives next to PGDATA is a slower `UPDATE`.

### What 21 CFR 11.10(e) actually requires

Regulation text (eCFR): *“Use of secure, computer-generated, time-stamped audit trails to independently record the date and time of operator entries and actions that create, modify, or delete electronic records. Record changes shall not obscure previously recorded information.”*

- **“Shall not obscure”** is about in-place overwrite of the **record** (the batch record still shows the old value in the trail). A versioned row + OLD/NEW in the trail satisfies this even if a DBA could theoretically UPDATE the trail. Predicate-rule reading (FDA 2003 Scope & Application) still requires that changes not obscure previous entries; the Agency also said it would exercise **enforcement discretion** on the computer-generated timestamped-trail specifics of 11.10(e) itself, while predicate rules stand.
- **“Secure”** is not defined as SHA-256. Vendor blogs (ISPE iSpeak 2026, Klyverity, Certivo) currently sell hash-chaining as “the technical standard.” That is market speech, not the regulation. Do not write it into ADR 0005 as if 11.10(e) named an algorithm.
- **Closed system** (11.10) is “procedures and controls,” technical **and** procedural. Industry-typical Part 11 package: application users cannot edit the trail; admin access is SOP-gated, named, and rare; backups are validated; time is server-side. Inspectors do ask “can anyone edit this table.” “The app role cannot” is a true sentence. “Nobody in the building can” is a false one on a bundled cluster.

**Honest position for a 30-person self-hosted shop:**

1. Grant-level append-only + trigger-block + `wicket_app` has no INSERT on audit = **necessary**, and it is what PLAN invariant 3 should mean.
2. It is **not** sufficient against the shop admin. Saying it is, in ADR 0005 or in `docs/06` later, is a 483 waiting for an investigator who knows `\du`.
3. Hash chain as a kernel feature is a **product decision**, not a regulatory must. It is a good decision if Wave 2 will export a customer-verifiable chain (IQ suite, `docs/02` §9). It is a bad decision if it is an in-table HMAC with the key in `PGDATA`.
4. The bundled-PG reality (slice 7) and this slice are the same threat. If opus decides “escape-hatch Postgres, customer’s DBA,” the owner-threat is the customer’s SOP problem. If opus decides “we bundle and we are the DBA,” Wicket owns the 11.10(e) story for the superuser path.

**PLAN amendment (proposed):** invariant 3 stays, but add: *the application role is not the table owner; a BEFORE UPDATE/DELETE trigger rejects mutation even for non-superuser owners; tamper-evidence against superuser is a signed export, not a grant.* Do not claim “the audit store is append-only” in `docs/02` §8 without the qualifier “for the application role.”

---

## 6. Test ergonomics

ADR 0005: *“Deleting a test record is not a thing you can casually do.”*

Wrong instinct: a `wicket_test` role with `DELETE` on `audit.event` in the same cluster. That role will leak into fixtures, CI, and eventually a “reset demo” button.

Right instincts, in order:

1. **Ephemeral database per test.** `sqlx::test` (0.7+) / testcontainers / a Wave 1 helper that `CREATE DATABASE … TEMPLATE wicket_template`. The test DB is dropped. No DELETE. This is the only strategy that also exercises deferred ledger constraints and concurrent tests.
2. **Transaction rollback.** A test that `BEGIN`s, writes, asserts, drops the `Tx` (rollback) never commits audit rows. Fast. **Does not** exercise `COMMIT`-time deferred constraints (ledger slice) or visibility across connections. Use for unit-ish crate tests; not for the property suite.
3. **Migrate-role reset of a dedicated `wicket_test` database**, never of a data dir that could be production. `DROP SCHEMA … CASCADE` as owner. Documented, gated on `current_database() IN ('wicket_test', …)` inside a SQL function owned by `wicket_migrate`. Still a loaded gun; prefer (1).

Do **not** `TRUNCATE audit.event` as `wicket_app`. Do **not** grant it. A test that needs a clean audit table is in the wrong database.

Fixture rows: insert **through `Tx::begin(Actor::test("…"))`** so the trail exists and looks like production. Golden-file tests that dump `audit.event` must tolerate timestamps (server clock) — compare everything except `at`, or freeze time only in the test actor’s reason field, never by stubbing `clock_timestamp` (you cannot without superuser).

Wave 1 must ship this helper (`wicket_db::test_db` or a `dev/test-db.sh`). PLAN §7 does not mention it. Slice 4 (stubs) will collide with this if `sqlx::test` needs a live Postgres before the installer exists — same gap as slice 7.

---

## 7. PLAN gaps (mechanical, not vibes)

These are missing lanes/subtasks, not style notes.

| Gap | Why it is load-bearing | Where it belongs |
|---|---|---|
| No roles, no GRANTs, no two URLs | Invariant 3 is untestable; migrations and app share a superuser | Wave 1 `workspace` (`dev/init-roles.sql`, compose, CI secrets) |
| No session-context helper | Every Wave 2 crate will `pool.begin()` and forget SET LOCAL | Wave 1 stub of `wicket-db`: `Tx::begin(Actor)` is **real**, not `todo!()` |
| No fail-closed trigger SQL | “Produced by the persistence layer” has no artifact | Wave 1 or serial start of `wicket-audit`: `migrations/0001_audit.sql` |
| No event trigger on `CREATE TABLE` | Modules not yet written can create unaudited tables | Same migration; this is the plugin-safety claim |
| No lint forbidding raw SQLx writes outside `wicket-db` | House style is otherwise unenforceable | Wave 1: `clippy.toml` `disallowed-macros` / `disallowed-methods` + CI `rg` for `sqlx::query`, `query!`, `QueryBuilder`, `raw_sql`, `copy_in_raw`, `set_config` |
| ADR 0005 grants app INSERT on audit | Directly contradicts “cannot write a false one” | ADR amendment (doc-adr or a DECISION) |
| `docs/02` §8 overclaims append-only | Inspector-facing sentence is false for bundled PG | architecture + ADR 0005 consequences |
| Hash-chain / signed-export undecided | Integrity vs owner is a product claim | opus DECISION (this slice + part11 slice) |
| `wicket-jobs` has no audit/identity edge | Background work must SET LOCAL a service principal | crate-graph slice; `Actor` in `wicket-core` + `Tx` in `wicket-db` may save the edge |
| SQLx version unpinned | 0.7 vs 0.8 vs 0.9 changes `Executor` and `SqlStr` | Wave 1 `Cargo.toml` workspace.dependencies |
| Non-audited table protocol unspecified | ADR allows it; without a catalog it becomes a per-write flag | `wicket-audit` SPEC |
| Test DB strategy unspecified | ADR admits the pain and then PLAN says nothing | Wave 1 `dev/` + `#[sqlx::test]` example in stub |
| Auto-audit of DDL (who ran a migration) | Event trigger on `ddl_command_end` writing to `audit.ddl` | `wicket-audit`; not invariant 3 but 11.10(k) adjacent |

**The missing mechanical enforcement lane, named:** Wave 1 ships (1) roles+grants, (2) `Tx::begin` helper, (3) audit trigger + event trigger SQL, (4) clippy/CI deny list. Wave 2 `wicket-audit` owns the schema and tests that a `query!` insert without `Tx::begin` **fails**, that `wicket_app` `UPDATE audit.event` **fails**, and that a forged `INSERT INTO audit.event` **fails**. That test list is the acceptance criterion PLAN §6 invariant 3 currently lacks.

Without those four, “produced by the persistence layer” is a comment.

---

## 8. EXECUTOR / SPLIT / TIER

| Work | Executor | Split | Audit tier |
|---|---|---|---|
| opus DECISION (trigger+GUC vs trait vs both; INSERT grant; hash chain vs signed export; time function) | **opus**, laned `decision-audit-persistence` | one decision file, amends ADR 0005 + PLAN §6 | n/a (decision) |
| Wave 1 roles, two URLs, clippy deny, `dev/init-roles.sql`, sqlx version pin | grok **or** cursor; not a blind race (it is config) | **split from** generic workspace skeleton: `workspace-roles-lint` as its own lane so the skeleton can merge first | **deep** (invariant-bearing) |
| `wicket-db` `Tx::begin` / `WriteContext` / pool `after_connect` UTC | Wave 2 crate lane | do not split the helper from the pool | **deep** |
| `wicket-audit` schema, triggers, event trigger, exempt catalog, tests | Wave 2 crate lane, **serial after** `wicket-db` stub is real enough to compile tests | tests **do** split: `audit-engine` vs `audit-pg-tests` if the PG suite > 10 min | **deep** |
| Hash chain / signed export (if DECISION says yes) | same `wicket-audit` lane or a follow-on; **not** a module | keep in kernel | **deep** |
| Proc-macro sugar | later, optional; never on the critical path | skip in this build unless DECISION demands it | standard |

Blind-race: not for the SQL trigger file (one correct shape, low implementation branching). Blind-race **would** be justified for a home-grown SQLx interceptor — which we should not build.

`wicket-audit` is integration-critical and previously-unspecified → PLAN already implies deep by putting it on the kernel path; this slice **agrees, and raises `wicket-db` to the same tier.**

---

## 9. Opus DECISION required?

**Yes.** This is a tough-decision trigger: two plausible persistence designs, a grant that contradicts the ADR’s own guarantee, and a compliance claim that is false against the owner threat. Free-pool executors must not pick this.

Questions for the then-unwritten audit-persistence decision (later promoted into `research/decisions/audit-persistence.md`) (suggested):

1. **Enforcement:** trigger+GUC fail-closed + sealed `Tx` + lint (**recommended**), trigger-only, or trait-only? Trait-only should be rejected; it is the “audit as a module” failure mode in kernel clothing.
2. **Audit INSERT grant:** revoke from `wicket_app`, `SECURITY DEFINER` only (**recommended**), or keep ADR 0005 as written and drop “cannot write a false one”?
3. **Time in the trigger:** `now()` / `statement_timestamp()` / `clock_timestamp()`?
4. **Tamper-evidence against owner:** grant+trigger is enough for v1 closed-system SOP (**recommended for this build**), in-table hash chain with off-box export in the IQ suite, or postpone to `docs/06` / Phase 6 validation-pack? Do **not** ship pgcrypto signatures with the key in PGDATA.
5. **Test reset:** ephemeral DB via `sqlx::test` as the only blessed path (**recommended**)?

Until that file exists, Wave 1 may still ship roles and a fail-closed trigger (those do not pre-empt 4). It must **not** grant `wicket_app` INSERT on `audit.event` “to match the ADR” — that cements the contradiction.

---

## 10. Direct answers to the two questions

**Q1. Does SQLx permit a clean interception point so writing a record produces its audit entry in the same transaction without module authors remembering?**

No. SQLx cannot do this. A macro, a sealed trait, or a repository on every write is what you would do *in application code*, and it is forgettable. The interception point that cannot be forgotten is a PostgreSQL AFTER ROW trigger. The application still must SET LOCAL actor/intent; making that fail-closed is how “remembering” becomes “the write does not happen.”

**Q2. Is grant-level append-only workable given migrations need DDL?**

Yes, if and only if migrate and app are different roles. PLAN never says that. It is a missing Wave 1 subtask. Grant-level append-only is then workable against `wicket_app` and is not workable against the bundled-cluster owner. Do not pretend otherwise; put the owner threat in the IQ/SOP story and optionally a verifiable export.

---

*End of slice 2. This file is the sole deliverable; no product files were edited.*
