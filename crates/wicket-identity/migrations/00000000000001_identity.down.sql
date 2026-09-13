-- Reverse of 0001_identity, including the audit FK.

ALTER TABLE audit.event DROP CONSTRAINT IF EXISTS event_actor_fk;

DO $redact$
BEGIN
  DELETE FROM audit.redact
   WHERE relid IN (
           to_regclass('identity.login_credential'),
           to_regclass('identity.signing_credential'),
           to_regclass('identity.credential_reset')
         );
EXCEPTION WHEN insufficient_privilege THEN
  NULL;
END
$redact$;

DROP TABLE IF EXISTS transient.session;
DROP TABLE IF EXISTS identity.principal_role;
DROP TABLE IF EXISTS identity.role_permission;
DROP TABLE IF EXISTS identity.role;
DROP TABLE IF EXISTS identity.credential_reset;
DROP TABLE IF EXISTS identity.signing_credential;
DROP TABLE IF EXISTS identity.login_credential;
DROP TABLE IF EXISTS identity.display_name_history;
DROP TABLE IF EXISTS identity.username_history;
DROP TABLE IF EXISTS identity.principal;

DROP FUNCTION IF EXISTS identity.record_display_name();
DROP FUNCTION IF EXISTS identity.record_username();
DROP FUNCTION IF EXISTS identity.refuse_username_reuse();

DELETE FROM wicket.schema_class WHERE nspname = 'identity';

DROP SCHEMA IF EXISTS identity;
