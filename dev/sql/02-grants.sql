-- 02-grants.sql — grant pattern per audit-persistence §1.3, PLAN §6b inv 16 (amended),
-- and DECISION D-W1-2. Applied as bootstrap superuser; idempotent.
-- Run inside each of datum_test_template and datum_test.

\set ON_ERROR_STOP on

\connect datum_test_template

-- 02-grants.sql — grant pattern per audit-persistence §1.3, PLAN §6b inv 16 (amended),
-- and DECISION D-W1-2. Applied as bootstrap superuser; idempotent.
--
-- Schema classes:
--   app       = records with history            -> no DELETE
--   transient = working state with no history   -> DELETE allowed
--   audit     = the trail                       -> SELECT only

CREATE SCHEMA IF NOT EXISTS app       AUTHORIZATION datum_migrate;
CREATE SCHEMA IF NOT EXISTS transient AUTHORIZATION datum_migrate;
CREATE SCHEMA IF NOT EXISTS audit     AUTHORIZATION datum_migrate;

REVOKE ALL   ON SCHEMA app, transient, audit FROM PUBLIC;
GRANT  USAGE ON SCHEMA app, transient, audit TO   datum_app;

-- app: records with history. No DELETE. (PLAN §6b inv 16, D-W1-2)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA app
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

-- transient: sessions, idempotency keys, job queue, projection caches. (D-W1-2)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;

-- audit: SELECT and nothing else, for every role. (audit-persistence §1.3)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA audit
  GRANT SELECT ON TABLES TO datum_app;

-- sequences: draw, never set.
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA app, transient
  GRANT USAGE ON SEQUENCES TO datum_app;

-- TRUNCATE appears in no GRANT above, in any schema, and PUBLIC holds nothing.
-- The absence is deliberate: TRUNCATE fires no row trigger, so it would erase
-- history with no audit row. (audit-persistence §1.1, D-W1-2)

-- Production note (D-W1-2): tables may be created by datum_owner; default
-- privileges key on the creating role. Repeat FOR ROLE datum_owner.
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA app
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;              -- PLAN §6b inv 16, D-W1-2
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;       -- D-W1-2
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA audit
  GRANT SELECT ON TABLES TO datum_app;                              -- audit-persistence §1.3
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA app, transient
  GRANT USAGE ON SEQUENCES TO datum_app;                            -- sequences: draw, never set

-- audit-persistence §1.3: writer roles must resolve schema audit.
GRANT USAGE ON SCHEMA audit TO datum_audit_row, datum_audit_event;

-- audit-persistence §1.3: datum_audit_row is the only writer of row-change columns.
-- Default INSERT (tables do not exist yet); column-restricted event INSERT below.
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA audit
  GRANT INSERT ON TABLES TO datum_audit_row;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA audit
  GRANT INSERT ON TABLES TO datum_audit_row;

-- audit-persistence §1.3: column-restricted INSERTs on audit.event, if present.
-- No tables are created here.
DO $$
BEGIN
  IF to_regclass('audit.event') IS NOT NULL THEN
    REVOKE ALL ON audit.event FROM PUBLIC;                            -- audit-persistence §1.3
    GRANT SELECT ON audit.event TO datum_app;                        -- audit-persistence §1.3
    GRANT INSERT ON audit.event TO datum_audit_row;                 -- audit-persistence §1.3
    GRANT INSERT (
      event_id, at, stmt_at, xid, actor_id, actor_kind, actor_display,
      acting_for_id, session_id, request_id, source_kind, source_device_id,
      source_ip, client_app, action, reason, doc_type, doc_id, esign_id
    ) ON audit.event TO datum_audit_event;                            -- audit-persistence §1.3
  END IF;
END $$;

\connect datum_test

-- 02-grants.sql — grant pattern per audit-persistence §1.3, PLAN §6b inv 16 (amended),
-- and DECISION D-W1-2. Applied as bootstrap superuser; idempotent.
--
-- Schema classes:
--   app       = records with history            -> no DELETE
--   transient = working state with no history   -> DELETE allowed
--   audit     = the trail                       -> SELECT only

CREATE SCHEMA IF NOT EXISTS app       AUTHORIZATION datum_migrate;
CREATE SCHEMA IF NOT EXISTS transient AUTHORIZATION datum_migrate;
CREATE SCHEMA IF NOT EXISTS audit     AUTHORIZATION datum_migrate;

REVOKE ALL   ON SCHEMA app, transient, audit FROM PUBLIC;
GRANT  USAGE ON SCHEMA app, transient, audit TO   datum_app;

-- app: records with history. No DELETE. (PLAN §6b inv 16, D-W1-2)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA app
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

-- transient: sessions, idempotency keys, job queue, projection caches. (D-W1-2)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;

-- audit: SELECT and nothing else, for every role. (audit-persistence §1.3)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA audit
  GRANT SELECT ON TABLES TO datum_app;

-- sequences: draw, never set.
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA app, transient
  GRANT USAGE ON SEQUENCES TO datum_app;

-- TRUNCATE appears in no GRANT above, in any schema, and PUBLIC holds nothing.
-- The absence is deliberate: TRUNCATE fires no row trigger, so it would erase
-- history with no audit row. (audit-persistence §1.1, D-W1-2)

-- Production note (D-W1-2): tables may be created by datum_owner; default
-- privileges key on the creating role. Repeat FOR ROLE datum_owner.
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA app
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;              -- PLAN §6b inv 16, D-W1-2
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;       -- D-W1-2
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA audit
  GRANT SELECT ON TABLES TO datum_app;                              -- audit-persistence §1.3
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA app, transient
  GRANT USAGE ON SEQUENCES TO datum_app;                            -- sequences: draw, never set

-- audit-persistence §1.3: writer roles must resolve schema audit.
GRANT USAGE ON SCHEMA audit TO datum_audit_row, datum_audit_event;

-- audit-persistence §1.3: datum_audit_row is the only writer of row-change columns.
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA audit
  GRANT INSERT ON TABLES TO datum_audit_row;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA audit
  GRANT INSERT ON TABLES TO datum_audit_row;

-- audit-persistence §1.3: column-restricted INSERTs on audit.event, if present.
DO $$
BEGIN
  IF to_regclass('audit.event') IS NOT NULL THEN
    REVOKE ALL ON audit.event FROM PUBLIC;                            -- audit-persistence §1.3
    GRANT SELECT ON audit.event TO datum_app;                        -- audit-persistence §1.3
    GRANT INSERT ON audit.event TO datum_audit_row;                 -- audit-persistence §1.3
    GRANT INSERT (
      event_id, at, stmt_at, xid, actor_id, actor_kind, actor_display,
      acting_for_id, session_id, request_id, source_kind, source_device_id,
      source_ip, client_app, action, reason, doc_type, doc_id, esign_id
    ) ON audit.event TO datum_audit_event;                            -- audit-persistence §1.3
  END IF;
END $$;
