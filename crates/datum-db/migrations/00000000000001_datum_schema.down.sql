-- Reverse of 0001_datum_schema. Catalogue of schema datum is removed.

DROP TABLE IF EXISTS datum.schema_history;
DROP TABLE IF EXISTS datum.schema_class;
DROP SCHEMA IF EXISTS datum CASCADE;
