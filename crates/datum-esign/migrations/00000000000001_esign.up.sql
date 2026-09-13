-- 0001_esign: electronic signatures (D-2b-1..3). Schema `esign` class app.
-- Working state is `transient.signing_session` (D-2b-3; event trigger skips
-- schema `transient`). Rust never names `transient.*` (R-2s-3); session DML
-- goes through invoker `esign.*` functions. Reversible. Owned by datum_owner.
-- No DELETE on app tables, no ON DELETE CASCADE. Column-level UPDATE on the
-- signature claim columns; a BEFORE UPDATE trigger refuses any other change.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'esign.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS esign AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA esign FROM PUBLIC;
GRANT USAGE ON SCHEMA esign TO datum_app;
GRANT USAGE, CREATE ON SCHEMA esign TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('esign', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA esign
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA esign
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA esign
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

CREATE TABLE esign.meaning_policy (
    meaning          text PRIMARY KEY,
    requires_reason  boolean NOT NULL,
    permission_hint  text    NOT NULL DEFAULT ''
);
ALTER TABLE esign.meaning_policy OWNER TO datum_owner;

CREATE TABLE esign.signature (
    signature_id          uuid PRIMARY KEY,
    signer_id             uuid        NOT NULL,
    signer_printed_name   text        NOT NULL,
    signer_username       text        NOT NULL,
    meaning               text        NOT NULL,
    reason                text            NULL,
    signed_at             timestamptz NOT NULL DEFAULT now(),
    signed_at_zone        text        NOT NULL,
    record_table          text        NOT NULL,
    record_id             uuid        NOT NULL,
    record_version        bigint      NOT NULL,
    doc_type              text        NOT NULL,
    record_content_hash   bytea       NOT NULL CHECK (octet_length(record_content_hash) = 32),
    record_snapshot       jsonb       NOT NULL,
    permission_snapshot   text[]      NOT NULL,
    credential_kind       text        NOT NULL CHECK (credential_kind IN ('signing_password', 'idp_step_up')),
    components_used       text[]      NOT NULL CHECK (cardinality(components_used) >= 1),
    signing_session_id    uuid            NULL,
    login_session_id      uuid            NULL,
    source_device         text            NULL,
    source_ip             inet            NULL,
    expires_at            timestamptz NOT NULL,
    consumed_at           timestamptz     NULL,
    consumed_xid          xid8            NULL,
    superseded_by         uuid            NULL,
    application_version   text        NOT NULL DEFAULT coalesce(nullif(current_setting('datum.app_version', true), ''), ''),
    configuration_version text        NOT NULL DEFAULT coalesce(nullif(current_setting('datum.config_version', true), ''), ''),
    CHECK (signer_printed_name <> ''),
    CHECK (signer_username <> ''),
    CHECK (meaning <> ''),
    CHECK (signed_at_zone <> ''),
    CHECK (record_table <> ''),
    CHECK (doc_type <> ''),
    CHECK (record_version >= 1)
);
ALTER TABLE esign.signature OWNER TO datum_owner;

ALTER TABLE esign.signature
  ADD CONSTRAINT signature_superseded_by_fk
  FOREIGN KEY (superseded_by) REFERENCES esign.signature (signature_id);

CREATE INDEX signature_record ON esign.signature (record_table, record_id, record_version);
CREATE INDEX signature_signer ON esign.signature (signer_id);
CREATE INDEX signature_open ON esign.signature (signature_id) WHERE consumed_at IS NULL;

CREATE TABLE transient.signing_session (
    id                  uuid PRIMARY KEY,
    principal_id        uuid        NOT NULL,
    login_session_id    uuid            NULL,
    device_fingerprint  text            NULL,
    boot_epoch          text        NOT NULL DEFAULT '',
    source_ip           inet            NULL,
    opened_at           timestamptz NOT NULL DEFAULT now(),
    last_signed_at      timestamptz NOT NULL DEFAULT now(),
    closed_at           timestamptz     NULL,
    close_reason        text            NULL
);
ALTER TABLE transient.signing_session OWNER TO datum_owner;

CREATE INDEX signing_session_open
  ON transient.signing_session (principal_id)
  WHERE closed_at IS NULL;

-- Insert-only: the only mutable facts are the single-use claim columns, and
-- they are monotone (NULL → once). Invoker rights; not SECURITY DEFINER (R-2s-8).
CREATE FUNCTION esign.refuse_immutable_update() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  IF NEW.signature_id          IS DISTINCT FROM OLD.signature_id
     OR NEW.signer_id          IS DISTINCT FROM OLD.signer_id
     OR NEW.signer_printed_name IS DISTINCT FROM OLD.signer_printed_name
     OR NEW.signer_username    IS DISTINCT FROM OLD.signer_username
     OR NEW.meaning            IS DISTINCT FROM OLD.meaning
     OR NEW.reason             IS DISTINCT FROM OLD.reason
     OR NEW.signed_at          IS DISTINCT FROM OLD.signed_at
     OR NEW.signed_at_zone     IS DISTINCT FROM OLD.signed_at_zone
     OR NEW.record_table       IS DISTINCT FROM OLD.record_table
     OR NEW.record_id          IS DISTINCT FROM OLD.record_id
     OR NEW.record_version     IS DISTINCT FROM OLD.record_version
     OR NEW.doc_type           IS DISTINCT FROM OLD.doc_type
     OR NEW.record_content_hash IS DISTINCT FROM OLD.record_content_hash
     OR NEW.record_snapshot    IS DISTINCT FROM OLD.record_snapshot
     OR NEW.permission_snapshot IS DISTINCT FROM OLD.permission_snapshot
     OR NEW.credential_kind    IS DISTINCT FROM OLD.credential_kind
     OR NEW.components_used    IS DISTINCT FROM OLD.components_used
     OR NEW.signing_session_id IS DISTINCT FROM OLD.signing_session_id
     OR NEW.login_session_id   IS DISTINCT FROM OLD.login_session_id
     OR NEW.source_device      IS DISTINCT FROM OLD.source_device
     OR NEW.source_ip          IS DISTINCT FROM OLD.source_ip
     OR NEW.expires_at         IS DISTINCT FROM OLD.expires_at
     OR NEW.application_version IS DISTINCT FROM OLD.application_version
     OR NEW.configuration_version IS DISTINCT FROM OLD.configuration_version
  THEN
    RAISE EXCEPTION 'esign.signature is insert-only'
      USING ERRCODE = '42501';
  END IF;
  IF OLD.consumed_at IS NOT NULL
     AND NEW.consumed_at IS DISTINCT FROM OLD.consumed_at THEN
    RAISE EXCEPTION 'esign.signature consumed_at is monotone'
      USING ERRCODE = '42501';
  END IF;
  IF OLD.consumed_xid IS NOT NULL
     AND NEW.consumed_xid IS DISTINCT FROM OLD.consumed_xid THEN
    RAISE EXCEPTION 'esign.signature consumed_xid is monotone'
      USING ERRCODE = '42501';
  END IF;
  IF OLD.superseded_by IS NOT NULL
     AND NEW.superseded_by IS DISTINCT FROM OLD.superseded_by THEN
    RAISE EXCEPTION 'esign.signature superseded_by is monotone'
      USING ERRCODE = '42501';
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION esign.refuse_immutable_update() OWNER TO datum_owner;

CREATE TRIGGER signature_refuse_immutable_update
  BEFORE UPDATE ON esign.signature
  FOR EACH ROW EXECUTE FUNCTION esign.refuse_immutable_update();

SELECT audit.attach('esign.signature'::regclass);
SELECT audit.attach('esign.meaning_policy'::regclass);

INSERT INTO esign.meaning_policy (meaning, requires_reason, permission_hint)
VALUES
  ('Released', false, 'wo.release'),
  ('Approved', false, 'calibration.approve'),
  ('Reviewed', true,  '');

GRANT SELECT, INSERT ON TABLE esign.signature TO datum_app;
REVOKE UPDATE ON TABLE esign.signature FROM datum_app, PUBLIC;
GRANT UPDATE (consumed_at, consumed_xid, superseded_by) ON TABLE esign.signature TO datum_app;
REVOKE DELETE ON TABLE esign.signature FROM datum_app, PUBLIC;

GRANT SELECT, INSERT, UPDATE ON TABLE esign.meaning_policy TO datum_app;
REVOKE DELETE ON TABLE esign.meaning_policy FROM datum_app, PUBLIC;

GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE transient.signing_session TO datum_app;

GRANT EXECUTE ON FUNCTION esign.refuse_immutable_update() TO datum_app, datum_migrate, datum_owner;

-- Invoker session helpers so Rust never names transient.* (R-2s-3).
CREATE FUNCTION esign.load_open_signing_session(p_principal uuid)
RETURNS TABLE (
    id uuid,
    principal_id uuid,
    login_session_id uuid,
    device_fingerprint text,
    boot_epoch text,
    source_ip text,
    opened_at timestamptz,
    last_signed_at timestamptz,
    closed_at timestamptz,
    close_reason text
)
LANGUAGE sql STABLE SET search_path = pg_catalog, pg_temp AS $fn$
  SELECT s.id, s.principal_id, s.login_session_id, s.device_fingerprint, s.boot_epoch,
         host(s.source_ip), s.opened_at, s.last_signed_at, s.closed_at, s.close_reason
    FROM transient.signing_session s
   WHERE s.principal_id = p_principal AND s.closed_at IS NULL
   ORDER BY s.opened_at DESC
   LIMIT 1
$fn$;
ALTER FUNCTION esign.load_open_signing_session(uuid) OWNER TO datum_owner;

CREATE FUNCTION esign.open_signing_session(
    p_id uuid,
    p_principal uuid,
    p_login uuid,
    p_device text,
    p_boot text,
    p_ip text
)
RETURNS TABLE (
    id uuid,
    principal_id uuid,
    login_session_id uuid,
    device_fingerprint text,
    boot_epoch text,
    source_ip text,
    opened_at timestamptz,
    last_signed_at timestamptz,
    closed_at timestamptz,
    close_reason text
)
LANGUAGE sql VOLATILE SET search_path = pg_catalog, pg_temp AS $fn$
  INSERT INTO transient.signing_session
      (id, principal_id, login_session_id, device_fingerprint, boot_epoch,
       source_ip, opened_at, last_signed_at)
  VALUES (p_id, p_principal, p_login, p_device, p_boot, CAST(p_ip AS inet), now(), now())
  RETURNING id, principal_id, login_session_id, device_fingerprint, boot_epoch,
            host(source_ip), opened_at, last_signed_at, closed_at, close_reason
$fn$;
ALTER FUNCTION esign.open_signing_session(uuid, uuid, uuid, text, text, text) OWNER TO datum_owner;

CREATE FUNCTION esign.touch_signing_session(p_id uuid) RETURNS void
LANGUAGE sql VOLATILE SET search_path = pg_catalog, pg_temp AS $fn$
  UPDATE transient.signing_session
     SET last_signed_at = now()
   WHERE id = p_id AND closed_at IS NULL
$fn$;
ALTER FUNCTION esign.touch_signing_session(uuid) OWNER TO datum_owner;

CREATE FUNCTION esign.close_signing_sessions(p_principal uuid, p_reason text) RETURNS bigint
LANGUAGE plpgsql VOLATILE SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE n bigint;
BEGIN
  UPDATE transient.signing_session
     SET closed_at = now(), close_reason = p_reason
   WHERE principal_id = p_principal AND closed_at IS NULL;
  GET DIAGNOSTICS n = ROW_COUNT;
  RETURN n;
END
$fn$;
ALTER FUNCTION esign.close_signing_sessions(uuid, text) OWNER TO datum_owner;

GRANT EXECUTE ON FUNCTION esign.load_open_signing_session(uuid) TO datum_app, datum_migrate, datum_owner;
GRANT EXECUTE ON FUNCTION esign.open_signing_session(uuid, uuid, uuid, text, text, text) TO datum_app, datum_migrate, datum_owner;
GRANT EXECUTE ON FUNCTION esign.touch_signing_session(uuid) TO datum_app, datum_migrate, datum_owner;
GRANT EXECUTE ON FUNCTION esign.close_signing_sessions(uuid, text) TO datum_app, datum_migrate, datum_owner;

ALTER SCHEMA esign OWNER TO datum_owner;
