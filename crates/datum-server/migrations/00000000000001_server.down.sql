-- Reverse of 0001_server.

DROP TABLE IF EXISTS server_transient.http_session;
DROP TABLE IF EXISTS server_transient.idempotency;
DELETE FROM datum.schema_class WHERE nspname = 'server_transient';
DROP SCHEMA IF EXISTS server_transient;

DROP TABLE IF EXISTS server.boot_record;
DELETE FROM datum.schema_class WHERE nspname = 'server';
DROP SCHEMA IF EXISTS server;
