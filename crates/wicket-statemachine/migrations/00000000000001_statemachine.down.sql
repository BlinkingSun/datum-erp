-- Reverse of 0001_statemachine.

DROP TABLE IF EXISTS sm.instance;
DROP TABLE IF EXISTS sm.edge;
DROP TABLE IF EXISTS sm.state;
DROP TABLE IF EXISTS sm.machine;

DELETE FROM wicket.schema_class WHERE nspname = 'sm';

DROP SCHEMA IF EXISTS sm;
