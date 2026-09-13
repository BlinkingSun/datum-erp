-- Reverse of 0001_module.

DROP TABLE IF EXISTS module.install_log;
DROP TABLE IF EXISTS module.configuration;
DROP TABLE IF EXISTS module.installed;

DELETE FROM wicket.schema_class WHERE nspname = 'module';

DROP SCHEMA IF EXISTS module;
