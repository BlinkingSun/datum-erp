-- 0001_inventory: documents and lines in schema-class `app`.
-- Working state (idempotency) in `inventory_transient` (class `transient`).
-- No stored running-quantity table. Lots and serials are uuid entity references, never text.
-- Reversible. Audited via audit.attach. No ON DELETE CASCADE. No DELETE on app tables.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'inventory.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS inventory AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA inventory FROM PUBLIC;
GRANT USAGE ON SCHEMA inventory TO datum_app;
GRANT USAGE, CREATE ON SCHEMA inventory TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('inventory', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA inventory
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA inventory
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA inventory
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

CREATE TABLE inventory.document (
  id                      uuid PRIMARY KEY,
  kind                    text NOT NULL CHECK (kind IN ('receipt','issue','move','adjustment','count')),
  status                  text NOT NULL CHECK (status IN ('draft','posted','voided')),
  reference               text,
  posted_group_id         uuid,
  version                 bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  application_version     text NOT NULL,
  configuration_version   text NOT NULL DEFAULT ''
);
ALTER TABLE inventory.document OWNER TO datum_owner;

CREATE TABLE inventory.document_line (
  id                      uuid PRIMARY KEY,
  document_id             uuid NOT NULL REFERENCES inventory.document (id),
  item_id                 uuid NOT NULL,
  lot_id                  uuid,
  serial_id               uuid,
  from_location_id        uuid,
  to_location_id          uuid,
  entered_amount          numeric(24,8) NOT NULL,
  entered_uom_id          bigint NOT NULL,
  entered_dimension       text NOT NULL,
  canonical_amount        numeric(24,8) NOT NULL,
  canonical_uom_id        bigint NOT NULL,
  canonical_dimension     text NOT NULL,
  conversion_factor       numeric(24,18) NOT NULL,
  reason_code             text,
  package_id              uuid,
  application_version     text NOT NULL,
  configuration_version   text NOT NULL DEFAULT '',
  CONSTRAINT document_line_dimension_known CHECK (
    entered_dimension IN ('Count','Length','Mass','Time','Volume','Area')
    AND canonical_dimension IN ('Count','Length','Mass','Time','Volume','Area')
  )
);
ALTER TABLE inventory.document_line OWNER TO datum_owner;
CREATE INDEX document_line_document_idx ON inventory.document_line (document_id);
CREATE INDEX document_line_item_idx ON inventory.document_line (item_id);
CREATE INDEX document_line_lot_idx ON inventory.document_line (lot_id);

SELECT audit.attach('inventory.document'::regclass);
SELECT audit.attach('inventory.document_line'::regclass);

GRANT SELECT, INSERT, UPDATE ON inventory.document TO datum_app;
GRANT SELECT, INSERT, UPDATE ON inventory.document_line TO datum_app;
REVOKE DELETE ON inventory.document, inventory.document_line FROM PUBLIC, datum_app;

CREATE SCHEMA IF NOT EXISTS inventory_transient AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA inventory_transient FROM PUBLIC;
GRANT USAGE ON SCHEMA inventory_transient TO datum_app;
GRANT USAGE, CREATE ON SCHEMA inventory_transient TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('inventory_transient', 'transient')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA inventory_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA inventory_transient
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA inventory_transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;

CREATE TABLE inventory_transient.idempotency (
  key           uuid PRIMARY KEY,
  body_hash     text NOT NULL,
  document_id   uuid NOT NULL,
  recorded_at   timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE inventory_transient.idempotency OWNER TO datum_owner;

GRANT SELECT, INSERT, UPDATE, DELETE ON inventory_transient.idempotency TO datum_app;

ALTER SCHEMA inventory OWNER TO datum_owner;
ALTER SCHEMA inventory_transient OWNER TO datum_owner;
