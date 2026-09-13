-- 0003: D-2b-1 signer FK; drop extra superseded_by + esign.supersession
-- (D-2b-7: superseded is live sm.instance.version > record_version).
-- D-2b-3: close open signing sessions on principal deactivate.
-- No new tables. No wicket.* GUC writes (lint-sql rule (d); 0001 only).
-- Invoker-rights; not SECURITY DEFINER (R-2s-8). Reversible.

-- Replace the insert-only trigger first so DROP COLUMN superseded_by does
-- not leave a function that names a gone column.
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

DROP FUNCTION IF EXISTS esign.supersede_signature(uuid, uuid);
DROP TABLE IF EXISTS esign.supersession;

ALTER TABLE esign.signature DROP CONSTRAINT IF EXISTS signature_superseded_by_fk;
ALTER TABLE esign.signature DROP COLUMN IF EXISTS superseded_by;

ALTER TABLE esign.signature
  ADD CONSTRAINT signature_signer_fk
  FOREIGN KEY (signer_id) REFERENCES identity.principal (id);

-- Live version for D-2b-7. plpgsql so CREATE succeeds when sm is not yet
-- installed (esign migrate_down_then_up is db+audit+identity then this crate).
-- search_path includes sm; the body names the bare relation (invariant 6).
CREATE FUNCTION esign.live_instance_version(p_doc_type text, p_doc_id uuid)
RETURNS bigint
LANGUAGE plpgsql
STABLE
SECURITY INVOKER
SET search_path = pg_catalog, sm, pg_temp
AS $fn$
DECLARE v bigint;
BEGIN
  IF p_doc_type IS NULL OR p_doc_id IS NULL THEN
    RETURN NULL;
  END IF;
  SELECT i.version INTO v
    FROM instance i
   WHERE i.doc_type = p_doc_type
     AND i.doc_id = p_doc_id;
  RETURN v;
END
$fn$;
ALTER FUNCTION esign.live_instance_version(text, uuid) OWNER TO wicket_owner;
GRANT EXECUTE ON FUNCTION esign.live_instance_version(text, uuid)
  TO wicket_app, wicket_migrate, wicket_owner;

CREATE FUNCTION esign.close_sessions_on_deactivate() RETURNS trigger
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $fn$
BEGIN
  IF NEW.status = 'inactive' AND OLD.status IS DISTINCT FROM 'inactive' THEN
    PERFORM esign.close_signing_sessions(NEW.id, 'principal_deactivated');
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION esign.close_sessions_on_deactivate() OWNER TO wicket_owner;
GRANT EXECUTE ON FUNCTION esign.close_sessions_on_deactivate()
  TO wicket_app, wicket_migrate, wicket_owner;

-- identity.principal is owned by wicket_owner; wicket_migrate inherits that role.
SET ROLE wicket_owner;
CREATE TRIGGER esign_close_sessions_on_deactivate
  AFTER UPDATE OF status ON identity.principal
  FOR EACH ROW
  EXECUTE FUNCTION esign.close_sessions_on_deactivate();
RESET ROLE;
