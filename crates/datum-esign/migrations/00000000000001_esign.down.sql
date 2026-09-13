-- Reverse of 0001_esign.

DROP FUNCTION IF EXISTS esign.close_signing_sessions(uuid, text);
DROP FUNCTION IF EXISTS esign.touch_signing_session(uuid);
DROP FUNCTION IF EXISTS esign.open_signing_session(uuid, uuid, uuid, text, text, text);
DROP FUNCTION IF EXISTS esign.load_open_signing_session(uuid);
DROP TABLE IF EXISTS transient.signing_session;
DROP TABLE IF EXISTS esign.signature;
DROP TABLE IF EXISTS esign.meaning_policy;
DROP FUNCTION IF EXISTS esign.refuse_immutable_update();

DELETE FROM datum.schema_class WHERE nspname = 'esign';

DROP SCHEMA IF EXISTS esign;
