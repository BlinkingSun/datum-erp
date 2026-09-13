-- Restore the one-shot registrar so 0001 down can drop it.

CREATE FUNCTION esign._register_hash_redact() RETURNS void
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  INSERT INTO audit.redact (relid, column_name, reason, decided_by)
  VALUES ('esign.signature'::regclass, 'record_content_hash', 'content binding', 'wicket-esign')
  ON CONFLICT DO NOTHING;
END
$fn$;
ALTER FUNCTION esign._register_hash_redact() OWNER TO wicket_owner;
