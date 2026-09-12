-- Reverse 0001_uom.

DROP TRIGGER IF EXISTS zz_uom_stock_immutable ON uom.item_stock;
DROP FUNCTION IF EXISTS uom.enforce_stock_item_immutable();
DROP FUNCTION IF EXISTS uom.item_has_postings(uuid);

DROP TABLE IF EXISTS uom.rounding_policy;
DROP TABLE IF EXISTS uom.factor;
DROP TABLE IF EXISTS uom.posting_stub;
DROP TABLE IF EXISTS uom.item_stock;
DROP TABLE IF EXISTS uom.unit;

DELETE FROM datum.schema_class WHERE nspname = 'uom';

DROP SCHEMA IF EXISTS uom;
