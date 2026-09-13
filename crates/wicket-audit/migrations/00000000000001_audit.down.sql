-- Reverse of 0001_audit. Schema `audit` is left in place (it exists in the
-- template); contents created here are removed. Event triggers are dropped
-- only when the current user is superuser; otherwise call
-- wicket_audit::uninstall_privileged first.

DO $priv$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = current_user AND rolsuper) THEN
    DROP EVENT TRIGGER IF EXISTS audit_protect_drop;
    DROP EVENT TRIGGER IF EXISTS audit_protect;
    DROP EVENT TRIGGER IF EXISTS audit_attach;
  END IF;
END
$priv$;

DROP TABLE IF EXISTS audit.anchor;
DROP TABLE IF EXISTS audit.tx_seal;
DROP TABLE IF EXISTS audit.exempt;
DROP TABLE IF EXISTS audit.reason_policy;
DROP TABLE IF EXISTS audit.redact;
DROP TABLE IF EXISTS audit.event CASCADE;

DROP FUNCTION IF EXISTS audit.reseal_from(bigint);
DROP FUNCTION IF EXISTS audit.record_anchor(bigint, bytea, text, text);
DROP FUNCTION IF EXISTS audit.head();
DROP FUNCTION IF EXISTS audit.verify(bigint, bigint);
DROP FUNCTION IF EXISTS audit.seal_current_tx() CASCADE;
DROP FUNCTION IF EXISTS audit.canon_row(audit.event);
DROP FUNCTION IF EXISTS audit.ts_canon(timestamptz);
DROP FUNCTION IF EXISTS audit.canon_jsonb(jsonb);
DROP FUNCTION IF EXISTS audit.ensure_partitions(integer);
DROP FUNCTION IF EXISTS audit.grant_event_insert(regclass);
DROP FUNCTION IF EXISTS audit.protect();
DROP FUNCTION IF EXISTS audit.attach_new_tables();
DROP FUNCTION IF EXISTS audit.attach(regclass);
DROP FUNCTION IF EXISTS audit.log_event(text, text, text, text, text, text, jsonb);
DROP FUNCTION IF EXISTS audit.stmt_truncate() CASCADE;
DROP FUNCTION IF EXISTS audit.row_change() CASCADE;
DROP FUNCTION IF EXISTS audit.refuse_mutation() CASCADE;
DROP FUNCTION IF EXISTS audit.pk_of(text[], jsonb);
DROP FUNCTION IF EXISTS audit.scrub(oid, jsonb);
DROP FUNCTION IF EXISTS audit.reason_required(oid, text);
DROP FUNCTION IF EXISTS audit.require_context();
DROP FUNCTION IF EXISTS audit.guc_inet(text);
DROP FUNCTION IF EXISTS audit.guc_uuid(text);

DROP TYPE IF EXISTS audit.context;

DO $schema$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'audit') THEN
    EXECUTE 'ALTER SCHEMA audit OWNER TO wicket_migrate';
  END IF;
END
$schema$;
