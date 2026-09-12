-- 0001_items: part master in schema-class `app`. Reversible. Owned by datum_owner.
-- No DELETE, no ON DELETE CASCADE. Audit attached at CREATE TABLE.
-- Schema is created as datum_migrate so default TRIGGER privileges exist
-- before ALTER OWNER (event trigger fires at CREATE TABLE).

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'items.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS items AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA items FROM PUBLIC;
GRANT USAGE ON SCHEMA items TO datum_app;
GRANT USAGE, CREATE ON SCHEMA items TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('items', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA items
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA items
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA items
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

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
  application_version     text NOT NULL DEFAULT coalesce(nullif(current_setting('datum.app_version', true), ''), ''),
  configuration_version   text NOT NULL DEFAULT coalesce(nullif(current_setting('datum.config_version', true), ''), ''),
  created_at              timestamptz NOT NULL DEFAULT now(),
  updated_at              timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT item_number_charset CHECK (
    char_length(number) BETWEEN 1 AND 40
    AND number ~ '^[A-Za-z0-9.-]+$'
  ),
  CONSTRAINT item_number_unique UNIQUE (number)
);
ALTER TABLE items.item OWNER TO datum_owner;
COMMENT ON TABLE items.item IS
  'Part master. Human identity is number+revision; id is the key. Expiry lives on the lot, not here.';

CREATE TABLE items.item_revision_history (
  id            uuid PRIMARY KEY,
  item_id       uuid NOT NULL,
  revision      text NOT NULL,
  recorded_at   timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE items.item_revision_history OWNER TO datum_owner;
COMMENT ON TABLE items.item_revision_history IS
  'Append-only revision log. Rows are never updated.';

-- Published has_postings seam: ledger.registry::has_postings is not on the
-- kernel surface; this SECURITY DEFINER wrapper is the items-side read.
CREATE FUNCTION items.item_has_postings(p_item_id uuid) RETURNS boolean
LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path = pg_catalog, items, ledger
AS $items$
BEGIN
  IF to_regclass('ledger.posting') IS NOT NULL THEN
    RETURN EXISTS (
      SELECT 1 FROM ledger.posting p WHERE p.item_id = p_item_id
    );
  END IF;
  RETURN false;
END
$items$;
ALTER FUNCTION items.item_has_postings(uuid) OWNER TO datum_owner;

SELECT audit.attach('items.item'::regclass);
SELECT audit.attach('items.item_revision_history'::regclass);

GRANT SELECT, INSERT, UPDATE ON TABLE items.item TO datum_app;
GRANT SELECT, INSERT ON TABLE items.item_revision_history TO datum_app;
REVOKE UPDATE, DELETE ON TABLE items.item_revision_history FROM datum_app, PUBLIC;
REVOKE DELETE ON TABLE items.item FROM datum_app, PUBLIC;
GRANT EXECUTE ON FUNCTION items.item_has_postings(uuid) TO datum_app, datum_owner, datum_migrate;

ALTER SCHEMA items OWNER TO datum_owner;
