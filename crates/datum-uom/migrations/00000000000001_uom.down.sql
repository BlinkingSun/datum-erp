-- Reverse 0001_uom. Drops only objects created in 0001_uom.up.sql.

DROP TRIGGER IF EXISTS zz_uom_stock_immutable ON uom.item_stock;
DROP FUNCTION IF EXISTS uom.enforce_stock_item_immutable();
DROP FUNCTION IF EXISTS uom.item_has_postings(uuid);

DROP TABLE IF EXISTS uom.rounding_policy;
DROP TABLE IF EXISTS uom.factor;
DROP TABLE IF EXISTS uom.item_stock;
DROP TABLE IF EXISTS uom.unit;

DELETE FROM datum.schema_class WHERE nspname = 'uom';

DROP SCHEMA IF EXISTS uom;

-- btree_gist: created in 0001_uom.up.sql. Drop only when no user-schema objects still
-- depend on it (shared DBs may retain the extension for another crate).
DO $drop_btree_gist$
DECLARE
  ext_oid oid;
  user_deps bigint;
BEGIN
  SELECT oid INTO ext_oid FROM pg_extension WHERE extname = 'btree_gist';
  IF ext_oid IS NULL THEN
    RETURN;
  END IF;

  SELECT count(*) INTO user_deps
  FROM pg_depend d
  JOIN pg_class c ON c.oid = d.objid AND d.classid = 'pg_class'::regclass
  JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE d.refobjid = ext_oid
    AND d.deptype = 'e'
    AND n.nspname NOT IN ('pg_catalog', 'information_schema')
    AND n.nspname NOT LIKE 'pg\_toast%'
    AND n.nspname NOT LIKE 'pg\_temp\_%';

  IF user_deps > 0 THEN
    RAISE NOTICE
      'datum-uom: skipping DROP EXTENSION btree_gist (% user relation(s) still depend on it)',
      user_deps;
    RETURN;
  END IF;

  DROP EXTENSION IF EXISTS btree_gist;
END
$drop_btree_gist$;
