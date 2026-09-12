-- Reverse 0001_lots.

DROP TABLE IF EXISTS lots.status_history;
DROP TABLE IF EXISTS lots.package;
DROP TABLE IF EXISTS lots.serial;
DROP TABLE IF EXISTS lots.lot;

DROP SCHEMA IF EXISTS lots;

DELETE FROM datum.schema_class WHERE nspname = 'lots';
