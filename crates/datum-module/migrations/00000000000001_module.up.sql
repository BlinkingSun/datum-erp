-- 0001_module: module registry (schema class app). Reversible.
-- Tables owned by datum_owner. DML at boot goes through datum_db::Tx.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'module.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS module AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA module FROM PUBLIC;
GRANT USAGE ON SCHEMA module TO datum_app;
GRANT USAGE, CREATE ON SCHEMA module TO datum_migrate, datum_owner;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA module
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA module
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA module
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

INSERT INTO datum.schema_class (nspname, class)
VALUES ('module', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

CREATE TABLE module.installed (
    id                   text        PRIMARY KEY,
    version              text        NOT NULL,
    regulated            boolean     NOT NULL,
    installed_at         timestamptz NOT NULL DEFAULT now(),
    enabled              boolean     NOT NULL,
    enabled_changed_at  timestamptz NOT NULL DEFAULT now(),
    manifest_hash       text        NOT NULL,
    CHECK (id = btrim(id) AND id <> ''),
    CHECK (version <> '')
);
ALTER TABLE module.installed OWNER TO datum_owner;

CREATE TABLE module.configuration (
    id             text        PRIMARY KEY,
    profile_id     text        NOT NULL,
    spec_version   text        NOT NULL,
    body           jsonb       NOT NULL,
    content_hash   text        NOT NULL,
    recorded_at    timestamptz NOT NULL DEFAULT now(),
    CHECK (id = btrim(id) AND id <> '')
);
ALTER TABLE module.configuration OWNER TO datum_owner;

CREATE TABLE module.install_log (
    module_id    text        PRIMARY KEY,
    applied_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (module_id = btrim(module_id) AND module_id <> '')
);
ALTER TABLE module.install_log OWNER TO datum_owner;

SELECT audit.attach('module.installed'::regclass);
SELECT audit.attach('module.configuration'::regclass);
SELECT audit.attach('module.install_log'::regclass);

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA module TO datum_app;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA module TO datum_app;

ALTER SCHEMA module OWNER TO datum_owner;
