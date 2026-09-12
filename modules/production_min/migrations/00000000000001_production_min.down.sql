-- Reverse 0001_production_min.

DROP TABLE IF EXISTS production_min.completion;
DROP TABLE IF EXISTS production_min.issue_line;
DROP TABLE IF EXISTS production_min.work_order;
DROP SCHEMA IF EXISTS production_min;

DELETE FROM datum.schema_class WHERE nspname = 'production_min';
