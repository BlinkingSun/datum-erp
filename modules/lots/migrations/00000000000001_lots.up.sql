-- 0001_lots: lot/serial identity, package hierarchy, expiry precision, UDI attachment.
-- PostgreSQL schema `lots` is schema-class `app` (records with history: no DELETE).
-- Reversible. Audited via audit.attach. No ON DELETE CASCADE.
--
-- Schema is created by wicket_migrate so CREATE TABLE can succeed; default
-- privileges grant TRIGGER to wicket_owner so audit.attach_new_tables (SECURITY
-- DEFINER, owner wicket_owner) can attach at CREATE TABLE time. Tables are then
-- ALTER OWNER TO wicket_owner (NOLOGIN).

SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'lots.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS lots AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA lots FROM PUBLIC;
GRANT USAGE ON SCHEMA lots TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA lots TO wicket_migrate, wicket_owner;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA lots
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA lots
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA lots
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('lots', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

CREATE TABLE lots.lot (
  id                      uuid           PRIMARY KEY,
  item_id                 uuid           NOT NULL,
  number                  text           NOT NULL,
  supplier_lot            text               NULL,
  heat_or_source_ref      text               NULL,
  received_at             timestamptz    NOT NULL DEFAULT now(),
  expiry_date             date               NULL,
  expiry_precision        text               NULL,
  cert_ref                text               NULL,
  status                  text           NOT NULL,
  udi_device_identifier    text               NULL,
  version                 bigint         NOT NULL DEFAULT 1,
  application_version     text           NOT NULL,
  configuration_version   text           NOT NULL DEFAULT '',
  CONSTRAINT lot_number_charset
    CHECK (number ~ '^[0-9A-Z-]{1,20}$'),
  CONSTRAINT lot_status_known
    CHECK (status IN ('quarantine', 'available', 'hold', 'rejected')),
  CONSTRAINT lot_expiry_precision_pair
    CHECK (
      (expiry_date IS NULL AND expiry_precision IS NULL)
      OR (expiry_date IS NOT NULL AND expiry_precision IN ('day', 'month', 'year'))
    ),
  CONSTRAINT lot_expiry_month_first
    CHECK (
      expiry_precision IS DISTINCT FROM 'month'
      OR EXTRACT(DAY FROM expiry_date) = 1
    ),
  CONSTRAINT lot_expiry_year_jan1
    CHECK (
      expiry_precision IS DISTINCT FROM 'year'
      OR (
        EXTRACT(MONTH FROM expiry_date) = 1
        AND EXTRACT(DAY FROM expiry_date) = 1
      )
    )
);
ALTER TABLE lots.lot OWNER TO wicket_owner;
CREATE UNIQUE INDEX lot_number_unique ON lots.lot (number);

CREATE TABLE lots.serial (
  id                         uuid           PRIMARY KEY,
  lot_id                     uuid           NOT NULL REFERENCES lots.lot (id),
  number                     text           NOT NULL,
  status                     text           NOT NULL,
  udi_production_identifier   text               NULL,
  version                    bigint         NOT NULL DEFAULT 1,
  application_version        text           NOT NULL,
  configuration_version      text           NOT NULL DEFAULT '',
  CONSTRAINT serial_number_charset
    CHECK (number ~ '^[0-9A-Z-]{1,20}$'),
  CONSTRAINT serial_status_known
    CHECK (status IN ('quarantine', 'available', 'hold', 'rejected'))
);
ALTER TABLE lots.serial OWNER TO wicket_owner;
CREATE UNIQUE INDEX serial_lot_number_unique ON lots.serial (lot_id, number);

CREATE TABLE lots.package (
  id                      uuid           PRIMARY KEY,
  lot_id                  uuid           NOT NULL REFERENCES lots.lot (id),
  parent_id               uuid               NULL REFERENCES lots.package (id),
  level                   text           NOT NULL,
  contained_amount        numeric(24, 8) NOT NULL,
  contained_unit          bigint         NOT NULL,
  contained_dimension     text           NOT NULL,
  label_ref               text               NULL,
  application_version     text           NOT NULL,
  configuration_version    text           NOT NULL DEFAULT '',
  CONSTRAINT package_level_known
    CHECK (level IN ('each', 'inner', 'case', 'pallet')),
  CONSTRAINT package_dimension_known
    CHECK (contained_dimension IN ('Count', 'Length', 'Mass', 'Time', 'Volume', 'Area')),
  CONSTRAINT package_quantity_non_negative
    CHECK (contained_amount >= 0)
);
ALTER TABLE lots.package OWNER TO wicket_owner;
CREATE INDEX package_lot_idx ON lots.package (lot_id);
CREATE INDEX package_parent_idx ON lots.package (parent_id);

CREATE TABLE lots.status_history (
  id                      uuid           PRIMARY KEY,
  lot_id                  uuid               NULL REFERENCES lots.lot (id),
  serial_id               uuid               NULL REFERENCES lots.serial (id),
  from_status             text               NULL,
  to_status               text           NOT NULL,
  reason                  text           NOT NULL,
  recorded_at             timestamptz    NOT NULL DEFAULT now(),
  application_version     text           NOT NULL,
  configuration_version    text           NOT NULL DEFAULT '',
  CONSTRAINT status_history_subject
    CHECK (
      (lot_id IS NOT NULL AND serial_id IS NULL)
      OR (lot_id IS NULL AND serial_id IS NOT NULL)
    ),
  CONSTRAINT status_history_to_known
    CHECK (to_status IN ('quarantine', 'available', 'hold', 'rejected'))
);
ALTER TABLE lots.status_history OWNER TO wicket_owner;

SELECT audit.attach('lots.lot'::regclass);
SELECT audit.attach('lots.serial'::regclass);
SELECT audit.attach('lots.package'::regclass);
SELECT audit.attach('lots.status_history'::regclass);

GRANT SELECT, INSERT, UPDATE ON lots.lot TO wicket_app;
GRANT SELECT, INSERT, UPDATE ON lots.serial TO wicket_app;
GRANT SELECT, INSERT, UPDATE ON lots.package TO wicket_app;
GRANT SELECT, INSERT ON lots.status_history TO wicket_app;

REVOKE DELETE ON lots.lot, lots.serial, lots.package, lots.status_history
  FROM PUBLIC, wicket_app;
REVOKE UPDATE ON lots.status_history FROM PUBLIC, wicket_app;
