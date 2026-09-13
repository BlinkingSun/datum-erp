-- Reverse of 0003: restore extra superseded_by + esign.supersession, drop the
-- signer FK and the D-2b-3 / D-2b-7 helpers.

SET ROLE wicket_owner;
DROP TRIGGER IF EXISTS esign_close_sessions_on_deactivate ON identity.principal;
RESET ROLE;

DROP FUNCTION IF EXISTS esign.close_sessions_on_deactivate();
DROP FUNCTION IF EXISTS esign.live_instance_version(text, uuid);

ALTER TABLE esign.signature DROP CONSTRAINT IF EXISTS signature_signer_fk;

ALTER TABLE esign.signature ADD COLUMN superseded_by uuid NULL;
ALTER TABLE esign.signature
  ADD CONSTRAINT signature_superseded_by_fk
  FOREIGN KEY (superseded_by) REFERENCES esign.signature (signature_id);

CREATE OR REPLACE FUNCTION esign.refuse_immutable_update() RETURNS trigger
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
     OR NEW.superseded_by      IS DISTINCT FROM OLD.superseded_by
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
  RETURN NEW;
END
$fn$;
ALTER FUNCTION esign.refuse_immutable_update() OWNER TO wicket_owner;

CREATE TABLE esign.supersession (
    old_signature_id uuid PRIMARY KEY REFERENCES esign.signature (signature_id),
    new_signature_id uuid NOT NULL REFERENCES esign.signature (signature_id),
    superseded_at    timestamptz NOT NULL DEFAULT now(),
    CHECK (old_signature_id <> new_signature_id)
);
ALTER TABLE esign.supersession OWNER TO wicket_owner;
GRANT SELECT, INSERT ON TABLE esign.supersession TO wicket_app;
REVOKE UPDATE, DELETE ON TABLE esign.supersession FROM wicket_app, PUBLIC;

CREATE FUNCTION esign.supersede_signature(p_old uuid, p_new uuid) RETURNS boolean
LANGUAGE plpgsql VOLATILE SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE n bigint;
BEGIN
  INSERT INTO esign.supersession (old_signature_id, new_signature_id)
  VALUES (p_old, p_new)
  ON CONFLICT (old_signature_id) DO NOTHING;
  GET DIAGNOSTICS n = ROW_COUNT;
  RETURN n > 0;
END
$fn$;
ALTER FUNCTION esign.supersede_signature(uuid, uuid) OWNER TO wicket_owner;
GRANT EXECUTE ON FUNCTION esign.supersede_signature(uuid, uuid)
  TO wicket_app, wicket_migrate, wicket_owner;
