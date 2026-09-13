-- 0001_items: part master in schema-class `app`. Reversible. Owned by wicket_owner.
-- No DELETE, no ON DELETE CASCADE. Audit attached at CREATE TABLE.
-- Schema is created as wicket_migrate so default TRIGGER privileges exist
-- before ALTER OWNER (event trigger fires at CREATE TABLE).

SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'items.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS items AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA items FROM PUBLIC;
GRANT USAGE ON SCHEMA items TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA items TO wicket_migrate, wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('items', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA items
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA items
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA items
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

CREATE TABLE items.item (
  id                      uuid PRIMARY KEY,
  number                  text NOT NULL,
  revision                text NOT NULL,
  description             text NOT NULL,
  kind                    text NOT NULL CHECK (kind IN ('make', 'buy', 'service', 'phantom')),
  stock_uom_id            bigint NOT NULL,
  stock_scale             smallint NOT NULL CHECK (stock_scale BETWEEN 0 AND 8),
  residual_tolerance      numeric(24,8) NOT NULL CHECK (residual_tolerance >= 0),
  cost_method             text NOT NULL CHECK (cost_method IN ('FIFO', 'MOVING_AVG', 'STANDARD')),
  status                  text NOT NULL CHECK (status IN ('draft', 'released', 'obsolete')),
  version                 bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  application_version     text NOT NULL DEFAULT coalesce(nullif(current_setting('wicket.app_version', true), ''), ''),
  configuration_version   text NOT NULL DEFAULT coalesce(nullif(current_setting('wicket.config_version', true), ''), ''),
  created_at              timestamptz NOT NULL DEFAULT now(),
  updated_at              timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT item_number_charset CHECK (
    char_length(number) BETWEEN 1 AND 40
    AND number ~ '^[A-Za-z0-9.-]+$'
  ),
  CONSTRAINT item_number_unique UNIQUE (number)
);
ALTER TABLE items.item OWNER TO wicket_owner;
COMMENT ON TABLE items.item IS
  'Part master. Human identity is number+revision; id is the key. Expiry lives on the lot, not here.';

CREATE TABLE items.item_revision_history (
  id            uuid PRIMARY KEY,
  item_id       uuid NOT NULL,
  revision      text NOT NULL,
  recorded_at   timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE items.item_revision_history OWNER TO wicket_owner;
COMMENT ON TABLE items.item_revision_history IS
  'Append-only revision log. Rows are never updated.';

-- D2 R5 is served by wicket_ledger::has_postings (R-2s-3 / R-2s-8).
-- Do not recreate items.item_has_postings here; 0002 drops any leftover.

SELECT audit.attach('items.item'::regclass);
SELECT audit.attach('items.item_revision_history'::regclass);

GRANT SELECT, INSERT, UPDATE ON TABLE items.item TO wicket_app;
GRANT SELECT, INSERT ON TABLE items.item_revision_history TO wicket_app;
REVOKE UPDATE, DELETE ON TABLE items.item_revision_history FROM wicket_app, PUBLIC;
REVOKE DELETE ON TABLE items.item FROM wicket_app, PUBLIC;

ALTER SCHEMA items OWNER TO wicket_owner;
