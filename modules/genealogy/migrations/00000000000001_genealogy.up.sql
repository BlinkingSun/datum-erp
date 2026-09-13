-- 0001_genealogy: app schema (R-2s-1) plus a rebuildable transient cache.
-- The cache is never authoritative; the ledger graph is. DELETE is allowed
-- (class transient). Lots and serials are kernel entities, never text.
-- Reversible. DML requires Tx::begin (zz_audit_row / require_context).

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'genealogy.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS genealogy AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA genealogy FROM PUBLIC;
GRANT USAGE ON SCHEMA genealogy TO datum_app;
GRANT USAGE, CREATE ON SCHEMA genealogy TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('genealogy', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA genealogy
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA genealogy
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA genealogy
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

CREATE SCHEMA IF NOT EXISTS genealogy_transient AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA genealogy_transient FROM PUBLIC;
GRANT USAGE ON SCHEMA genealogy_transient TO datum_app;
GRANT USAGE, CREATE ON SCHEMA genealogy_transient TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('genealogy_transient', 'transient')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA genealogy_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA genealogy_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;

CREATE TABLE genealogy_transient.trace_cache (
  key           text PRIMARY KEY,
  computed_at   timestamptz NOT NULL DEFAULT now(),
  tree          jsonb NOT NULL
);
ALTER TABLE genealogy_transient.trace_cache OWNER TO datum_owner;
COMMENT ON TABLE genealogy_transient.trace_cache IS
  'Rebuildable genealogy tree cache. Invalidated by inventory/production events.';

SELECT audit.attach('genealogy_transient.trace_cache'::regclass);

GRANT SELECT, INSERT, UPDATE, DELETE ON genealogy_transient.trace_cache TO datum_app;

ALTER SCHEMA genealogy OWNER TO datum_owner;
ALTER SCHEMA genealogy_transient OWNER TO datum_owner;
