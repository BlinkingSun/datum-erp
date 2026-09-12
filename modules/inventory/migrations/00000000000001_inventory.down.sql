-- Reverse 0001_inventory.

DROP TABLE IF EXISTS inventory_transient.idempotency;
DROP SCHEMA IF EXISTS inventory_transient;

DROP TABLE IF EXISTS inventory.document_line;
DROP TABLE IF EXISTS inventory.document;
DROP SCHEMA IF EXISTS inventory;

DELETE FROM datum.schema_class WHERE nspname IN ('inventory', 'inventory_transient');
