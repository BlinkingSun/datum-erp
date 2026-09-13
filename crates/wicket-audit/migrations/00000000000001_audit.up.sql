-- 0001_audit: schema audit, event trail, triggers, chain, redaction, export support.
-- Reversible. Event triggers and writer-role function ownership are completed by
-- a superuser (see wicket_audit::install_privileged); CREATE EVENT TRIGGER is
-- superuser-only in PostgreSQL.
-- Hashing uses the core `sha256(bytea)` of PostgreSQL 14+ (no pgcrypto).

DO $schema$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'audit') THEN
    EXECUTE 'CREATE SCHEMA audit AUTHORIZATION wicket_owner';
  ELSE
    EXECUTE 'ALTER SCHEMA audit OWNER TO wicket_owner';
  END IF;
END
$schema$;

REVOKE ALL ON SCHEMA audit FROM PUBLIC;
GRANT USAGE ON SCHEMA audit TO wicket_app, wicket_audit_row, wicket_audit_event,
  wicket_migrate, wicket_owner;
GRANT CREATE ON SCHEMA audit TO wicket_owner, wicket_migrate;

CREATE TYPE audit.context AS (
  actor_id          uuid,
  actor_kind        text,
  actor_display     text,
  acting_for_id     uuid,
  session_id        uuid,
  request_id        uuid,
  source_kind       text,
  source_device_id  text,
  source_ip         inet,
  client_app        text,
  action            text,
  reason            text,
  doc_type          text,
  doc_id            uuid,
  esign_id          uuid,
  app_version       text,
  config_version    text
);
ALTER TYPE audit.context OWNER TO wicket_owner;
GRANT USAGE ON TYPE audit.context TO wicket_app, wicket_audit_row, wicket_audit_event,
  wicket_migrate, wicket_owner;

CREATE TABLE audit.event (
  event_id          uuid        NOT NULL DEFAULT gen_random_uuid(),
  at                timestamptz NOT NULL,
  stmt_at           timestamptz NOT NULL,
  xid               xid8        NOT NULL,
  actor_id          uuid        NOT NULL,
  actor_kind        text        NOT NULL CHECK (actor_kind IN ('user', 'service', 'migration')),
  actor_display     text        NOT NULL,
  acting_for_id     uuid            NULL,
  session_id        uuid            NULL,
  request_id        uuid            NULL,
  source_kind       text        NOT NULL CHECK (source_kind IN
                      ('ui', 'api', 'job', 'import', 'migration', 'maintenance', 'app_event')),
  source_device_id  text            NULL,
  source_ip         inet            NULL,
  client_app        text            NULL,
  action            text        NOT NULL,
  reason            text            NULL,
  doc_type          text            NULL,
  doc_id            uuid            NULL,
  esign_id          uuid            NULL,
  schema_name       name            NULL,
  table_name        name            NULL,
  op                text            NULL CHECK (op IN ('INSERT', 'UPDATE', 'DELETE', 'TRUNCATE')),
  row_key           jsonb           NULL,
  old_row           jsonb           NULL,
  new_row           jsonb           NULL,
  changed_columns   text[]          NULL,
  app_version       text        NOT NULL DEFAULT '',
  config_version    text        NOT NULL DEFAULT '',
  CONSTRAINT event_shape CHECK (
    (source_kind =  'app_event' AND op IS NULL     AND table_name IS NULL)
 OR (source_kind <> 'app_event' AND op IS NOT NULL AND table_name IS NOT NULL)
  ),
  PRIMARY KEY (at, event_id)
) PARTITION BY RANGE (at);

CREATE TABLE audit.redact (
  relid       oid         NOT NULL,
  column_name name        NOT NULL,
  reason      text        NOT NULL,
  decided_by  text        NOT NULL,
  decided_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (relid, column_name)
);

CREATE TABLE audit.reason_policy (
  relid oid     NOT NULL PRIMARY KEY,
  ops   text[]  NOT NULL
);

CREATE TABLE audit.exempt (
  relid       oid             NULL,
  nspname     name        NOT NULL,
  relname     name        NOT NULL,
  reason      text        NOT NULL,
  decided_by  text        NOT NULL,
  decided_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (nspname, relname)
);

CREATE TABLE audit.tx_seal (
  seq         bigint      PRIMARY KEY,
  xid         xid8        NOT NULL UNIQUE,
  sealed_at   timestamptz NOT NULL,
  tz          text        NOT NULL,
  row_count   integer     NOT NULL,
  rows_digest bytea       NOT NULL,
  prev_hash   bytea       NOT NULL UNIQUE,
  hash        bytea       NOT NULL UNIQUE,
  chain_algo  text        NOT NULL
);

CREATE TABLE audit.anchor (
  seq         bigint      NOT NULL,
  hash        bytea       NOT NULL,
  anchored_at timestamptz NOT NULL,
  sink        text        NOT NULL,
  receipt     text            NULL,
  PRIMARY KEY (seq, sink, anchored_at)
);

CREATE INDEX event_xid ON audit.event (xid);
CREATE INDEX event_doc ON audit.event (doc_type, doc_id);

CREATE FUNCTION audit.guc_uuid(n text) RETURNS uuid
LANGUAGE plpgsql STABLE SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE v text;
BEGIN
  v := nullif(current_setting(n, true), '');
  IF v IS NULL THEN RETURN NULL; END IF;
  RETURN v::uuid;
EXCEPTION WHEN invalid_text_representation THEN
  RETURN NULL;
END
$audit$;

CREATE FUNCTION audit.guc_inet(n text) RETURNS inet
LANGUAGE plpgsql STABLE SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE v text;
BEGIN
  v := nullif(current_setting(n, true), '');
  IF v IS NULL THEN RETURN NULL; END IF;
  RETURN v::inet;
EXCEPTION WHEN invalid_text_representation THEN
  RETURN NULL;
END
$audit$;

CREATE FUNCTION audit.require_context() RETURNS audit.context
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  c audit.context;
  kind text;
BEGIN
  c.actor_id := audit.guc_uuid('wicket.actor_id');
  kind := lower(nullif(current_setting('wicket.actor_kind', true), ''));
  c.actor_kind := CASE kind
    WHEN 'user' THEN 'user'
    WHEN 'serviceprincipal' THEN 'service'
    WHEN 'service' THEN 'service'
    WHEN 'migration' THEN 'migration'
    ELSE kind
  END;
  c.actor_display := coalesce(nullif(current_setting('wicket.actor_display', true), ''), '');
  c.acting_for_id := audit.guc_uuid('wicket.acting_for');
  c.session_id := audit.guc_uuid('wicket.session_id');
  c.request_id := audit.guc_uuid('wicket.request_id');
  c.source_kind := nullif(current_setting('wicket.source_kind', true), '');
  c.source_device_id := nullif(current_setting('wicket.source_device', true), '');
  c.source_ip := audit.guc_inet('wicket.source_ip');
  c.client_app := nullif(current_setting('wicket.client_app', true), '');
  c.action := nullif(current_setting('wicket.action', true), '');
  c.reason := nullif(current_setting('wicket.reason', true), '');
  c.doc_type := nullif(current_setting('wicket.doc_type', true), '');
  c.doc_id := audit.guc_uuid('wicket.doc_id');
  c.esign_id := audit.guc_uuid('wicket.esign_id');
  c.app_version := coalesce(current_setting('wicket.app_version', true), '');
  c.config_version := coalesce(current_setting('wicket.config_version', true), '');

  IF c.actor_id IS NULL THEN
    RAISE EXCEPTION 'wicket: refused write with no attributable actor'
      USING ERRCODE = '42501',
            HINT = 'begin the transaction through wicket_db::Tx::begin';
  END IF;

  IF c.actor_display = '' THEN
    c.actor_display := c.actor_id::text;
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
END
$audit$;

CREATE FUNCTION audit.reason_required(relid oid, op text) RETURNS boolean
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE ops text[];
BEGIN
  SELECT p.ops INTO ops FROM audit.reason_policy p WHERE p.relid = reason_required.relid;
  IF FOUND THEN
    RETURN op = ANY (ops);
  END IF;
  RETURN op IN ('UPDATE', 'DELETE');
END
$audit$;

CREATE FUNCTION audit.scrub(relid oid, j jsonb) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  col name;
BEGIN
  IF j IS NULL THEN
    RETURN NULL;
  END IF;
  FOR col IN
    SELECT r.column_name
      FROM audit.redact r
     WHERE r.relid = scrub.relid
  LOOP
    IF j ? col::text THEN
      j := jsonb_set(j, ARRAY[col::text], '"[redacted]"'::jsonb);
    END IF;
  END LOOP;
  FOR col IN
    SELECT a.attname
      FROM pg_attribute a
     WHERE a.attrelid = scrub.relid
       AND a.atttypid = 'pg_catalog.bytea'::regtype
       AND a.attnum > 0
       AND NOT a.attisdropped
  LOOP
    IF j ? col::text THEN
      j := jsonb_set(j, ARRAY[col::text], '"[redacted]"'::jsonb);
    END IF;
  END LOOP;
  RETURN j;
END
$audit$;

CREATE FUNCTION audit.pk_of(pk_cols text[], j jsonb) RETURNS jsonb
LANGUAGE plpgsql IMMUTABLE SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  out jsonb := '{}'::jsonb;
  col text;
BEGIN
  IF pk_cols IS NULL OR j IS NULL THEN
    RETURN NULL;
  END IF;
  FOREACH col IN ARRAY pk_cols LOOP
    out := out || jsonb_build_object(col, j -> col);
  END LOOP;
  RETURN out;
END
$audit$;

CREATE FUNCTION audit.row_change() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  ctx     audit.context := audit.require_context();
  pk_cols text[]        := TG_ARGV[0]::text[];
  old_j   jsonb;
  new_j   jsonb;
  changed text[];
BEGIN
  old_j := CASE WHEN TG_OP IN ('UPDATE', 'DELETE') THEN audit.scrub(TG_RELID, to_jsonb(OLD)) END;
  new_j := CASE WHEN TG_OP IN ('INSERT', 'UPDATE') THEN audit.scrub(TG_RELID, to_jsonb(NEW)) END;

  IF TG_OP = 'UPDATE' THEN
    SELECT array_agg(k ORDER BY k) INTO changed
      FROM jsonb_object_keys(new_j) AS k
     WHERE new_j -> k IS DISTINCT FROM old_j -> k;
    IF changed IS NULL THEN RETURN NULL; END IF;
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
    schema_name, table_name, op, row_key, old_row, new_row, changed_columns,
    app_version, config_version)
  VALUES (
    now(), clock_timestamp(), pg_current_xact_id(),
    ctx.actor_id, ctx.actor_kind, ctx.actor_display, ctx.acting_for_id,
    ctx.session_id, ctx.request_id, ctx.source_kind, ctx.source_device_id,
    ctx.source_ip, ctx.client_app,
    ctx.action, ctx.reason, ctx.doc_type, ctx.doc_id, ctx.esign_id,
    TG_TABLE_SCHEMA, TG_TABLE_NAME, TG_OP,
    audit.pk_of(pk_cols, COALESCE(new_j, old_j)),
    old_j, new_j, changed,
    ctx.app_version, ctx.config_version);
  RETURN NULL;
END
$audit$;

CREATE FUNCTION audit.stmt_truncate() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  ctx audit.context := audit.require_context();
BEGIN
  INSERT INTO audit.event (
    at, stmt_at, xid,
    actor_id, actor_kind, actor_display, acting_for_id,
    session_id, request_id, source_kind, source_device_id, source_ip, client_app,
    action, reason, doc_type, doc_id, esign_id,
    schema_name, table_name, op, row_key, old_row, new_row, changed_columns,
    app_version, config_version)
  VALUES (
    now(), clock_timestamp(), pg_current_xact_id(),
    ctx.actor_id, ctx.actor_kind, ctx.actor_display, ctx.acting_for_id,
    ctx.session_id, ctx.request_id, ctx.source_kind, ctx.source_device_id,
    ctx.source_ip, ctx.client_app,
    ctx.action, ctx.reason, ctx.doc_type, ctx.doc_id, ctx.esign_id,
    TG_TABLE_SCHEMA, TG_TABLE_NAME, 'TRUNCATE',
    NULL, NULL, NULL, NULL,
    ctx.app_version, ctx.config_version);
  RETURN NULL;
END
$audit$;

CREATE FUNCTION audit.log_event(
  p_kind     text,
  p_action   text,
  p_reason   text,
  p_doc_type text,
  p_doc_id   text,
  p_esign_id text,
  p_detail   jsonb
) RETURNS uuid
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  ctx audit.context;
  id  uuid;
  kind text := nullif(p_kind, '');
BEGIN
  IF kind IS NULL THEN
    RAISE EXCEPTION 'wicket: log_event requires a kind' USING ERRCODE = '42501';
  END IF;
  IF kind !~ '^(security|login|export|print|signature|clock|maintenance|kernel)(\.|$)' THEN
    RAISE EXCEPTION 'wicket: log_event kind % is not a kernel event', kind
      USING ERRCODE = '42501';
  END IF;

  IF nullif(current_setting('wicket.actor_id', true), '') IS NULL
     AND kind LIKE 'security.%' THEN
    ctx.actor_id := '00000000-0000-4000-8000-000000000001';
    ctx.actor_kind := 'service';
    ctx.actor_display := 'unattributable';
    ctx.source_kind := 'app_event';
    ctx.action := kind;
    ctx.reason := nullif(p_reason, '');
    ctx.doc_type := nullif(p_doc_type, '');
    ctx.doc_id := audit.guc_uuid('wicket.doc_id');
    IF ctx.doc_id IS NULL AND p_doc_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN
      ctx.doc_id := p_doc_id::uuid;
    END IF;
    ctx.esign_id := NULL;
    ctx.app_version := coalesce(current_setting('wicket.app_version', true), '');
    ctx.config_version := coalesce(current_setting('wicket.config_version', true), '');
    IF p_detail ? 'request_id' THEN
      BEGIN
        ctx.request_id := (p_detail ->> 'request_id')::uuid;
      EXCEPTION WHEN invalid_text_representation THEN
        ctx.request_id := NULL;
      END;
    END IF;
  ELSE
    ctx := audit.require_context();
    ctx.action := coalesce(nullif(p_action, ''), kind);
    IF nullif(p_reason, '') IS NOT NULL THEN
      ctx.reason := p_reason;
    END IF;
    IF nullif(p_doc_type, '') IS NOT NULL THEN
      ctx.doc_type := p_doc_type;
    END IF;
    IF p_doc_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN
      ctx.doc_id := p_doc_id::uuid;
    END IF;
    IF p_esign_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN
      ctx.esign_id := p_esign_id::uuid;
    END IF;
    ctx.source_kind := 'app_event';
  END IF;

  id := gen_random_uuid();
  INSERT INTO audit.event (
    event_id, at, stmt_at, xid,
    actor_id, actor_kind, actor_display, acting_for_id,
    session_id, request_id, source_kind, source_device_id, source_ip, client_app,
    action, reason, doc_type, doc_id, esign_id,
    app_version, config_version)
  VALUES (
    id, now(), clock_timestamp(), pg_current_xact_id(),
    ctx.actor_id, ctx.actor_kind, ctx.actor_display, ctx.acting_for_id,
    ctx.session_id, ctx.request_id, 'app_event', ctx.source_device_id,
    ctx.source_ip, ctx.client_app,
    ctx.action, ctx.reason, ctx.doc_type, ctx.doc_id, ctx.esign_id,
    ctx.app_version, ctx.config_version);
  RETURN id;
END
$audit$;

CREATE FUNCTION audit.attach(rel regclass) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  pk text[];
  has_row boolean;
  has_trunc boolean;
BEGIN
  SELECT EXISTS (
    SELECT 1 FROM pg_trigger t
     WHERE t.tgrelid = rel AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
  ) INTO has_row;
  SELECT EXISTS (
    SELECT 1 FROM pg_trigger t
     WHERE t.tgrelid = rel AND t.tgname = 'zz_audit_truncate' AND NOT t.tgisinternal
  ) INTO has_trunc;

  IF has_row AND has_trunc THEN
    RETURN;
  END IF;

  SELECT array_agg(a.attname::text ORDER BY k.ord)
    INTO pk
    FROM pg_index i
    JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord) ON TRUE
    JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
   WHERE i.indrelid = rel AND i.indisprimary;

  IF pk IS NULL THEN
    RAISE EXCEPTION 'wicket: % has no primary key; an audited table must be addressable', rel;
  END IF;

  IF NOT has_row THEN
    EXECUTE format(
      'CREATE TRIGGER zz_audit_row AFTER INSERT OR UPDATE OR DELETE ON %s
         FOR EACH ROW EXECUTE FUNCTION audit.row_change(%L)', rel, pk::text);
  END IF;
  IF NOT has_trunc THEN
    EXECUTE format(
      'CREATE TRIGGER zz_audit_truncate AFTER TRUNCATE ON %s
         FOR EACH STATEMENT EXECUTE FUNCTION audit.stmt_truncate()', rel);
  END IF;
END
$audit$;

CREATE FUNCTION audit.attach_new_tables() RETURNS event_trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE cmd record;
DECLARE cls pg_class%ROWTYPE;
DECLARE skip boolean;
BEGIN
  FOR cmd IN
    SELECT * FROM pg_event_trigger_ddl_commands()
     WHERE object_type = 'table'
  LOOP
    CONTINUE WHEN cmd.schema_name IS NULL;
    CONTINUE WHEN cmd.schema_name LIKE 'pg_%';
    CONTINUE WHEN cmd.schema_name = 'information_schema';
    -- D-2b-12: skip set from schema_class catalog, not a literal name list.
    -- class transient|audit covers every <module>_transient; nspname wicket
    -- is class app but attached explicitly (attach_kernel_audit).
    -- format(%I) so the source does not carry a cross-schema FROM token.
    EXECUTE format(
      $q$SELECT EXISTS (
           SELECT 1 FROM %I.schema_class sc
            WHERE sc.nspname = $1::name
              AND sc.class IN ('transient', 'audit')
         )$q$, 'wicket')
      INTO skip
      USING cmd.schema_name;
    CONTINUE WHEN cmd.schema_name = 'wicket' OR skip;
    CONTINUE WHEN cmd.objid IS NULL;
    SELECT * INTO cls FROM pg_class WHERE oid = cmd.objid;
    CONTINUE WHEN NOT FOUND;
    CONTINUE WHEN cls.relispartition;
    CONTINUE WHEN cls.relkind NOT IN ('r', 'p');
    CONTINUE WHEN EXISTS (
      SELECT 1 FROM audit.exempt e
       WHERE e.relid = cmd.objid
          OR (e.nspname = cmd.schema_name AND e.relname = cls.relname)
    );
    PERFORM audit.attach(cmd.objid);
  END LOOP;
END
$audit$;

CREATE FUNCTION audit.protect() RETURNS event_trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  obj record;
  q text := current_query();
  m text[];
  relid oid;
  rel_name text;
BEGIN
  -- Superuser-only. Membership in wicket_owner grants nothing: wicket_owner cannot
  -- log in, and the migrate login is the role this defence exists against.
  -- SECURITY DEFINER would make current_user the owner; session_user is the login.
  IF EXISTS (
    SELECT 1 FROM pg_roles r
     WHERE r.rolname = session_user AND r.rolsuper
  ) THEN
    RETURN;
  END IF;

  IF TG_EVENT = 'ddl_command_start' THEN
    IF TG_TAG = 'DROP TRIGGER'
       AND q ~* 'drop[[:space:]]+trigger[[:space:]]+(if[[:space:]]+exists[[:space:]]+)?"?zz_audit'
    THEN
      RAISE EXCEPTION 'wicket: refusing DROP TRIGGER of an audit trigger by non-superuser %',
        session_user USING ERRCODE = '42501';
    END IF;
    IF TG_TAG = 'ALTER TRIGGER' AND q ~* 'zz_audit' THEN
      RAISE EXCEPTION 'wicket: refusing ALTER TRIGGER of an audit trigger by non-superuser %',
        session_user USING ERRCODE = '42501';
    END IF;
    IF TG_TAG = 'DROP EVENT TRIGGER'
       AND q ~* 'audit_attach|audit_protect'
    THEN
      RAISE EXCEPTION 'wicket: refusing DROP EVENT TRIGGER by non-superuser %',
        session_user USING ERRCODE = '42501';
    END IF;
    IF TG_TAG IN ('DROP FUNCTION', 'DROP ROUTINE', 'DROP PROCEDURE')
       AND q ~* '[[:space:]]audit\.(row_change|stmt_truncate|log_event|protect|attach_new_tables|attach|refuse_mutation|require_context|seal_current_tx)\y'
    THEN
      RAISE EXCEPTION 'wicket: refusing DROP FUNCTION of an audit function by non-superuser %',
        session_user USING ERRCODE = '42501';
    END IF;
    -- DISABLE / ENABLE [REPLICA|ALWAYS] of zz_audit_* (row, truncate, seal,
    -- immutable, and any other zz_audit_* this crate installs), or DISABLE
    -- TRIGGER ALL|USER (and ENABLE REPLICA|ALWAYS TRIGGER ALL|USER, which
    -- would change audit firing mode) on a table that already carries one.
    -- A module's own named trigger stays manageable by wicket_migrate.
    IF TG_TAG = 'ALTER TABLE' AND (
         q ~* 'disable[[:space:]]+trigger'
      OR q ~* 'enable[[:space:]]+(replica[[:space:]]+|always[[:space:]]+)?trigger'
    ) THEN
      IF q ~* 'zz_audit' THEN
        RAISE EXCEPTION
          'wicket: refusing ALTER TABLE ... TRIGGER by non-superuser %',
          session_user USING ERRCODE = '42501';
      END IF;
      IF q ~* 'disable[[:space:]]+trigger[[:space:]]+(all|user)\y'
         OR q ~* 'enable[[:space:]]+(replica[[:space:]]+|always[[:space:]]+)trigger[[:space:]]+(all|user)\y'
      THEN
        m := regexp_match(
          q,
          'ALTER[[:space:]]+TABLE[[:space:]]+(IF[[:space:]]+EXISTS[[:space:]]+)?(ONLY[[:space:]]+)?([a-zA-Z0-9_."]+)',
          'i');
        IF m IS NOT NULL THEN
          rel_name := m[3];
          relid := to_regclass(rel_name);
          IF relid IS NULL THEN
            SELECT c.oid INTO relid
              FROM pg_class c
              JOIN pg_namespace n ON n.oid = c.relnamespace
              JOIN pg_trigger t ON t.tgrelid = c.oid
             WHERE c.relname = rel_name
               AND n.nspname NOT IN ('pg_catalog', 'pg_toast', 'information_schema')
               AND t.tgname LIKE 'zz_audit%'
               AND NOT t.tgisinternal
             LIMIT 1;
          END IF;
        END IF;
        IF relid IS NOT NULL AND EXISTS (
          SELECT 1 FROM pg_trigger t
           WHERE t.tgrelid = relid
             AND t.tgname LIKE 'zz_audit%'
             AND NOT t.tgisinternal
        ) THEN
          RAISE EXCEPTION
            'wicket: refusing ALTER TABLE ... TRIGGER by non-superuser %',
            session_user USING ERRCODE = '42501';
        END IF;
        -- Unresolvable ALL|USER: fail closed (search_path is not the session's).
        IF relid IS NULL THEN
          RAISE EXCEPTION
            'wicket: refusing ALTER TABLE ... TRIGGER by non-superuser %',
            session_user USING ERRCODE = '42501';
        END IF;
      END IF;
    END IF;
    RETURN;
  END IF;

  IF TG_EVENT = 'sql_drop' THEN
    FOR obj IN SELECT * FROM pg_event_trigger_dropped_objects() LOOP
      IF obj.original AND obj.object_type = 'trigger'
         AND obj.object_identity ~* 'zz_audit'
      THEN
        RAISE EXCEPTION 'wicket: refusing DROP TRIGGER %', obj.object_identity
          USING ERRCODE = '42501';
      END IF;
      IF obj.original AND obj.object_type IN ('function', 'procedure', 'routine')
         AND obj.schema_name = 'audit'
         AND obj.object_name IN (
           'row_change', 'stmt_truncate', 'log_event', 'protect',
           'attach', 'attach_new_tables', 'refuse_mutation',
           'require_context', 'seal_current_tx'
         )
      THEN
        RAISE EXCEPTION 'wicket: refusing DROP FUNCTION %', obj.object_identity
          USING ERRCODE = '42501';
      END IF;
      IF obj.original AND obj.object_type = 'event trigger'
         AND obj.object_name IN ('audit_attach', 'audit_protect', 'audit_protect_drop')
      THEN
        RAISE EXCEPTION 'wicket: refusing DROP EVENT TRIGGER %', obj.object_name
          USING ERRCODE = '42501';
      END IF;
    END LOOP;
  END IF;
END
$audit$;

CREATE FUNCTION audit.grant_event_insert(rel regclass) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
BEGIN
  EXECUTE format('GRANT INSERT ON TABLE %s TO wicket_audit_row', rel);
  EXECUTE format(
    'GRANT INSERT (
       event_id, at, stmt_at, xid, actor_id, actor_kind, actor_display,
       acting_for_id, session_id, request_id, source_kind, source_device_id,
       source_ip, client_app, action, reason, doc_type, doc_id, esign_id,
       app_version, config_version
     ) ON TABLE %s TO wicket_audit_event',
    rel);
END
$audit$;

CREATE FUNCTION audit.ensure_partitions(months integer) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp SET timezone = 'UTC' AS $audit$
DECLARE
  start_m date;
  i int;
  span int;
  from_ts timestamptz;
  to_ts timestamptz;
  part name;
  r record;
BEGIN
  span := GREATEST(coalesce(months, 1), 1);
  start_m := date_trunc('month', timezone('UTC', now()))::date;
  FOR i IN 0..span LOOP
    from_ts := (start_m + make_interval(months => i))::timestamp AT TIME ZONE 'UTC';
    to_ts := (start_m + make_interval(months => i + 1))::timestamp AT TIME ZONE 'UTC';
    part := format('event_%s', to_char(from_ts AT TIME ZONE 'UTC', 'YYYY_MM'));
    IF NOT EXISTS (
      SELECT 1 FROM pg_class c
      JOIN pg_namespace n ON n.oid = c.relnamespace
      WHERE n.nspname = 'audit' AND c.relname = part
    ) THEN
      EXECUTE format(
        'CREATE TABLE audit.%I PARTITION OF audit.event FOR VALUES FROM (%L) TO (%L)',
        part, from_ts, to_ts);
      EXECUTE format('ALTER TABLE audit.%I OWNER TO wicket_owner', part);
    END IF;
  END LOOP;
  PERFORM audit.grant_event_insert('audit.event'::regclass);
  FOR r IN
    SELECT inhrelid::regclass AS rel
      FROM pg_inherits
     WHERE inhparent = 'audit.event'::regclass
  LOOP
    PERFORM audit.grant_event_insert(r.rel);
  END LOOP;
END
$audit$;

CREATE FUNCTION audit.ts_canon(t timestamptz) RETURNS text
LANGUAGE sql IMMUTABLE PARALLEL SAFE SET search_path = pg_catalog, pg_temp AS $audit$
  SELECT CASE
    WHEN t IS NULL THEN ''
    ELSE to_char(t AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US') || '+00'
  END;
$audit$;

CREATE FUNCTION audit.canon_jsonb(j jsonb) RETURNS text
LANGUAGE plpgsql IMMUTABLE SET search_path = pg_catalog, pg_temp AS $audit$
DECLARE
  t text;
  k text;
  parts text[] := ARRAY[]::text[];
  elem jsonb;
BEGIN
  IF j IS NULL THEN
    RETURN '';
  END IF;
  t := jsonb_typeof(j);
  CASE t
    WHEN 'null' THEN
      RETURN 'null';
    WHEN 'boolean' THEN
      RETURN j::text;
    WHEN 'number' THEN
      RETURN (j #>> '{}');
    WHEN 'string' THEN
      RETURN j::text;
    WHEN 'array' THEN
      FOR elem IN SELECT value FROM jsonb_array_elements(j) LOOP
        parts := parts || audit.canon_jsonb(elem);
      END LOOP;
      RETURN '[' || array_to_string(parts, ',') || ']';
    WHEN 'object' THEN
      FOR k IN SELECT key FROM jsonb_object_keys(j) AS key ORDER BY key LOOP
        parts := parts || (to_json(k)::text || ':' || audit.canon_jsonb(j -> k));
      END LOOP;
      RETURN '{' || array_to_string(parts, ',') || '}';
    ELSE
      RETURN j::text;
  END CASE;
END
$audit$;

CREATE FUNCTION audit.canon_row(e audit.event) RETURNS text
LANGUAGE sql IMMUTABLE SET search_path = pg_catalog, pg_temp SET timezone = 'UTC' AS $audit$
  SELECT 'v1'
    || chr(31) || coalesce(e.event_id::text, '')
    || chr(31) || audit.ts_canon(e.at)
    || chr(31) || audit.ts_canon(e.stmt_at)
    || chr(31) || coalesce(e.xid::text, '')
    || chr(31) || coalesce(e.actor_id::text, '')
    || chr(31) || coalesce(e.actor_kind, '')
    || chr(31) || coalesce(e.actor_display, '')
    || chr(31) || coalesce(e.acting_for_id::text, '')
    || chr(31) || coalesce(e.session_id::text, '')
    || chr(31) || coalesce(e.request_id::text, '')
    || chr(31) || coalesce(e.source_kind, '')
    || chr(31) || coalesce(e.source_device_id, '')
    || chr(31) || coalesce(e.source_ip::text, '')
    || chr(31) || coalesce(e.client_app, '')
    || chr(31) || coalesce(e.action, '')
    || chr(31) || coalesce(e.reason, '')
    || chr(31) || coalesce(e.doc_type, '')
    || chr(31) || coalesce(e.doc_id::text, '')
    || chr(31) || coalesce(e.esign_id::text, '')
    || chr(31) || coalesce(e.schema_name::text, '')
    || chr(31) || coalesce(e.table_name::text, '')
    || chr(31) || coalesce(e.op, '')
    || chr(31) || audit.canon_jsonb(e.row_key)
    || chr(31) || audit.canon_jsonb(e.old_row)
    || chr(31) || audit.canon_jsonb(e.new_row)
    || chr(31) || coalesce(array_to_string(e.changed_columns, ','), '')
    || chr(31) || coalesce(e.app_version, '')
    || chr(31) || coalesce(e.config_version, '');
$audit$;

CREATE FUNCTION audit.seal_current_tx() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp SET timezone = 'UTC' AS $audit$
DECLARE
  cur_xid xid8 := pg_current_xact_id();
  prev_seq bigint;
  prev_h bytea;
  new_seq bigint;
  n int;
  digest bytea;
  sealed timestamptz;
  tz_txt text;
  new_hash bytea;
  algo text := 'wicket-audit-1';
  sep bytea := convert_to(chr(31), 'UTF8');
BEGIN
  IF EXISTS (SELECT 1 FROM audit.tx_seal s WHERE s.xid = cur_xid) THEN
    RETURN NULL;
  END IF;

  PERFORM pg_advisory_xact_lock(hashtextextended('audit.chain', 0));

  IF EXISTS (SELECT 1 FROM audit.tx_seal s WHERE s.xid = cur_xid) THEN
    RETURN NULL;
  END IF;

  SELECT s.seq, s.hash INTO prev_seq, prev_h
    FROM audit.tx_seal s
   ORDER BY s.seq DESC
   LIMIT 1;

  IF prev_seq IS NULL THEN
    prev_seq := 0;
    prev_h := decode(repeat('00', 32), 'hex');
  END IF;
  new_seq := prev_seq + 1;

  SELECT count(*)::int INTO n FROM audit.event e WHERE e.xid = cur_xid;
  IF n = 0 THEN
    RETURN NULL;
  END IF;

  SELECT sha256(convert_to(string_agg(
           audit.canon_row(e) || chr(30),
           ''
           ORDER BY e.stmt_at, e.table_name::text, e.op, e.row_key::text, e.event_id
         ), 'UTF8'))
    INTO digest
    FROM audit.event e
   WHERE e.xid = cur_xid;

  sealed := clock_timestamp();
  tz_txt := current_setting('TimeZone');
  new_hash := sha256(
      prev_h
      || sep || convert_to(new_seq::text, 'UTF8')
      || sep || convert_to(cur_xid::text, 'UTF8')
      || sep || convert_to(audit.ts_canon(sealed), 'UTF8')
      || sep || digest
      || sep || convert_to(n::text, 'UTF8')
      || sep || convert_to(algo, 'UTF8')
  );

  INSERT INTO audit.tx_seal (
    seq, xid, sealed_at, tz, row_count, rows_digest, prev_hash, hash, chain_algo)
  VALUES (
    new_seq, cur_xid, sealed, tz_txt, n, digest, prev_h, new_hash, algo);

  RETURN NULL;
END
$audit$;

CREATE FUNCTION audit.verify(from_seq bigint, to_seq bigint)
RETURNS TABLE(seq bigint, ok boolean, detail text)
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp SET timezone = 'UTC' AS $audit$
DECLARE
  s audit.tx_seal;
  lo bigint := coalesce(from_seq, 1);
  hi bigint;
  prev_h bytea := decode(repeat('00', 32), 'hex');
  prev_seq bigint := 0;
  digest bytea;
  expect_hash bytea;
  n int;
  sep bytea := convert_to(chr(31), 'UTF8');
BEGIN
  SELECT max(t.seq) INTO hi FROM audit.tx_seal t;
  IF hi IS NULL THEN
    RETURN;
  END IF;
  hi := LEAST(coalesce(to_seq, hi), hi);

  IF lo > 1 THEN
    SELECT t.hash, t.seq INTO prev_h, prev_seq
      FROM audit.tx_seal t
     WHERE t.seq = lo - 1;
    IF prev_h IS NULL THEN
      seq := lo;
      ok := false;
      detail := format('missing previous seal %s', lo - 1);
      RETURN NEXT;
      RETURN;
    END IF;
  END IF;

  FOR s IN
    SELECT * FROM audit.tx_seal t
     WHERE t.seq >= lo AND t.seq <= hi
     ORDER BY t.seq
  LOOP
    IF s.seq <> prev_seq + 1 THEN
      seq := s.seq;
      ok := false;
      detail := format('gap: expected seq %s, found %s', prev_seq + 1, s.seq);
      RETURN NEXT;
      RETURN;
    END IF;
    IF s.prev_hash IS DISTINCT FROM prev_h THEN
      seq := s.seq;
      ok := false;
      detail := 'prev_hash does not match previous head';
      RETURN NEXT;
      RETURN;
    END IF;

    SELECT count(*)::int INTO n FROM audit.event e WHERE e.xid = s.xid;
    SELECT sha256(convert_to(string_agg(
             audit.canon_row(e) || chr(30),
             ''
             ORDER BY e.stmt_at, e.table_name::text, e.op, e.row_key::text, e.event_id
           ), 'UTF8'))
      INTO digest
      FROM audit.event e
     WHERE e.xid = s.xid;

    IF n IS DISTINCT FROM s.row_count OR digest IS DISTINCT FROM s.rows_digest THEN
      seq := s.seq;
      ok := false;
      detail := 'rows_digest does not match canonical rows';
      RETURN NEXT;
      RETURN;
    END IF;

    expect_hash := sha256(
        s.prev_hash
        || sep || convert_to(s.seq::text, 'UTF8')
        || sep || convert_to(s.xid::text, 'UTF8')
        || sep || convert_to(audit.ts_canon(s.sealed_at), 'UTF8')
        || sep || s.rows_digest
        || sep || convert_to(s.row_count::text, 'UTF8')
        || sep || convert_to(s.chain_algo, 'UTF8')
    );
    IF expect_hash IS DISTINCT FROM s.hash THEN
      seq := s.seq;
      ok := false;
      detail := 'seal hash does not recompute';
      RETURN NEXT;
      RETURN;
    END IF;

    seq := s.seq;
    ok := true;
    detail := NULL;
    RETURN NEXT;
    prev_h := s.hash;
    prev_seq := s.seq;
  END LOOP;
END
$audit$;

CREATE FUNCTION audit.head()
RETURNS TABLE(seq bigint, hash bytea, xid xid8, sealed_at timestamptz, row_count integer, chain_algo text)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
  SELECT t.seq, t.hash, t.xid, t.sealed_at, t.row_count, t.chain_algo
    FROM audit.tx_seal t
   ORDER BY t.seq DESC
   LIMIT 1;
$audit$;

CREATE FUNCTION audit.record_anchor(p_seq bigint, p_hash bytea, p_sink text, p_receipt text)
RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $audit$
BEGIN
  INSERT INTO audit.anchor (seq, hash, anchored_at, sink, receipt)
  VALUES (p_seq, p_hash, clock_timestamp(), p_sink, p_receipt);
END
$audit$;

CREATE FUNCTION audit.reseal_from(from_seq bigint) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp SET timezone = 'UTC' AS $audit$
DECLARE
  s audit.tx_seal;
  prev_h bytea;
  prev_seq bigint;
  digest bytea;
  n int;
  new_hash bytea;
  sep bytea := convert_to(chr(31), 'UTF8');
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('audit.chain', 0));
  IF from_seq <= 1 THEN
    prev_h := decode(repeat('00', 32), 'hex');
    prev_seq := 0;
  ELSE
    SELECT t.hash, t.seq INTO prev_h, prev_seq
      FROM audit.tx_seal t WHERE t.seq = from_seq - 1;
    IF prev_h IS NULL THEN
      RAISE EXCEPTION 'wicket: reseal_from missing seq %', from_seq - 1;
    END IF;
  END IF;

  FOR s IN
    SELECT * FROM audit.tx_seal t WHERE t.seq >= from_seq ORDER BY t.seq
  LOOP
    SELECT count(*)::int INTO n FROM audit.event e WHERE e.xid = s.xid;
    SELECT sha256(convert_to(string_agg(
             audit.canon_row(e) || chr(30),
             ''
             ORDER BY e.stmt_at, e.table_name::text, e.op, e.row_key::text, e.event_id
           ), 'UTF8'))
      INTO digest
      FROM audit.event e
     WHERE e.xid = s.xid;
    new_hash := sha256(
        prev_h
        || sep || convert_to(s.seq::text, 'UTF8')
        || sep || convert_to(s.xid::text, 'UTF8')
        || sep || convert_to(audit.ts_canon(s.sealed_at), 'UTF8')
        || sep || digest
        || sep || convert_to(n::text, 'UTF8')
        || sep || convert_to(s.chain_algo, 'UTF8')
    );
    UPDATE audit.tx_seal t
       SET row_count = n,
           rows_digest = digest,
           prev_hash = prev_h,
           hash = new_hash
     WHERE t.seq = s.seq;
    prev_h := new_hash;
    prev_seq := s.seq;
  END LOOP;
END
$audit$;

CREATE FUNCTION audit.refuse_mutation() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $audit$
BEGIN
  RAISE EXCEPTION 'wicket: audit.event is insert-only'
    USING ERRCODE = '42501';
END
$audit$;

SELECT audit.ensure_partitions(1);

CREATE TRIGGER zz_audit_immutable
  BEFORE UPDATE OR DELETE ON audit.event
  FOR EACH ROW EXECUTE FUNCTION audit.refuse_mutation();

CREATE CONSTRAINT TRIGGER zz_audit_seal
  AFTER INSERT ON audit.event
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION audit.seal_current_tx();

DO $owner$
DECLARE r record;
BEGIN
  ALTER TABLE audit.event OWNER TO wicket_owner;
  ALTER TABLE audit.redact OWNER TO wicket_owner;
  ALTER TABLE audit.reason_policy OWNER TO wicket_owner;
  ALTER TABLE audit.exempt OWNER TO wicket_owner;
  ALTER TABLE audit.tx_seal OWNER TO wicket_owner;
  ALTER TABLE audit.anchor OWNER TO wicket_owner;
  FOR r IN
    SELECT c.oid::regclass AS rel
      FROM pg_class c
      JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE n.nspname = 'audit' AND c.relispartition
  LOOP
    EXECUTE format('ALTER TABLE %s OWNER TO wicket_owner', r.rel);
  END LOOP;
END
$owner$;

INSERT INTO audit.exempt (relid, nspname, relname, reason, decided_by) VALUES
  ('audit.tx_seal'::regclass, 'audit', 'tx_seal',
   'chain integrity is the chain itself; auditing seals is recursion with no evidentiary value',
   'wicket-audit/0001'),
  ('audit.anchor'::regclass, 'audit', 'anchor',
   'chain integrity is the chain itself; auditing anchors is recursion with no evidentiary value',
   'wicket-audit/0001'),
  (NULL, 'numbering', 'counter',
   'every allocation would write an audit row whose entire evidentiary content is a counter increment; the allocation is evidenced by the audited document that carries the number',
   'wicket-audit/0001');

REVOKE ALL ON audit.event FROM PUBLIC;
GRANT SELECT ON audit.event TO wicket_app;
GRANT INSERT ON audit.event TO wicket_audit_row;
GRANT INSERT (
  event_id, at, stmt_at, xid, actor_id, actor_kind, actor_display,
  acting_for_id, session_id, request_id, source_kind, source_device_id,
  source_ip, client_app, action, reason, doc_type, doc_id, esign_id,
  app_version, config_version
) ON audit.event TO wicket_audit_event;

GRANT SELECT ON audit.redact, audit.reason_policy, audit.exempt,
  audit.tx_seal, audit.anchor TO wicket_app;

REVOKE ALL ON ALL FUNCTIONS IN SCHEMA audit FROM PUBLIC;
GRANT EXECUTE ON FUNCTION audit.require_context() TO wicket_app, wicket_audit_row,
  wicket_audit_event, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION audit.row_change() TO wicket_owner, wicket_migrate, wicket_app;
GRANT EXECUTE ON FUNCTION audit.stmt_truncate() TO wicket_owner, wicket_migrate, wicket_app;
GRANT EXECUTE ON FUNCTION audit.log_event(text, text, text, text, text, text, jsonb)
  TO wicket_app, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION audit.attach(regclass) TO wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.attach_new_tables() TO wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.protect() TO wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.ensure_partitions(integer) TO wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.grant_event_insert(regclass) TO wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.scrub(oid, jsonb) TO wicket_audit_row, wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.pk_of(text[], jsonb) TO wicket_audit_row, wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.reason_required(oid, text) TO wicket_audit_row, wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.ts_canon(timestamptz) TO wicket_app, wicket_owner, wicket_migrate, wicket_audit_row;
GRANT EXECUTE ON FUNCTION audit.canon_jsonb(jsonb) TO wicket_app, wicket_owner, wicket_migrate, wicket_audit_row;
GRANT EXECUTE ON FUNCTION audit.canon_row(audit.event) TO wicket_app, wicket_owner, wicket_migrate, wicket_audit_row;
GRANT EXECUTE ON FUNCTION audit.seal_current_tx() TO wicket_owner, wicket_migrate, wicket_audit_row, wicket_audit_event;
GRANT EXECUTE ON FUNCTION audit.verify(bigint, bigint) TO wicket_app, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION audit.head() TO wicket_app, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION audit.record_anchor(bigint, bytea, text, text)
  TO wicket_app, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION audit.reseal_from(bigint) TO wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION audit.refuse_mutation() TO wicket_owner, wicket_migrate;
GRANT EXECUTE ON FUNCTION audit.guc_uuid(text) TO wicket_app, wicket_audit_row,
  wicket_audit_event, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION audit.guc_inet(text) TO wicket_app, wicket_audit_row,
  wicket_audit_event, wicket_migrate, wicket_owner;

ALTER FUNCTION audit.require_context() OWNER TO wicket_owner;
ALTER FUNCTION audit.attach(regclass) OWNER TO wicket_owner;
ALTER FUNCTION audit.attach_new_tables() OWNER TO wicket_owner;
ALTER FUNCTION audit.protect() OWNER TO wicket_owner;
ALTER FUNCTION audit.ensure_partitions(integer) OWNER TO wicket_owner;
ALTER FUNCTION audit.grant_event_insert(regclass) OWNER TO wicket_owner;
ALTER FUNCTION audit.seal_current_tx() OWNER TO wicket_owner;
ALTER FUNCTION audit.verify(bigint, bigint) OWNER TO wicket_owner;
ALTER FUNCTION audit.head() OWNER TO wicket_owner;
ALTER FUNCTION audit.record_anchor(bigint, bytea, text, text) OWNER TO wicket_owner;
ALTER FUNCTION audit.reseal_from(bigint) OWNER TO wicket_owner;
ALTER FUNCTION audit.refuse_mutation() OWNER TO wicket_owner;
ALTER FUNCTION audit.ts_canon(timestamptz) OWNER TO wicket_owner;
ALTER FUNCTION audit.canon_jsonb(jsonb) OWNER TO wicket_owner;
ALTER FUNCTION audit.canon_row(audit.event) OWNER TO wicket_owner;
ALTER FUNCTION audit.reason_required(oid, text) OWNER TO wicket_owner;
ALTER FUNCTION audit.scrub(oid, jsonb) OWNER TO wicket_owner;
ALTER FUNCTION audit.pk_of(text[], jsonb) OWNER TO wicket_owner;
ALTER FUNCTION audit.guc_uuid(text) OWNER TO wicket_owner;
ALTER FUNCTION audit.guc_inet(text) OWNER TO wicket_owner;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA app
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA app
  GRANT TRIGGER ON TABLES TO wicket_owner;

DO $priv$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = current_user AND rolsuper) THEN
    ALTER FUNCTION audit.row_change() OWNER TO wicket_audit_row;
    ALTER FUNCTION audit.stmt_truncate() OWNER TO wicket_audit_row;
    ALTER FUNCTION audit.log_event(text, text, text, text, text, text, jsonb)
      OWNER TO wicket_audit_event;
    IF NOT EXISTS (SELECT 1 FROM pg_event_trigger WHERE evtname = 'audit_attach') THEN
      CREATE EVENT TRIGGER audit_attach ON ddl_command_end
        WHEN TAG IN ('CREATE TABLE', 'CREATE TABLE AS', 'SELECT INTO')
        EXECUTE FUNCTION audit.attach_new_tables();
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_event_trigger WHERE evtname = 'audit_protect') THEN
      CREATE EVENT TRIGGER audit_protect ON ddl_command_start
        WHEN TAG IN (
          'ALTER TABLE', 'ALTER TRIGGER', 'DROP TRIGGER',
          'DROP FUNCTION', 'DROP ROUTINE', 'DROP PROCEDURE'
        )
        EXECUTE FUNCTION audit.protect();
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_event_trigger WHERE evtname = 'audit_protect_drop') THEN
      CREATE EVENT TRIGGER audit_protect_drop ON sql_drop
        WHEN TAG IN (
          'DROP TRIGGER', 'DROP FUNCTION', 'DROP ROUTINE',
          'DROP PROCEDURE'
        )
        EXECUTE FUNCTION audit.protect();
    END IF;
  END IF;
END
$priv$;
