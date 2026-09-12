-- 0001_locations: site and location master (app schema `locations`).

CREATE SCHEMA IF NOT EXISTS locations AUTHORIZATION datum_owner;

REVOKE ALL ON SCHEMA locations FROM PUBLIC;
GRANT USAGE ON SCHEMA locations TO datum_app, datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('locations', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA locations
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA locations
  GRANT USAGE ON SEQUENCES TO datum_app;

CREATE TABLE locations.site (
  id       uuid PRIMARY KEY,
  code     text NOT NULL UNIQUE CHECK (code ~ '^[A-Z0-9-]+$'),
  name     text NOT NULL,
  version  bigint NOT NULL DEFAULT 1 CHECK (version >= 1)
);
ALTER TABLE locations.site OWNER TO datum_owner;

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
ALTER TABLE locations.location OWNER TO datum_owner;

CREATE UNIQUE INDEX location_one_per_boundary
  ON locations.location (boundary_class)
  WHERE boundary_class IS NOT NULL;

CREATE UNIQUE INDEX location_one_wip_per_work_order
  ON locations.location (work_order_id)
  WHERE work_order_id IS NOT NULL;

SELECT audit.attach('locations.site'::regclass);
SELECT audit.attach('locations.location'::regclass);

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA locations TO datum_app;
REVOKE DELETE ON ALL TABLES IN SCHEMA locations FROM PUBLIC, datum_app;
