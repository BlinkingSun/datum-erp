-- Reverse of 0001_items.

DROP FUNCTION IF EXISTS items.item_has_postings(uuid);
DROP TABLE IF EXISTS items.item_revision_history;
DROP TABLE IF EXISTS items.item;
DELETE FROM datum.schema_class WHERE nspname = 'items';
DROP SCHEMA IF EXISTS items;
