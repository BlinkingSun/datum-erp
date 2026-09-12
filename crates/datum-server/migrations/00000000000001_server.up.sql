-- 0001_server: audited boot record (class app) and HTTP working state (class transient).
-- Reversible. No DELETE on app tables. No ON DELETE CASCADE.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'server.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS server AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA server FROM PUBLIC;
GRANT USAGE ON SCHEMA server TO datum_app;
GRANT USAGE, CREATE ON SCHEMA server TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('server', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA server
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA server
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA server
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

CREATE TABLE server.boot_record (
  id                      uuid PRIMARY KEY,
  profile_id              text NOT NULL,
  spec_version            text NOT NULL,
  manifest_hash           text NOT NULL,
  bind_addr               text NOT NULL,
  application_version     text NOT NULL,
  configuration_version   text NOT NULL DEFAULT coalesce(nullif(current_setting('datum.config_version', true), ''), ''),
  created_at              timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE server.boot_record OWNER TO datum_owner;
COMMENT ON TABLE server.boot_record IS
  'Audited record of each kernel boot: profile, bind, and configuration-manifest hash.';

SELECT audit.attach('server.boot_record'::regclass);

GRANT SELECT, INSERT, UPDATE ON server.boot_record TO datum_app;
REVOKE DELETE ON server.boot_record FROM PUBLIC, datum_app;

CREATE SCHEMA IF NOT EXISTS server_transient AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA server_transient FROM PUBLIC;
GRANT USAGE ON SCHEMA server_transient TO datum_app;
GRANT USAGE, CREATE ON SCHEMA server_transient TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('server_transient', 'transient')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA server_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA server_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;

CREATE TABLE server_transient.idempotency (
  key           uuid PRIMARY KEY,
  body_hash     text NOT NULL,
  status        integer NOT NULL,
  response      jsonb NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE server_transient.idempotency OWNER TO datum_owner;

CREATE TABLE server_transient.http_session (
  id            uuid PRIMARY KEY,
  principal_id  uuid NOT NULL,
  display_name  text NOT NULL,
  csrf          text NOT NULL,
  permissions   text[] NOT NULL DEFAULT '{}',
  expires_at    timestamptz NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE server_transient.http_session OWNER TO datum_owner;

GRANT SELECT, INSERT, UPDATE, DELETE ON server_transient.idempotency TO datum_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON server_transient.http_session TO datum_app;
