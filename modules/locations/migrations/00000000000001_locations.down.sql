-- Reverse 0001_locations.

DROP TABLE IF EXISTS locations.location;
DROP TABLE IF EXISTS locations.site;

DELETE FROM wicket.schema_class WHERE nspname = 'locations';

DROP SCHEMA IF EXISTS locations;
