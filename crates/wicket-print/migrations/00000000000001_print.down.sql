DROP TABLE IF EXISTS print.render_log;
DROP TABLE IF EXISTS print.template;
DROP TABLE IF EXISTS print.install;
DELETE FROM wicket.schema_class WHERE nspname = 'print';
DROP SCHEMA IF EXISTS print;
