-- 0001_genealogy: app schema (R-2s-1) plus a rebuildable transient cache.
-- The cache is never authoritative; the ledger graph is. DELETE is allowed
-- (class transient). Lots and serials are kernel entities, never text.
-- Reversible. DML requires Tx::begin (zz_audit_row / require_context).

SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'genealogy.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS genealogy AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA genealogy FROM PUBLIC;
GRANT USAGE ON SCHEMA genealogy TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA genealogy TO wicket_migrate, wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('genealogy', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA genealogy
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA genealogy
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA genealogy
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

CREATE SCHEMA IF NOT EXISTS genealogy_transient AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA genealogy_transient FROM PUBLIC;
GRANT USAGE ON SCHEMA genealogy_transient TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA genealogy_transient TO wicket_migrate, wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('genealogy_transient', 'transient')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA genealogy_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA genealogy_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO wicket_app;

CREATE TABLE genealogy_transient.trace_cache (
  key           text PRIMARY KEY,
  computed_at   timestamptz NOT NULL DEFAULT now(),
  tree          jsonb NOT NULL
);
ALTER TABLE genealogy_transient.trace_cache OWNER TO wicket_owner;
COMMENT ON TABLE genealogy_transient.trace_cache IS
  'Rebuildable genealogy tree cache. Invalidated by inventory/production events.';

SELECT audit.attach('genealogy_transient.trace_cache'::regclass);

GRANT SELECT, INSERT, UPDATE, DELETE ON genealogy_transient.trace_cache TO wicket_app;

ALTER SCHEMA genealogy OWNER TO wicket_owner;
ALTER SCHEMA genealogy_transient OWNER TO wicket_owner;
