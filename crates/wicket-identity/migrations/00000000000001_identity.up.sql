-- 0001_identity: principals, credentials, RBAC, sessions, audit FK.
-- Tables in schema identity (class app) are owned by wicket_owner.
-- Sessions live in schema transient (DELETE allowed; invariant 16).
-- Username is text plus unique index on lower(username): CREATE EXTENSION
-- citext is superuser-only and migrations run as wicket_migrate.
--
-- sqlx (and wicket_db::migrate::run via raw_sql simple-query) applies this file
-- as one implicit transaction, so SET LOCAL / set_config(..., is_local) holds
-- for the built-in principal INSERTs. Do not add `-- no-transaction`.

-- Created by wicket_migrate so the session can CREATE TABLE. audit.attach_new_tables
-- is SECURITY DEFINER owned by wicket_owner, so wicket_owner needs TRIGGER on
-- every new table at CREATE TABLE time (the event trigger fires before we can
-- ALTER OWNER). Default privileges below grant that.

-- Audit actor for DML in this file (zz_audit_row / require_context).
-- Well-known id of identity.principal 'migration' (inserted below, before the FK).
SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'identity.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS identity AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA identity FROM PUBLIC;
GRANT USAGE ON SCHEMA identity TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA identity TO wicket_migrate, wicket_owner;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA identity
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA identity
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA identity
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

INSERT INTO wicket.schema_class (nspname, class)
VALUES ('identity', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

CREATE TABLE identity.principal (
    id              uuid        PRIMARY KEY,
    kind            text        NOT NULL CHECK (kind IN ('user', 'service', 'migration')),
    username        text        NOT NULL,
    display_name    text        NOT NULL,
    status          text        NOT NULL CHECK (status IN ('active', 'inactive')),
    created_at      timestamptz NOT NULL DEFAULT now(),
    deactivated_at  timestamptz NULL,
    CHECK (username = btrim(username) AND username <> ''),
    CHECK (display_name <> '')
);
ALTER TABLE identity.principal OWNER TO wicket_owner;
CREATE UNIQUE INDEX principal_username_lower ON identity.principal (lower(username));

CREATE TABLE identity.username_history (
    id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    username      text        NOT NULL,
    principal_id  uuid        NOT NULL REFERENCES identity.principal (id),
    at            timestamptz NOT NULL
);
ALTER TABLE identity.username_history OWNER TO wicket_owner;
CREATE UNIQUE INDEX username_history_username_lower
    ON identity.username_history (lower(username));

CREATE TABLE identity.display_name_history (
    id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    principal_id  uuid        NOT NULL REFERENCES identity.principal (id),
    display_name  text        NOT NULL,
    at            timestamptz NOT NULL
);
ALTER TABLE identity.display_name_history OWNER TO wicket_owner;
CREATE INDEX display_name_history_lookup
    ON identity.display_name_history (principal_id, at DESC);

CREATE TABLE identity.login_credential (
    principal_id     uuid        PRIMARY KEY REFERENCES identity.principal (id),
    hash             text        NOT NULL,
    m                integer     NOT NULL,
    t                integer     NOT NULL,
    p                integer     NOT NULL,
    rotated_at       timestamptz NOT NULL DEFAULT now(),
    failed_attempts  integer     NOT NULL DEFAULT 0,
    locked_until     timestamptz NULL,
    last_failed_at   timestamptz NULL
);
ALTER TABLE identity.login_credential OWNER TO wicket_owner;

CREATE TABLE identity.signing_credential (
    principal_id    uuid        PRIMARY KEY REFERENCES identity.principal (id),
    hash            text        NOT NULL,
    established_at  timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE identity.signing_credential OWNER TO wicket_owner;

CREATE TABLE identity.credential_reset (
    id             uuid        PRIMARY KEY,
    principal_id   uuid        NOT NULL REFERENCES identity.principal (id),
    requested_by   uuid        NOT NULL REFERENCES identity.principal (id),
    kind           text        NOT NULL CHECK (kind IN ('login', 'signing')),
    token_hash     text        NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    expires_at     timestamptz NOT NULL,
    completed_at   timestamptz NULL,
    completed_by   uuid        NULL REFERENCES identity.principal (id)
);
ALTER TABLE identity.credential_reset OWNER TO wicket_owner;

CREATE TABLE identity.role (
    id    uuid PRIMARY KEY,
    name  text NOT NULL UNIQUE
);
ALTER TABLE identity.role OWNER TO wicket_owner;

CREATE TABLE identity.role_permission (
    role_id         uuid NOT NULL REFERENCES identity.role (id),
    permission_key  text NOT NULL,
    PRIMARY KEY (role_id, permission_key)
);
ALTER TABLE identity.role_permission OWNER TO wicket_owner;

CREATE TABLE identity.principal_role (
    principal_id  uuid NOT NULL REFERENCES identity.principal (id),
    role_id       uuid NOT NULL REFERENCES identity.role (id),
    PRIMARY KEY (principal_id, role_id)
);
ALTER TABLE identity.principal_role OWNER TO wicket_owner;

CREATE TABLE transient.session (
    id             uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    principal_id   uuid        NOT NULL REFERENCES identity.principal (id),
    created_at     timestamptz NOT NULL DEFAULT now(),
    last_seen_at   timestamptz NOT NULL DEFAULT now(),
    expires_at     timestamptz NOT NULL,
    device         text        NULL,
    ip             inet        NULL
);
ALTER TABLE transient.session OWNER TO wicket_owner;

CREATE FUNCTION identity.refuse_username_reuse() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  IF TG_OP = 'UPDATE' AND lower(OLD.username) = lower(NEW.username) THEN
    RETURN NEW;
  END IF;
  IF EXISTS (
    SELECT 1 FROM identity.username_history h
     WHERE lower(h.username) = lower(NEW.username)
  ) THEN
    RAISE EXCEPTION 'wicket: username reused'
      USING ERRCODE = '23505',
            CONSTRAINT = 'username_history_username_lower',
            SCHEMA = 'identity',
            TABLE = 'username_history';
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION identity.refuse_username_reuse() OWNER TO wicket_owner;

CREATE FUNCTION identity.record_username() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  IF TG_OP = 'UPDATE' AND lower(OLD.username) = lower(NEW.username) THEN
    RETURN NEW;
  END IF;
  INSERT INTO identity.username_history (username, principal_id, at)
  VALUES (NEW.username, NEW.id, clock_timestamp());
  RETURN NEW;
END
$fn$;
ALTER FUNCTION identity.record_username() OWNER TO wicket_owner;

CREATE FUNCTION identity.record_display_name() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  IF TG_OP = 'UPDATE' AND OLD.display_name IS NOT DISTINCT FROM NEW.display_name THEN
    RETURN NEW;
  END IF;
  INSERT INTO identity.display_name_history (principal_id, display_name, at)
  VALUES (NEW.id, NEW.display_name, clock_timestamp());
  RETURN NEW;
END
$fn$;
ALTER FUNCTION identity.record_display_name() OWNER TO wicket_owner;

CREATE TRIGGER principal_refuse_username_reuse
  BEFORE INSERT OR UPDATE OF username ON identity.principal
  FOR EACH ROW EXECUTE FUNCTION identity.refuse_username_reuse();

CREATE TRIGGER principal_record_username
  AFTER INSERT OR UPDATE OF username ON identity.principal
  FOR EACH ROW EXECUTE FUNCTION identity.record_username();

CREATE TRIGGER principal_record_display_name
  AFTER INSERT OR UPDATE OF display_name ON identity.principal
  FOR EACH ROW EXECUTE FUNCTION identity.record_display_name();

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA identity TO wicket_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE transient.session TO wicket_app;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA identity TO wicket_app;

INSERT INTO audit.redact (relid, column_name, reason, decided_by)
VALUES
  ('identity.login_credential'::regclass,    'hash',       'credential', 'wicket-identity'),
  ('identity.signing_credential'::regclass,  'hash',       'credential', 'wicket-identity'),
  ('identity.credential_reset'::regclass,    'token_hash', 'credential', 'wicket-identity');

-- Built-in principals (SPEC ADDENDUM 1 item 1). Inserted before event_actor_fk
-- so a down-then-up (audit rows from the previous up still present) validates.
-- `migration` first: zz_audit_row stamps actor_id with the GUC set above.
INSERT INTO identity.principal
    (id, kind, username, display_name, status, created_at)
VALUES
    ('00000000-0000-4000-8000-000000000002', 'migration', 'migration', 'migration', 'active', now());
INSERT INTO identity.principal
    (id, kind, username, display_name, status, created_at)
VALUES
    ('00000000-0000-4000-8000-000000000001', 'service',   'system',    'system',    'active', now());

-- zz_audit_seal is DEFERRABLE INITIALLY DEFERRED on audit.event. Those pending
-- events must fire before ALTER TABLE on a partition (else SQLSTATE 55006).
SET CONSTRAINTS ALL IMMEDIATE;

-- PostgreSQL 17 refuses NOT VALID foreign keys on a partitioned table
-- (audit.event is RANGE-partitioned). SPEC ADDENDUM 1 item 2: the constraint
-- is added already-valid. Existing rows (the built-in inserts above) validate.
ALTER TABLE audit.event
  ADD CONSTRAINT event_actor_fk
  FOREIGN KEY (actor_id) REFERENCES identity.principal (id);

ALTER SCHEMA identity OWNER TO wicket_owner;
