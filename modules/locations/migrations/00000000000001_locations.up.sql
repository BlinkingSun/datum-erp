-- 0001_locations: site and location master (app schema `locations`).
-- Schema class app (CONTRACT §8a): no DELETE, no ON DELETE CASCADE.
-- Created by wicket_migrate so CREATE TABLE can fire audit.attach_new_tables;
-- tables are then owned by wicket_owner. Reversible.

SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'locations.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS locations AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA locations FROM PUBLIC;
GRANT USAGE ON SCHEMA locations TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA locations TO wicket_migrate, wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('locations', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA locations
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA locations
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA locations
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

CREATE TABLE locations.site (
  id       uuid PRIMARY KEY,
  code     text NOT NULL UNIQUE CHECK (code ~ '^[A-Z0-9-]+$'),
  name     text NOT NULL,
  version  bigint NOT NULL DEFAULT 1 CHECK (version >= 1)
);
ALTER TABLE locations.site OWNER TO wicket_owner;

CREATE TABLE locations.location (
  id              uuid PRIMARY KEY,
  code            text NOT NULL UNIQUE CHECK (code ~ '^[A-Z0-9-]+$'),
  name            text NOT NULL,
  site_id         uuid NOT NULL REFERENCES locations.site (id),
  parent_id       uuid REFERENCES locations.location (id),
  kind            text NOT NULL CHECK (kind IN ('warehouse','area','bin','wip','osp','virtual')),
  boundary_class  ledger.boundary,
  status          text NOT NULL DEFAULT 'active' CHECK (status IN ('active','inactive')),
  work_order_id   uuid,
  version         bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  CONSTRAINT boundary_or_real CHECK (
    (boundary_class IS NULL AND kind <> 'virtual')
    OR (boundary_class IS NOT NULL AND kind = 'virtual')
  ),
  CONSTRAINT wip_work_order CHECK (
    kind <> 'wip' OR work_order_id IS NOT NULL
  )
);
ALTER TABLE locations.location OWNER TO wicket_owner;

CREATE UNIQUE INDEX location_one_per_boundary
  ON locations.location (boundary_class)
  WHERE boundary_class IS NOT NULL;

CREATE UNIQUE INDEX location_one_wip_per_work_order
  ON locations.location (work_order_id)
  WHERE work_order_id IS NOT NULL;

SELECT audit.attach('locations.site'::regclass);
SELECT audit.attach('locations.location'::regclass);

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA locations TO wicket_app;
REVOKE DELETE ON ALL TABLES IN SCHEMA locations FROM PUBLIC, wicket_app;

ALTER SCHEMA locations OWNER TO wicket_owner;
