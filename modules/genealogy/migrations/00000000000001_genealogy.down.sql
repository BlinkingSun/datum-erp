-- Reverse 0001_genealogy.

DROP TABLE IF EXISTS genealogy_transient.trace_cache;
DROP SCHEMA IF EXISTS genealogy_transient;

DROP SCHEMA IF EXISTS genealogy;

DELETE FROM wicket.schema_class WHERE nspname IN ('genealogy', 'genealogy_transient');
