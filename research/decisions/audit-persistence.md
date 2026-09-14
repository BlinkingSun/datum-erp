# DECISION — Audit persistence, tamper evidence, and what we may claim (D3 + D4)

**Decider:** opus (decision authority, task `erp`, lane `decision-audit-persistence`)
**Date:** 2026-09-11
**Status:** Decided. Binding on Wave 1 `workspace`, Wave 2 `wicket-db`, `wicket-audit`,
`wicket-identity`, `wicket-esign`, `wicket-numbering`, and on all customer-facing prose.
**Inputs:** `research/audits/slice-audit-persistence.md`, `research/audits/slice-part11.md`
§§4.6, 5.1–5.8, `research/audits/plan-audit.md` §0.3, §0.4, R3, R4, G10, G11, G18, D3, D4,
`research/audits/slice-property-tests.md` (one-group-per-transaction), ADR 0003, ADR 0005,
PLAN §6.
**Files amended by this decision:** `docs/adr/0005-compliance-in-kernel.md`,
`PLAN.md` §6 invariants 3–5. No code was written.

D3 and D4 are one subject and are decided together. How the trail is guaranteed and what
we may say about it are the same question asked twice: the claim is only as strong as the
weakest mechanism under it, and the mechanism is only worth building if the claim it
supports is one we are willing to have read back to us by an investigator.

---

## 0. The two things that were wrong, and what replaces them

**Wrong claim 1 — "audit records are produced by the persistence layer."** SQLx is not a
persistence layer in the sense that sentence needs. It has no unit of work, no entity
callbacks, no dyn-safe `Executor` to wrap, and no hook that sees OLD and NEW values.
Pool hooks are connection-lifetime, not statement-lifetime. A sealed `Tx` type plus a
clippy deny-list is house style: correct if remembered, which is precisely the property
ADR 0005 exists to reject.

**Replacement:** the guarantee is a PostgreSQL `AFTER ... FOR EACH ROW` trigger, attached
automatically by an event trigger at `CREATE TABLE`, writing through a `SECURITY DEFINER`
function. Rust supplies actor and business intent as transaction-local settings and fails
closed when they are absent. Rust does not produce the audit row and is not trusted to.

**Wrong claim 2 — "the audit table is append-only at the grant level, so an application
bug cannot violate it."** That much is true. What the ADR and the PLAN then let a reader
believe — that the trail cannot be altered — is false against the person who matters. On a
self-hosted install the shop administrator owns the data directory, is cluster superuser,
and can `UPDATE audit.event`, `ALTER TABLE ... DISABLE TRIGGER`,
`SET session_replication_role = replica`, `pg_dump | edit | pg_restore`, or stop the
postmaster and edit files. The PostgreSQL project says this in its own audit-trigger
documentation: in-database auditing cannot securely track a superuser, the owner of the
audited table, or the owner of the audit table.

**Replacement:** grants are retained and tightened — they are the correct and necessary
answer to application bugs and to application code, and that is a real threat, not a
strawman. Against the cluster owner the mechanism is **tamper evidence**: a
per-transaction hash chain whose head is anchored **off the box** by the customer. The
claim is scoped accordingly in §7, and the scoped claim is the only one the project makes
anywhere.

Both amendments make ADR 0005's *intent* true for the first time. Nothing in the intent
changes. Two sentences about mechanism were wrong and are now right.

---

## 1. The trigger design

Target: **PostgreSQL 15 or later** (parameter ACLs `GRANT SET ON PARAMETER`, `xid8` /
`pg_current_xact_id()`, deterministic `jsonb`). ADR 0003 does not pin a version; it
should, and that is flagged in §11, not decided here.

### 1.1 Roles

Five roles. Three of them cannot log in.

| Role | Login | Purpose | Notable privileges |
|---|---|---|---|
| `wicket_owner` | NOLOGIN | owns every table and every trigger function | no runtime use; `REASSIGN OWNED` target |
| `wicket_migrate` | LOGIN | runs migrations; member of `wicket_owner` | DDL, GRANT; **not** the app pool URL |
| `wicket_app` | LOGIN | the application pool | `SELECT, INSERT, UPDATE, DELETE` on business tables; **`SELECT` only** on `audit.*`; no `TRUNCATE`, no `TRIGGER`, no `SET session_replication_role` |
| `wicket_audit_row` | NOLOGIN | owns `audit.row_change()`; the only writer of row-change columns | column-restricted `INSERT` on `audit.event` |
| `wicket_audit_event` | NOLOGIN | owns `audit.log_event()`; writes kernel events (login, export, print, signature, security) | column-restricted `INSERT` on `audit.event`, **no** access to `op` / `old_row` / `new_row` / `table_name` |

`wicket_audit_row` and `wicket_audit_event` are deliberately **not** the table owner, so a
bug in a `SECURITY DEFINER` function is an insert bug and never a rewrite-history bug.
Two writer roles rather than one, because PostgreSQL column-level `INSERT` grants let us
make "the kernel event path physically cannot fabricate a row change" a privilege fact
rather than a code review.

```sql
CREATE ROLE wicket_owner       NOLOGIN;
CREATE ROLE wicket_audit_row   NOLOGIN;
CREATE ROLE wicket_audit_event NOLOGIN;
CREATE ROLE wicket_migrate     LOGIN;
CREATE ROLE wicket_app         LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT wicket_owner TO wicket_migrate;
REVOKE SET ON PARAMETER session_replication_role FROM wicket_app;  -- PG 15+
```

Two connection URLs. `WICKET_DATABASE_URL` (`wicket_app`) and
`WICKET_MIGRATE_DATABASE_URL` (`wicket_migrate`). Wave 1 ships both, plus
`dev/init-roles.sql`, or invariant 3 is untestable and every Wave 2 test will be written
against a superuser and prove nothing.

### 1.2 The table

```sql
CREATE SCHEMA audit AUTHORIZATION wicket_owner;

CREATE TABLE audit.event (
  event_id        uuid        NOT NULL DEFAULT gen_random_uuid(),
  -- time (§4)
  at              timestamptz NOT NULL,       -- now(): time of the atomic change
  stmt_at         timestamptz NOT NULL,       -- clock_timestamp(): intra-tx order
  xid             xid8        NOT NULL,       -- binds the row to its seal (§6)
  -- actor
  actor_id        uuid        NOT NULL REFERENCES identity.principal(id),
  actor_kind      text        NOT NULL CHECK (actor_kind IN ('user','service','migration')),
  actor_display   text        NOT NULL,       -- denormalised name AS OF the change
  acting_for_id   uuid            NULL REFERENCES identity.principal(id),
  -- provenance
  session_id      uuid            NULL,
  request_id      uuid            NULL,
  source_kind     text        NOT NULL CHECK (source_kind IN
                      ('ui','api','job','import','migration','maintenance','app_event')),
  source_device_id text           NULL,       -- §5; 21 CFR 11.10(h)
  source_ip       inet            NULL,
  client_app      text            NULL,
  -- business intent
  action          text        NOT NULL,       -- 'work_order.complete'
  reason          text            NULL,       -- required per catalogue; see §5
  doc_type        text            NULL,       -- 'work_order'
  doc_id          uuid            NULL,       -- the regulated record this belongs to
  esign_id        uuid            NULL,       -- the signature that authorised it
  -- the change itself (trigger-only columns)
  schema_name     name            NULL,
  table_name      name            NULL,
  op              text            NULL CHECK (op IN ('INSERT','UPDATE','DELETE','TRUNCATE')),
  row_key         jsonb           NULL,
  old_row         jsonb           NULL,
  new_row         jsonb           NULL,
  changed_columns text[]          NULL,
  CONSTRAINT event_shape CHECK (
    (source_kind =  'app_event' AND op IS NULL     AND table_name IS NULL)
 OR (source_kind <> 'app_event' AND op IS NOT NULL AND table_name IS NOT NULL)),
  PRIMARY KEY (at, event_id)
) PARTITION BY RANGE (at);
```

Monthly range partitions, created ahead by a maintenance job. Partitioning is in the
schema from migration 0001 because retrofitting a partition key onto the hottest table in
the system after eight years of data is the one audit change that *is* a rewrite
(`sweep-plan-part11.md` §5.1). Partitions are detached and archived, never dropped, and an
archived partition travels with the seals that cover it (§6).

There is no `serial` / `IDENTITY` on this table, deliberately. See §8: a sequence-assigned
id would put permanent unexplainable gaps in the audit trail itself.

### 1.3 Grants

```sql
REVOKE ALL ON audit.event FROM PUBLIC;
GRANT SELECT ON audit.event TO wicket_app;                  -- SELECT, and nothing else
GRANT INSERT ON audit.event TO wicket_audit_row;
GRANT INSERT (event_id, at, stmt_at, xid, actor_id, actor_kind, actor_display,
              acting_for_id, session_id, request_id, source_kind, source_device_id,
              source_ip, client_app, action, reason, doc_type, doc_id, esign_id)
      ON audit.event TO wicket_audit_event;                 -- no op/old_row/new_row/table
```

No role holds `UPDATE`, `DELETE`, or `TRUNCATE` on `audit.event`. Not the app role, not
the migrate role, not the writer roles. The owner can grant itself those, and a superuser
needs no grant; that is the trust boundary, stated in §7 rather than papered over.

### 1.4 The row-change trigger

One generic function for every audited table. Primary-key column names are passed as
trigger arguments by the attach function, so the trigger does no catalogue lookup per row.

```sql
CREATE FUNCTION audit.row_change() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
  ctx     audit.context := audit.require_context();   -- fails closed; see §2
  pk_cols text[]        := TG_ARGV[0]::text[];
  old_j   jsonb;
  new_j   jsonb;
  changed text[];
BEGIN
  old_j := CASE WHEN TG_OP IN ('UPDATE','DELETE') THEN audit.scrub(TG_RELID, to_jsonb(OLD)) END;
  new_j := CASE WHEN TG_OP IN ('INSERT','UPDATE') THEN audit.scrub(TG_RELID, to_jsonb(NEW)) END;

  IF TG_OP = 'UPDATE' THEN
    SELECT array_agg(k ORDER BY k) INTO changed
      FROM jsonb_object_keys(new_j) AS k
     WHERE new_j -> k IS DISTINCT FROM old_j -> k;
    IF changed IS NULL THEN RETURN NULL; END IF;          -- no-op UPDATE, no audit row
  END IF;

  IF ctx.reason IS NULL AND audit.reason_required(TG_RELID, TG_OP) THEN
    RAISE EXCEPTION 'wicket: % on %.% requires a reason for change',
      TG_OP, TG_TABLE_SCHEMA, TG_TABLE_NAME USING ERRCODE = '42501';
  END IF;

  INSERT INTO audit.event (
    at, stmt_at, xid,
    actor_id, actor_kind, actor_display, acting_for_id,
    session_id, request_id, source_kind, source_device_id, source_ip, client_app,
    action, reason, doc_type, doc_id, esign_id,
    schema_name, table_name, op, row_key, old_row, new_row, changed_columns)
  VALUES (
    now(), clock_timestamp(), pg_current_xact_id(),
    ctx.actor_id, ctx.actor_kind, ctx.actor_display, ctx.acting_for_id,
    ctx.session_id, ctx.request_id, ctx.source_kind, ctx.source_device_id,
    ctx.source_ip, ctx.client_app,
    ctx.action, ctx.reason, ctx.doc_type, ctx.doc_id, ctx.esign_id,
    TG_TABLE_SCHEMA, TG_TABLE_NAME, TG_OP,
    audit.pk_of(pk_cols, COALESCE(new_j, old_j)),
    old_j, new_j, changed);
  RETURN NULL;
END $$;
ALTER FUNCTION audit.row_change() OWNER TO wicket_audit_row;
```

`audit.scrub(regclass, jsonb)` removes columns listed in
`audit.redact(relid, column, reason, decided_by)` and substitutes `"[redacted]"`. This
exists because `to_jsonb(NEW)` on `identity.principal` would otherwise copy every password
hash into a table that is retained for eight years and readable by `wicket_app`. Large
`bytea` columns are redacted the same way; document payloads are stored by content hash
and the audit row carries the hash, not the blob.

`TRUNCATE` is covered by a statement-level trigger, because row triggers do not fire for
it:

```sql
CREATE FUNCTION audit.stmt_truncate() RETURNS trigger ...  -- writes op='TRUNCATE'
```

`wicket_app` is not granted `TRUNCATE` on anything, so in practice this trigger exists to
catch the owner and to make the trail complete rather than to permit the operation.

### 1.5 Attachment — why a module author gets audit for free

```sql
CREATE FUNCTION audit.attach(rel regclass) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE pk text[];
BEGIN
  SELECT array_agg(a.attname::text ORDER BY k.ord)
    INTO pk
    FROM pg_index i
    JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord) ON TRUE
    JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
   WHERE i.indrelid = rel AND i.indisprimary;

  IF pk IS NULL THEN
    RAISE EXCEPTION 'wicket: % has no primary key; an audited table must be addressable', rel;
  END IF;

  EXECUTE format(
    'CREATE TRIGGER zz_audit_row AFTER INSERT OR UPDATE OR DELETE ON %s
       FOR EACH ROW EXECUTE FUNCTION audit.row_change(%L)', rel, pk);
  EXECUTE format(
    'CREATE TRIGGER zz_audit_truncate AFTER TRUNCATE ON %s
       FOR EACH STATEMENT EXECUTE FUNCTION audit.stmt_truncate()', rel);
END $$;

CREATE FUNCTION audit.attach_new_tables() RETURNS event_trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE cmd record;
BEGIN
  FOR cmd IN SELECT * FROM pg_event_trigger_ddl_commands()
             WHERE object_type = 'table' AND schema_name <> 'audit'
  LOOP
    CONTINUE WHEN EXISTS (SELECT 1 FROM audit.exempt e WHERE e.relid = cmd.objid);
    PERFORM audit.attach(cmd.objid);
  END LOOP;
END $$;

CREATE EVENT TRIGGER audit_attach ON ddl_command_end
  WHEN TAG IN ('CREATE TABLE', 'CREATE TABLE AS', 'SELECT INTO')
  EXECUTE FUNCTION audit.attach_new_tables();
```

The trigger name is prefixed `zz_` so it fires after any business `AFTER` trigger on the
same table — PostgreSQL fires per-event triggers in name order — which means the audit row
reflects the row as other triggers left it.

`audit.exempt(relid, reason, decided_by, decided_at)` is the implementation of ADR 0005's
"a table may be declared non-audited, and that declaration is itself a reviewed, recorded
decision." Rows are written by a migration, in a transaction, as `wicket_migrate`, and
`audit.exempt` is itself audited — so exempting a table is a permanent, attributable
record. Exemptions in this build: `numbering.counter` (§8), `audit.tx_seal`, and
`audit.anchor` (§6), each with its reason in the row.

A second event trigger defends the machinery:

```sql
CREATE EVENT TRIGGER audit_protect ON ddl_command_end
  WHEN TAG IN ('ALTER TABLE','DROP TABLE','ALTER TRIGGER','DROP TRIGGER','ALTER EVENT TRIGGER')
  EXECUTE FUNCTION audit.protect();  -- RAISE on DISABLE TRIGGER / drop of audit objects
```

Event triggers cannot reject DML, only DDL, so this is not a substitute for §6. It raises
the cost of "quietly turn it off" from one statement to a statement that fails and is
itself logged. A superuser can `ALTER EVENT TRIGGER audit_protect DISABLE`. Stated, not
hidden.

### 1.6 Why not the Rust-side alternatives

Kept on the record because both will be proposed again.

*An `Executor` wrapper that injects an audit insert* sees SQL text, not row images; cannot
produce prior values without a `SELECT` it does not have; misses `COPY`, `raw_sql`,
`QueryBuilder`, and every caller still holding a `PgPool`; is not dyn-safe; and would have
to parse SQL correctly for CTEs, `INSERT ... SELECT`, `UPDATE ... FROM`, and `MERGE`. It is
a leaky decorator, not a persistence layer.

*A derive macro on entities* only fires for writes that go through generated
`insert`/`update`. Bulk statements, ledger postings, numbering, and jobs will not. It also
re-creates the false-audit problem, since the macro writes whatever the attribute says.

Both are acceptable as **ergonomics** over the sealed `Tx` of §2. Neither is the
guarantee, and neither may be described as one.

---

## 2. How the actor reaches the trigger, and the pool discipline

This is the bug the audit slice warned about, and it is worth being blunt: a session-level
`SET wicket.actor_id = '...'` survives `COMMIT`, SQLx does not reset session state when a
connection returns to the pool, and the next HTTP request to acquire that connection would
be attributed to the previous operator. In a regulated system that is not a bug, it is a
falsified record. Three independent mechanisms prevent it, and a fourth detects it.

### 2.1 Transaction-local only, in one statement, from Rust

```rust
// wicket-db — the only legal write surface in the workspace
pub struct WritePool(PgPool);          // no Deref, no as_pool(), no into_inner()
pub struct Tx<'c> { inner: Transaction<'c, Postgres> }

impl Tx<'_> {
    pub async fn begin(pool: &WritePool, ctx: &WriteContext) -> Result<Tx<'_>> {
        let mut inner = pool.0.begin().await?;                     // BEGIN
        sqlx::query!(
            r#"SELECT pg_catalog.set_config('wicket.actor_id',      $1, true),
                      pg_catalog.set_config('wicket.actor_kind',    $2, true),
                      pg_catalog.set_config('wicket.actor_display', $3, true),
                      pg_catalog.set_config('wicket.acting_for',    $4, true),
                      pg_catalog.set_config('wicket.session_id',    $5, true),
                      pg_catalog.set_config('wicket.request_id',    $6, true),
                      pg_catalog.set_config('wicket.source_kind',   $7, true),
                      pg_catalog.set_config('wicket.source_device', $8, true),
                      pg_catalog.set_config('wicket.source_ip',     $9, true),
                      pg_catalog.set_config('wicket.client_app',   $10, true),
                      pg_catalog.set_config('wicket.action',       $11, true),
                      pg_catalog.set_config('wicket.reason',       $12, true),
                      pg_catalog.set_config('wicket.doc_type',     $13, true),
                      pg_catalog.set_config('wicket.doc_id',       $14, true),
                      pg_catalog.set_config('wicket.esign_id',     $15, true),
                      pg_catalog.set_config('wicket.txid',
                          pg_catalog.pg_current_xact_id()::text,      true)"#,
            /* ... */
        ).execute(&mut *inner).await?;
        Ok(Tx { inner })
    }
}
```

Five details that are decisions, not style:

1. **`is_local := true`, always.** `set_config(name, value, true)` has `SET LOCAL`
   semantics: PostgreSQL itself reverts the value at `COMMIT` or `ROLLBACK`. The value
   therefore cannot outlive the transaction and cannot still be set when the connection
   returns to the pool. This is the guarantee; everything else in this section is defence
   in depth. Session-level `SET` of any `wicket.*` name is forbidden anywhere in the
   workspace and is on the CI deny-list.
2. **`set_config`, not `SET LOCAL`.** `SET LOCAL name = $1` is a syntax error — `SET` does
   not take bind parameters. People will try it, discover it fails, and reach for string
   interpolation. `set_config` binds.
3. **One statement, not sixteen.** One round trip, and — more importantly — it is
   impossible to have set half the context.
4. **Not wrapped in a PL/pgSQL helper.** A function declared with a per-function `SET`
   clause (for example `SET search_path = ...`) has its `SET LOCAL` effects **undone at
   function exit**. A context-setting helper is exactly the function that most wants a
   pinned `search_path`, so the two requirements are in direct conflict. Issuing the
   statement from Rust avoids the trap entirely.
5. **`wicket.txid` is captured.** See §2.3.

`WriteContext` cannot be constructed from strings by module code. It is built from an
`&Authenticated` produced by `wicket-identity`'s session verification, or from
`Actor::service(ServicePrincipal::Jobs)` and friends, whose constructors are
crate-private. A module cannot name an actor it did not authenticate.

Read-only work uses `ReadPool` and never calls `set_config` — note that
`pg_current_xact_id()` forces assignment of a real transaction id, which we do not want on
read paths.

### 2.2 Pool discipline

```rust
PgPoolOptions::new()
    .after_connect(|c, _| Box::pin(async move {
        sqlx::raw_sql("SET timezone = 'UTC'; \
                       SET application_name = 'wicket'; \
                       SET idle_in_transaction_session_timeout = '15s'")
            .execute(c).await?;
        Ok(())
    }))
    .after_release(|c, _| Box::pin(async move {
        // Belt, not the guarantee. RESET ALL also clears placeholder GUCs and,
        // unlike DISCARD ALL, keeps the prepared-statement cache.
        match sqlx::raw_sql("RESET ALL").execute(c).await {
            Ok(_)  => Ok(true),    // reuse
            Err(_) => Ok(false),   // discard the connection rather than reuse it dirty
        }
    }))
```

`after_connect` sets UTC and nothing actor-shaped — connection-lifetime state must never
carry identity. `after_release` runs `RESET ALL` and, if that fails for any reason, returns
`Ok(false)` so SQLx closes the connection instead of handing it to the next request.
`DISCARD ALL` is not used: it cannot run inside a transaction and it flushes the
prepared-statement cache SQLx depends on.

External connection pooling, if a customer ever adds it, is restricted to **session or
transaction pooling**. Statement-level pooling breaks `SET LOCAL` and is not supported.
Written down now because the bundled deployment has no pooler and someone will add one
later without asking.

### 2.3 The transaction-identity check — why a leak is detected, not merely unlikely

```sql
CREATE FUNCTION audit.require_context() RETURNS audit.context
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE c audit.context;
BEGIN
  c.actor_id := nullif(current_setting('wicket.actor_id', true), '')::uuid;
  -- ... remaining fields, each via current_setting(name, true) ...

  IF c.actor_id IS NULL THEN
    RAISE EXCEPTION 'wicket: refused write with no attributable actor'
      USING ERRCODE = '42501',
            HINT = 'begin the transaction through wicket_db::Tx::begin';
  END IF;

  IF nullif(current_setting('wicket.txid', true), '')::xid8
       IS DISTINCT FROM pg_current_xact_id() THEN
    RAISE EXCEPTION 'wicket: refused write with context from another transaction'
      USING ERRCODE = '42501';
  END IF;

  IF c.action IS NULL OR c.source_kind IS NULL THEN
    RAISE EXCEPTION 'wicket: refused write with no declared action' USING ERRCODE = '42501';
  END IF;
  RETURN c;
END $$;
```

The `wicket.txid` comparison is the mechanical answer to pool leakage. Context is valid only
for the transaction that set it. If a session-level `SET` ever leaked into a pooled
connection — through a future code path, a `psql` session, a pooler misconfiguration, or a
mistake nobody has made yet — the leaked value belongs to a transaction id that is no
longer current, and the next write **fails** rather than being attributed to the wrong
operator. A misattributed audit record is the one failure mode worse than a refused write,
so it is the one we make impossible rather than unlikely.

### 2.4 Absent context: fail closed, twice

There is no "unknown" actor and no `NULL` actor. `audit.event.actor_id` is `NOT NULL` with
a foreign key to `identity.principal`, so an unattributable audit row cannot be
represented in the schema, let alone written.

- **Layer one, refuse early.** A request with no authenticated session never reaches a
  write. `WriteContext` cannot be built without an `Authenticated`, so the handler returns
  401 before opening a transaction. This is the path users experience.
- **Layer two, refuse late.** Any write that reaches PostgreSQL without transaction-local
  context — a forgotten `Tx::begin`, a `psql` session, a `COPY`, a maintenance script, a
  module using `sqlx::query!` against a raw pool it should not have — raises `42501` in the
  `AFTER` trigger, which aborts the whole transaction. The business write does not happen.
  There is no partial state, because the audit insert and the business write are the same
  transaction by construction.
- **The refusal is itself recorded.** `wicket-db` maps `42501` from
  `audit.require_context()` to an internal error and, on a **separate connection and
  separate transaction** (the original is doomed), calls `audit.log_event()` to record a
  `security.unattributable_write` event with the request id, the source, and the SQLSTATE.
  A fail-closed abort that leaves no trace is an operational mystery; one that leaves a
  security event is a bug report.

Background jobs are not an exception. `wicket-jobs` opens transactions through
`Tx::begin(WriteContext::service(ServicePrincipal::Jobs, action))`, and a job that forgets
dies fail-closed on its first write. This requires `Actor`/`WriteContext` in `wicket-core`
and `Tx` in `wicket-db`, which is how `wicket-jobs` gets attribution without an edge to
`wicket-identity` or `wicket-audit` (the crate-graph gap flagged in plan-audit R5).

### 2.5 The residual: in-process code can name another real actor

`wicket_app` can call `set_config('wicket.actor_id', '<some other real user>', true)`. The
trigger trusts the setting, and the foreign key only proves the principal exists.
Mitigations are the sealed `WriteContext` and a CI deny-list (`clippy.toml`
`disallowed-macros` / `disallowed-methods`, plus an `rg` gate) forbidding `sqlx::query*`,
`QueryBuilder`, `raw_sql`, `copy_in_raw`, `set_config`, and `current_setting` outside
`wicket-db` and `wicket-audit`. Those are process controls, and process controls are exactly
what ADR 0005 rejects as a guarantee.

So state it plainly: **in-process application code is inside the trust boundary.** The
mechanism guarantees that a write cannot escape the trail and that the trail cannot be
edited by the application; it does not defend against hostile code compiled into the
application binary. That is a software-integrity problem, answered by the validated build,
the hashed module manifest, and — when out-of-process plugins arrive — by not giving them a
connection. §7 does not claim otherwise.

---

## 3. Does the application role keep `INSERT` on `audit.event`?

**No. Revoked. `wicket_app` holds `SELECT` and nothing else.** This overturns the ADR 0005
sentence "the application role has insert and select."

The reasoning is short. "A module author cannot write a false audit row" is either a
privilege or a wish. If `wicket_app` holds `INSERT`, any module, any handler, and any future
contributor can write an audit row describing a change that never happened, or describing
it as someone else, with a timestamp of their choosing. No lint prevents it, because the
statement is indistinguishable from a legitimate one. With `INSERT` revoked, the attempt
returns `42501: permission denied for table event`, which is a fact about the database and
not about anyone's diligence.

What it costs, honestly:

1. **A blessed function surface is now required.** Not every audit-worthy event is a row
   change: successful and failed logins, permission denials, exports, prints, signature
   executions, clock changes, and the unattributable-write refusal of §2.4 are all events
   with no OLD and NEW. `audit.log_event(kind, action, reason, doc_type, doc_id, esign_id,
   detail)` is `SECURITY DEFINER`, owned by `wicket_audit_event`, `EXECUTE` granted only to
   `wicket_app`, `REVOKE ... FROM PUBLIC`, `search_path` pinned, no dynamic SQL. It stamps
   `at` / `stmt_at` / `xid` from the server and actor from the transaction-local context —
   the caller supplies intent and cannot supply identity or time. `kind` is constrained to
   a kernel enum and the free-text action is namespaced by it. Because `wicket_audit_event`
   holds only column-level `INSERT`, this path **physically cannot** write `op`,
   `table_name`, `old_row`, or `new_row`: a forged row change is a privilege error, and the
   `event_shape` CHECK rejects the reverse confusion too.
2. **`SECURITY DEFINER` is a privilege-escalation surface and must be reviewed as one.**
   Pinned `search_path`, `REVOKE EXECUTE ON ALL FUNCTIONS IN SCHEMA audit FROM PUBLIC`,
   explicit grants, and no `EXECUTE format()` on caller-supplied text except in
   `audit.attach`, which runs as `wicket_owner` only during DDL. The definer roles are not
   the table owner and hold no `UPDATE`/`DELETE`, so the blast radius of a bug in any of
   these functions is "wrote an audit row it should not have," never "erased one."
3. **Test and fixture ergonomics.** Fixtures cannot bulk-insert audit rows to set up a
   scenario. They must write through `Tx::begin(Actor::test(...))`, which is better —
   fixtures then exercise the real trail — but it is more typing. Restore and data import
   run as `wicket_migrate`, not as the app.
4. **Diagnostics.** A developer who fumbles the setup gets `permission denied` rather than
   a silent no-op, which is the correct trade and needs a good error message in `wicket-db`.

Not a cost: performance. The trigger insert is in the same transaction and the definer
switch is negligible at shop volume.

---

## 4. The time source

**Decided: `at = now()` and `stmt_at = clock_timestamp()`, both `timestamptz`, both taken
inside the trigger. No timestamp is ever a bound parameter.**

`now()` (= `transaction_timestamp()`) is constant for a transaction. Since the ledger
decision makes one posting group one transaction, a work-order completion that touches
twelve rows produces twelve audit rows with **one identical time of record** — which is the
truth an inspector needs: those twelve changes were one atomic act, not twelve events at
twelve microsecond-separated times. `clock_timestamp()` alone would manufacture a false
sequence of distinct events out of a single indivisible one.

`stmt_at` from `clock_timestamp()` is carried as well, for two reasons. It gives a
deterministic intra-transaction order for reconstructing what happened in what order inside
one change, which matters when a state machine and a ledger posting both touch a document
in one transaction. And it makes the decision robust: if the one-group-per-transaction rule
is ever relaxed, `now()` would collapse two business events onto one stamp, and `stmt_at`
plus `xid` still distinguishes and orders them. The time decision therefore does not depend
on the ledger decision holding — it is correct either way, which is the property we want
from a column we can never retroactively populate.

`xid` (`pg_current_xact_id()`) is stored so every row of one atomic change is provably one
change, and so rows join to their seal (§6). Ordering across transactions on one connection
is monotonic non-decreasing in `at`; across connections, commit order is what the seal
chain records, and `at` is not a total order. No test may assert otherwise.

`at` is the partition key. Storage is UTC, `timestamptz` throughout. The server's
`TimeZone` is recorded once per transaction on the seal row, because the 2025 Annex 11
draft asks for timezone explicitly and a bare UTC instant loses what the operator saw on
the clock on the wall.

**And the honest part.** Server time is not trusted time. The bundled cluster reads the host
OS clock; the shop administrator is the host administrator; a clock set backwards, a
restored VM snapshot, or a `date` command produces audit rows with whatever the box
believed. A hash chain does not fix this — a backdated row hashes perfectly. 21 CFR
11.10(e) requires "time-stamped," not "traceable to UTC via authenticated NTP," so this is
compliant; it is simply not what "server-authoritative time" sounds like. Therefore
`docs/06` must state that the time source is the host clock, that clock administration is a
customer SOP, and that Wicket records clock changes it can observe (`audit.log_event` on
detected backward jumps between statements). "Server-side time" is never marketed as a
substitute for trusted time. That is a `docs/06` obligation flagged in §11, not a file this
decision edits.

---

## 5. The audit row contents, the reason field, the device, and the export shape

The column list is §1.2. The decisions inside it:

**`reason` is a real column, required by catalogue, enforced in the trigger.**
`audit.reason_required(relid, op)` reads `audit.reason_policy(relid, ops text[])`, written
by migrations. Default policy: **required for `UPDATE` and `DELETE` on every table in the
regulated set; optional for `INSERT`.** EU Annex 11 §9 asks for the reason where
applicable, and "why did you change this released router" is the question an inspector
actually asks; "why did you create it" is answered by the document itself. A missing
required reason aborts the transaction, exactly as a missing actor does. If the reason were
optional in the schema and modules did not pass it, it could never be recovered — which is
why it is enforced by the database rather than requested in a code review.

**`actor_display` is denormalised on purpose.** People marry, change names, and leave. An
eight-year-old audit row must be readable without joining a mutable identity table, and
must show the name as it was at the time of the change. The foreign key on `actor_id` gives
integrity; the display string gives readability.

**`source_device_id` belongs in the row: yes.** Nullable, populated from context, with a
policy bit that can make it required per route — a shop-floor scan or a gage reading is
exactly the "as appropriate" case in 21 CFR 11.10(h). The device-check *feature* is a later
module; the *column* is now, because history never gains a device identity retroactively.
The same argument covers `source_ip`, `session_id`, `request_id`, `acting_for_id`, and
`esign_id`: each is a nullable column that costs nothing today and is unrecoverable if
omitted.

**`doc_type` / `doc_id` are the answer to acceptance criterion (g).** Without a document
correlation on the row, "every change to this work order" is a join across every table a
work order touches, invented eight years later by someone reading a schema they did not
write. With it, it is one indexed predicate.

**Export shape — 21 CFR 11.10(b), complete and accurate copies in both human-readable and
electronic form.** One command produces both forms from one query and one snapshot:

```
wicket audit export --doc work_order:WO-2026-0417 --from 2026-01-01 --to 2034-01-01
```

emits a single directory or zip bundle:

| Member | Form | Purpose |
|---|---|---|
| `manifest.json` | electronic | `export_id`, `export_schema_version`, Wicket version, PostgreSQL version, query predicate, row count, seal range, per-file SHA-256, generated-at, exporting actor |
| `events.ndjson` | electronic | one JSON object per audit event, all columns, including `old_row` / `new_row` |
| `events.csv` | electronic | flat columns for inspectors who use a spreadsheet; `old_row` / `new_row` as canonical JSON text |
| `seals.ndjson` | electronic | every `audit.tx_seal` covering the range, plus the anchors that cover those seals |
| `dictionary.md` | human-readable | column meanings, enum values, and the canonicalisation rule, so the bundle is self-describing without Wicket |
| `report.pdf` | human-readable | PDF/A, one section per event: local and UTC time, actor display and id, action, reason, document, device, and a before/after table of changed columns only; signature manifestations per 11.50(b) where `esign_id` is set; a verification page stating the seal range, head hash, anchor status, and the exact command to re-verify |

Both forms carry the same `export_id` and the same file digests, so the paper copy and the
electronic copy are provably the same set of records — which is what "complete and accurate
copies" means when an investigator holds one and audits the other. The bundle is readable
with no Wicket installation and no database: this is the Annex 11 §17 answer to "still
accessible and readable after equipment or program changes," and it is why the export
format is versioned and frozen rather than being a `pg_dump`. A `pg_dump` is neither
human-readable nor readable after a breaking schema change, and is not an acceptable answer
to 11.10(b).

Retention: partitions are never dropped. Archival detaches a monthly partition and stores
it with the seals and anchors covering it; the archive is itself an export bundle, so a
restored archive can be verified off-box by the same tool. `wicket-audit` owns the exporter;
`report.pdf` is rendered by the print primitive, which is why the missing print crate
(plan-audit G8) blocks this deliverable and is flagged in §11.

---

## 6. Tamper evidence — the hash chain is in v1

**Decided: yes, in v1.** Not because 21 CFR 11.10(e) requires it — it does not. "Record
changes shall not obscure previously recorded information" is satisfied by insert-only rows
carrying old and new values, and the *digital signature* of 11.3 is invoked by 11.30 for
open systems, not by 11.10(e). The vendor blogs selling hash-chaining as "the technical
standard" are selling, not citing.

It is in v1 for three reasons that are ours rather than the regulator's. First, the schema
cannot be retrofitted honestly: adding a chain in v2 means backfilling hashes over history
that was, by construction, unverifiable while it was being written, and the chain then
attests only that nobody tampered *after* we started looking. Second, adding columns to
`audit.event` later is a migration across every Wave 2 worktree that already compiled
against the row type, which is the integration contract. Third, without it the project has
no honest answer to the one threat its own architecture names — the insider — and would
have to delete the integrity claim entirely.

### 6.1 The chain: one link per transaction, sealed at commit

Per-row chaining would serialise every insert in the system through one lock.
Per-transaction chaining takes the lock once, at the end, and matches the granularity that
means something: one atomic business change.

```sql
CREATE TABLE audit.tx_seal (
  seq         bigint      PRIMARY KEY,
  xid         xid8        NOT NULL UNIQUE,
  sealed_at   timestamptz NOT NULL,       -- clock_timestamp() at commit
  tz          text        NOT NULL,       -- current_setting('TimeZone')
  row_count   integer     NOT NULL,
  rows_digest bytea       NOT NULL,       -- sha256 over the tx's canonical rows
  prev_hash   bytea       NOT NULL UNIQUE,
  hash        bytea       NOT NULL UNIQUE,
  chain_algo  text        NOT NULL        -- 'wicket-audit-1'
);
```

```
canon(row)  = 'v1' || 0x1F || event_id || 0x1F || to_char(at, 'YYYY-MM-DD"T"HH24:MI:SS.USOF')
              || 0x1F || ... every column, in a frozen order ...
              || 0x1F || audit.canon_jsonb(old_row) || 0x1F || audit.canon_jsonb(new_row)

rows_digest = sha256( concat of canon(row) || 0x1E over the transaction's audit rows,
                      ordered by (stmt_at, table_name, op, row_key::text, event_id) )

hash        = sha256( prev_hash || seq::text || xid::text || to_char(sealed_at, ...)
                      || rows_digest || row_count::text || chain_algo )
```

`audit.canon_jsonb` is ours, not PostgreSQL's `jsonb::text`: keys sorted lexicographically,
no insignificant whitespace, numbers rendered as their text form, recursive. It is frozen
and versioned by `chain_algo`, so a future `wicket-audit-2` verifies new ranges by new rules
and old ranges by old ones. Betting that a PostgreSQL major version's `jsonb` text output
is byte-stable for eight years is not a bet we are making.

Sealing runs in a **deferred constraint trigger**, which fires at `COMMIT` after all
statements — so every audit row of the transaction already exists when the seal is
computed:

```sql
CREATE CONSTRAINT TRIGGER zz_audit_seal
  AFTER INSERT ON audit.event
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION audit.seal_tx();
```

`audit.seal_tx()` is idempotent per transaction — the first firing writes the seal, later
firings see a seal for `pg_current_xact_id()` and return. It takes
`pg_advisory_xact_lock('audit.chain')` before reading the current head. That lock is
**transaction-scoped**, so it is held until the sealing transaction commits or aborts, which
is what makes the chain gap-free and fork-free: a competing transaction cannot read the head
until the previous one has fully ended, so it chains onto the new head, or onto the old one
if the previous transaction rolled back. `seq` is `prev.seq + 1`, read inside the lock —
never `nextval()`, which would leave permanent gaps on rollback (§8). `UNIQUE` on `seq`,
`prev_hash`, and `hash` turns any residual race — including a `REPEATABLE READ` or
`SERIALIZABLE` transaction whose snapshot hides a concurrent seal — into a loud constraint
violation rather than a silent fork. Write transactions run `READ COMMITTED` by default;
the rare serialisation failure is retried by `wicket-db`.

`audit.tx_seal` and `audit.anchor` are in `audit.exempt` — auditing the audit chain is a
recursion with no evidentiary value, and their integrity is the chain itself.

### 6.2 Verification, and where it happens off the box

```sql
audit.verify(from_seq bigint, to_seq bigint)
  RETURNS TABLE(seq bigint, ok boolean, detail text)
```

recomputes every seal in the range from the rows and the previous hash. This is useful and
it is **not evidence**: a superuser who edited a row and re-sealed the chain will make this
function return "ok" for everything. A chain verified only by the box that could have forged
it proves nothing. So:

**Anchors.** A nightly job writes the chain head as a short, human-transcribable record:

```
wicket audit anchor
  seq        1284013
  head       9f2c 41a7 8b03 5de6 1c88 0f45 a2b9 7e10 ...
  sealed_at  2026-09-11T03:00:07Z
  rows       4118902
  version    wicket 1.0.3 / wicket-audit-1
```

**Where it goes off the box** — the customer picks at least one at install time, and the IQ
suite refuses to pass with none configured:

1. Printed and pasted into the quality logbook, initialled by the quality manager. A
   30-person shop already does exactly this for equipment logs, so it survives contact with
   reality.
2. Emailed to the QA mailbox, which on a LAN install is usually a hosted mailbox outside
   the shop's control — a copy the cluster owner cannot silently rewrite.
3. Written to WORM or object-lock storage, or to a USB key kept in the QA cabinet, for shops
   with that habit.

**Who runs the verification, and when.** The customer's quality manager, as the periodic
audit-trail review that Annex 11 §9 already requires them to perform. The mechanical step
is: run `wicket audit export`, carry the bundle to a **different computer**, and run
`wicket-verify` — a small standalone binary that needs no database and no Wicket install. It
recomputes the chain from the bundle's rows and compares the head against the anchors
recorded off the box at the time. A head that matches a two-year-old anchor written in the
QA logbook is evidence that the covered history has not been rewritten since, and that
evidence does not depend on trusting the server, the cluster owner, or us. Wicket ships the
procedure as a one-page SOP in the validation pack and a named test in the IQ suite, and the
application health page warns when the last confirmed off-box anchor is older than the
configured interval — default seven days — because an anchor procedure nobody performs is
the same as no anchor.

**What is explicitly not done:** `pgcrypto` signatures with the key stored in `PGDATA`. A
signature whose key sits next to the data it signs is a slower `UPDATE` with better
marketing. If a customer wants signed anchors, the key is theirs and lives off the box.

### 6.3 What the chain actually buys

| Control | Stops `wicket_app` | Stops owner at `psql` | Stops a doctored restore | Stops editing files in `PGDATA` |
|---|---|---|---|---|
| No `INSERT` / `UPDATE` / `DELETE` grant | yes | no | no | no |
| `BEFORE UPDATE/DELETE` raise + `audit_protect` | yes | until disabled | no | no |
| Hash chain, verified on the same box | yes | no (re-seal) | no | no |
| Hash chain + **off-box anchor** | yes | **detected** | **detected** | **detected** |

Tamper-**evident**, not tamper-proof, and evident only for periods an off-box anchor covers.
That distinction is the whole of §7.

---

## 7. THE HONEST SENTENCE

This is the wording the project uses — in `docs/01`, `docs/02`, `docs/06`, the sales site,
and the validation pack — and it is the wording a customer can repeat to an investigator.

> **Every change to a regulated record in Wicket is written to the audit trail by the
> database itself, inside the same transaction as the change, with the operator's identity,
> the server time, the prior and new values, and the reason where one is required; a write
> that cannot be attributed to an authenticated operator is refused rather than recorded as
> unknown. The application — including any module, and including a defective one — can read
> the audit trail but cannot insert, alter, or delete an entry: that is enforced by database
> privileges, not by application code. Wicket does not claim the trail cannot be altered by
> someone with administrative control of the database server itself; instead, each
> transaction is sealed into a hash chain whose head is published off the server on a
> schedule you control, so that any later alteration of stored history is detectable by
> verifying an exported copy against those off-server records on a separate machine — which
> is the check your periodic audit-trail review performs.**

Three sentences, because a true statement of this needs three: what is guaranteed, who
cannot break it, and where the boundary is and what covers it.

What it does **not** say, deliberately: that the trail is immutable; that it cannot be
altered; that nobody can edit it; that time is traceable to an authenticated source; that
the chain is required by 21 CFR Part 11. Each of those would be false or unprovable, and an
investigator who runs `\du` or asks who the superuser is finds out in one minute. Being the
vendor whose compliance claim survives that minute is worth more than the stronger sentence.

Anyone writing customer-facing prose quotes this paragraph or a strictly weaker statement of
it. "Compliance as architecture" stays as positioning; "the audit trail cannot be altered"
does not appear anywhere. `docs/02-architecture.md:251` currently reads that the audit store
is append-only at the database level and must become "append-only to the application; see
the audit trail statement" — flagged in §11, as that file is outside this decision's edit
permission.

---

## 8. Gap-free numbering for regulated document types

**Decided: a transactional counter row. `nextval()` and `GENERATED AS IDENTITY` are
forbidden for any number a customer or an inspector will read.**

A PostgreSQL sequence is deliberately non-transactional so that concurrent sessions never
block on it. The cost is that a rolled-back or failed transaction consumes a value
permanently, and the gap is unexplainable after the fact: "why is there no work order
WO-2026-0416" has no good answer in an inspection, and the honest answer — the database
allocated it to a transaction that failed — is one the investigator has to take on faith
from the vendor whose numbering they are auditing.

```sql
CREATE TABLE numbering.counter (
  doc_type   text   NOT NULL,
  period_key text   NOT NULL DEFAULT '',   -- '', '2026', '2026-09', per format
  next_value bigint NOT NULL,
  format     text   NOT NULL,              -- 'WO-{yyyy}-{0000}'
  PRIMARY KEY (doc_type, period_key)
);

-- allocation, in the caller's transaction:
UPDATE numbering.counter
   SET next_value = next_value + 1
 WHERE doc_type = $1 AND period_key = $2
RETURNING next_value - 1 AS allocated;
```

The row lock serialises allocation per `(doc_type, period_key)` and is released at commit;
if the transaction rolls back, the increment rolls back with it and the number is not
consumed. Rules that go with it:

- **Allocate late.** The number is taken as close to commit as possible and never held
  across user interaction or a network call. Reserving a number when a form opens
  re-creates the gap as soon as the user closes the tab.
- **Never reuse a committed number, never delete a numbered document.** Cancellation is a
  status (`Void`) with a reason and a full audit trail. A visible voided document is an
  answer; a missing number is a question.
- **Sequences remain fine for surrogate keys** nobody reads, and for non-regulated types.
  The ban is on human-visible regulated numbers.
- `numbering.counter` is in `audit.exempt` — every allocation would otherwise write an audit
  row whose entire evidentiary content is "a counter went up," while the allocation is
  already evidenced by the audited document that carries the number. The exemption row
  states exactly that reason.
- **Acceptance tests:** allocate a number, abort the transaction, allocate again, assert the
  same value is returned; and allocate concurrently from N transactions, assert the committed
  set is exactly contiguous.

Throughput is one allocation at a time per document type per transaction. At a 30-person
shop that is not a constraint, and where it ever becomes one, the answer is a coarser period
key, not a sequence.

`wicket-numbering` currently has no stated algorithm in PLAN (plan-audit G16). This section is
the algorithm. It also wants a PLAN §6 invariant of its own, whose exact text is in §11 —
this decision's edit permission covers invariants 3 through 5 only, so the sentence is
proposed rather than written.

---

## 9. Electronic signature under 21 CFR 11.200

**Confirmed, not overturned: every signing requires all identification components. Wicket
does not implement the 11.200(a)(1)(i) continuous-session relaxation in v1.**

11.200(a)(1) requires at least two distinct identification components. Clause (i) permits
that, within a single continuous period of controlled system access, only the first signing
uses all components and subsequent signings may use one — but only a component "executable
by, and designed to be used only by, the individual." Clause (ii) requires all components
for signings outside such a period.

Reasons to take the conservative default:

1. **We cannot honestly assert clause (i)'s precondition on a shop floor.** The beachhead has
   shared tablets and kiosks at work centres. A session cookie on a shared device is not a
   component "designed to be used only by the individual," and the whole relaxation rests on
   that phrase.
2. **The relaxation costs more than it saves.** Implementing it means defining, enforcing,
   and *proving* continuity of controlled access — inactivity, screen lock, device change,
   network change, tab restore — and every edge case is an audit question. A password prompt
   is cheaper than the evidence that a prompt was not needed.
3. **The direction of retrofit favours strict.** Relaxing later is a policy flag and a
   re-validated signing flow. Tightening later, after a customer has validated the loose
   behaviour, invalidates their validation. Strict first is the reversible choice.
4. **It matches what we already say.** `docs/02` already promises re-authentication on
   signing. The conservative default makes that sentence literally true instead of
   approximately true.

Consequences for `wicket-esign` and `wicket-identity`: signing always prompts for the
identification code and password, or an IdP step-up for OIDC shops (SSO does not make a shop
Part 11-complete, and the step-up is required); a signature records the signer, the server
time, the printed meaning of the signature, and the hash of the exact record version signed;
the signature's `esign_id` appears on the audit rows of the transaction it authorised; failed
signing attempts are `audit.log_event` security events; and an administrator-initiated
credential reset must not let one person alone assume another's identity, which is
11.200(a)(3) and a `wicket-identity` acceptance criterion. Biometrics are out of scope and no
placeholder column for them is created.

---

## 10. Acceptance criteria — what each case actually produces

| # | Situation | What happens, mechanically | Where it is proven |
|---|---|---|---|
| **a** | Module author writes a normal `INSERT` and forgets audit entirely | `zz_audit_row` fires; `audit.require_context()` reads the transaction-local context `Tx::begin` set; a complete audit row is written in the same transaction with server time, actor, `new_row`, action, and document. The author wrote nothing and could not have prevented it. If the table came from their own migration, the `audit_attach` event trigger attached the triggers at `CREATE TABLE` — including for tables nobody has written yet | `wicket-audit`: insert into a fresh table created inside the test migration, assert one audit row with the right `row_key` and `changed_columns` |
| **b** | Module author deliberately tries to write a false audit row | `INSERT INTO audit.event ...` as `wicket_app` → `42501 permission denied for table event`. Through `audit.log_event`, the column-level grant on `wicket_audit_event` makes `op`, `table_name`, `old_row`, `new_row` unwritable and the `event_shape` CHECK rejects the attempt, while `actor_id`, `at`, `stmt_at`, and `xid` are stamped by the server from context the caller cannot borrow from another transaction. `UPDATE` / `DELETE` on `audit.event` → `42501`. Residual: in-process code can name another *real* authenticated principal (§2.5), which is inside the trust boundary and is not claimed against | `wicket-audit`: four negative tests, each asserting SQLSTATE `42501` |
| **c** | Request arrives with no authenticated actor | No `Authenticated`, so no `WriteContext`, so no `Tx::begin`: the handler returns 401 and no transaction opens. If a write reaches PostgreSQL anyway, `audit.require_context()` raises `42501` in the `AFTER` trigger and the **entire transaction aborts** — the business row is not written, and nothing is recorded as "unknown," which `actor_id NOT NULL REFERENCES identity.principal` makes unrepresentable. A `security.unattributable_write` event is logged on a separate connection | `wicket-db`: write on a raw pool connection without context, assert abort and that the target table is unchanged |
| **d** | Two requests reuse the same pooled connection back to back | Request 1's context was set with `set_config(..., true)`, so PostgreSQL discards it at `COMMIT`; `after_release` additionally runs `RESET ALL` and discards the connection if that fails. Request 2's `Tx::begin` sets its own. If any future path leaked a session-level value, `wicket.txid` would not equal `pg_current_xact_id()` and request 2's first write would be **refused**, never misattributed | `wicket-db`: pool with `max_connections = 1`, two sequential writes by different actors, assert two audit rows with the correct distinct actors; plus a test that sets a session-level `wicket.actor_id` by hand and asserts the next write fails |
| **e** | Someone with database superuser access edits a historical audit row | Grants do not stop them and `audit_protect` can be disabled by them. The edited row's transaction no longer reproduces its seal's `rows_digest`, so `audit.verify` fails at that `seq`. To hide it they must re-seal that transaction and, because each `hash` includes `prev_hash`, every seal after it — which changes the chain head. The head then disagrees with the off-box anchor recorded before the edit, and `wicket-verify` on a separate machine localises the divergence to "after anchor N." Uncovered case, stated plainly: history written since the last off-box anchor, or a site that configured no anchor sink, is not detectable — which is why the IQ suite fails with no sink configured and the health page nags when anchors go stale | `wicket-audit`: tamper test as owner, assert `audit.verify` fails at the expected `seq`; re-seal test, assert the head diverges from a stored anchor |
| **f** | Transaction rolls back after consuming a document number | Nothing was consumed. The number came from `UPDATE numbering.counter ... RETURNING`, which rolls back with the transaction; the next allocation returns the same value. No audit row exists for the failed attempt, which is correct — Part 11 audits changes to records, not attempts — and a failed *signing* or a permission denial is separately recorded as a security event on its own connection. The seal chain has no gap either: `seq` is `prev.seq + 1` taken under a transaction-scoped advisory lock, so an aborted transaction leaves the head where it was | `wicket-numbering`: abort-then-reallocate test; concurrent allocation test asserting a contiguous committed set. `wicket-audit`: abort test asserting no seal and an unchanged head |
| **g** | Investigator asks for every change to one work order, readable and electronic, eight years later | `wicket audit export --doc work_order:<id>` selects on `(doc_type, doc_id)` across the monthly partitions — including archived ones, restored as export bundles — and emits one bundle containing `events.ndjson`, `events.csv`, `seals.ndjson`, `manifest.json`, `dictionary.md`, and `report.pdf`. The PDF is the human-readable copy, with actor display names as of each change, local and UTC times, reasons, changed-column before/after tables, and 11.50(b) signature manifestations where a signature is linked. Both forms carry the same `export_id` and file digests, so the paper and electronic copies are provably the same records. The bundle is self-describing and readable with no Wicket install, which is the answer to reading it after a program change; the seals and anchors travel with it, so the investigator's own verification runs off the box | `wicket-audit`: golden-bundle test on a seeded work order, digest-stability test, and an IQ-suite named test that exports, verifies, and renders |

---

## 11. What this decision obliges, by lane

Binding. These are not suggestions to the executors.

**Wave 1 `workspace`** gains a subtask, split from the skeleton so the skeleton can merge
first: `dev/init-roles.sql` with the five roles and the grants of §1.1 and §1.3; two
`DATABASE_URL`s; the `clippy.toml` deny-list plus a CI `rg` gate for `sqlx::query*`,
`QueryBuilder`, `raw_sql`, `copy_in_raw`, `set_config`, and `current_setting` outside
`wicket-db` and `wicket-audit`; a pinned SQLx version and a pinned PostgreSQL major version in
the test fixture; `after_connect` / `after_release` as written in §2.2; and `Tx::begin`
**real, not `todo!()`**, in the `wicket-db` stub, because thirteen Wave 2 lanes will otherwise
each invent it.

**Wave 2 `wicket-audit`** owns §§1.2–1.5, §6, and the exporter. Serial after `wicket-db`, deep
tier. Its acceptance criteria are the seven rows of §10, each as a named test, plus the
`audit.exempt` rows for `numbering.counter`, `audit.tx_seal`, and `audit.anchor` with their
reasons.

**Wave 2 `wicket-db`** owns §2, deep tier, including the separate-connection security-event
path, `READ COMMITTED` by default, and serialisation retry.

**Test databases** are ephemeral per test (`#[sqlx::test]`, or a `CREATE DATABASE ...
TEMPLATE` helper). No role anywhere is granted `DELETE` on `audit.event` for test
convenience. A test that needs a clean audit table is in the wrong database.

**Flagged, outside this decision's edit permission** — each needs an owner:

1. `docs/02-architecture.md:251` claims the audit store is append-only at the database level.
   It must be scoped to the application role and point at §7's statement.
2. `docs/01-vision-and-scope.md` and any sales prose must quote §7 verbatim or weaker.
3. ADR 0003 does not pin a PostgreSQL major version; §1 requires 15 or later.
4. `docs/06` (regulatory, unwritten) owns the host-clock honesty statement from §4, the
   anchor SOP from §6.2, and the OIDC step-up statement from §9.
5. PLAN §5 has no print crate; `report.pdf` and 11.50(b) manifestations have no home without
   one (plan-audit G8, D6).
6. **Proposed PLAN §6 invariant 9**, for whoever owns the next §6 amendment: *"Regulated
   document numbers are gap-free. They are allocated from a counter row in the caller's
   transaction, never from a PostgreSQL sequence, and a committed number is never reused;
   cancellation is a visible status, not a missing number."*
7. `wicket-jobs` needs `Actor` / `WriteContext` from `wicket-core` and `Tx` from `wicket-db`
   (plan-audit R5); no edge to `wicket-identity` or `wicket-audit` is required.

---

## 12. Summary of the decisions

| # | Question | Decision |
|---|---|---|
| 1 | Where the guarantee lives | PostgreSQL `AFTER ... FOR EACH ROW` trigger, generic, attached by an event trigger at `CREATE TABLE`, writing through `SECURITY DEFINER`. Not SQLx |
| 2 | How the actor reaches it | One `set_config(..., is_local := true)` statement from `Tx::begin`, plus `wicket.txid` checked against `pg_current_xact_id()`. `RESET ALL` on release as belt. Absent or stale context aborts the transaction |
| 3 | Does `wicket_app` keep `INSERT` on audit | **No.** `SELECT` only. Kernel events go through a column-restricted `SECURITY DEFINER` function |
| 4 | Time source | `at = now()`, `stmt_at = clock_timestamp()`, `xid` stored, all `timestamptz` UTC, all server-side. Honest about the host clock |
| 5 | Row contents and export | Actor plus display, provenance, device, action, required-by-catalogue reason, `doc_type` / `doc_id`, `esign_id`, `old_row` / `new_row` / `changed_columns`, monthly partitions; export bundle of NDJSON, CSV, seals, manifest, dictionary, and PDF/A under one `export_id` |
| 6 | Hash chain in v1 | **Yes.** Per-transaction seal chain sealed by a deferred constraint trigger; verified off-box by the quality manager against anchors published off the server; no on-box key signing |
| 7 | The claim | §7, verbatim, everywhere. Tamper-evident, not tamper-proof; the trust boundary is named |
| 8 | Numbering | Transactional counter row. `nextval()` forbidden for regulated numbers |
| 9 | 11.200 | Conservative default confirmed: all identification components, every signing |

*End of decision. Amends `docs/adr/0005-compliance-in-kernel.md` and `PLAN.md` §6
invariants 3–5. No product code was written.*
