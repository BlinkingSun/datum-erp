-- Reverse 0001_customfields.

DROP TABLE IF EXISTS customfields.value_reference;
DROP TABLE IF EXISTS customfields.value_enum;
DROP TABLE IF EXISTS customfields.value_date;
DROP TABLE IF EXISTS customfields.value_bool;
DROP TABLE IF EXISTS customfields.value_decimal;
DROP TABLE IF EXISTS customfields.value_integer;
DROP TABLE IF EXISTS customfields.value_text;
DROP TABLE IF EXISTS customfields.value_string;
DROP TABLE IF EXISTS customfields.definition;

DELETE FROM datum.schema_class WHERE nspname = 'customfields';

DROP SCHEMA IF EXISTS customfields;
