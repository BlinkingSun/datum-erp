-- 0003_engine_write: sm.instance DML for datum_app is SELECT only. Spawn and
-- transition go through invoker-rights helpers (NOT SECURITY DEFINER — R-2s-8
-- does not exempt this crate). The helpers write a datum_owner view so the
-- invoker does not need INSERT/UPDATE on the table (view default is owner
-- rights on the base table). GRANT is role-scoped (REVOKE PUBLIC). Reversible.

REVOKE INSERT, UPDATE, DELETE ON TABLE sm.instance FROM PUBLIC, datum_app;
GRANT SELECT ON TABLE sm.instance TO datum_app;

CREATE VIEW sm.instance_engine AS
  SELECT doc_type, doc_id, machine_id, state, version, entered_at
    FROM sm.instance;
ALTER VIEW sm.instance_engine OWNER TO datum_owner;
REVOKE ALL ON sm.instance_engine FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE ON sm.instance_engine TO datum_app;

CREATE FUNCTION sm.instance_engine_guard() RETURNS trigger
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $fn$
BEGIN
  IF pg_catalog.current_setting('sm.engine_write', true) IS DISTINCT FROM '1' THEN
    RAISE EXCEPTION 'sm.instance is writable only through sm.spawn_instance / sm.transition_instance'
      USING ERRCODE = '42501';
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION sm.instance_engine_guard() OWNER TO datum_owner;

CREATE TRIGGER instance_engine_guard
  BEFORE INSERT OR UPDATE ON sm.instance
  FOR EACH ROW
  EXECUTE FUNCTION sm.instance_engine_guard();

CREATE FUNCTION sm.spawn_instance(
  p_doc_type text,
  p_doc_id uuid,
  p_machine_id uuid,
  p_state text
) RETURNS TABLE (
  machine_id uuid,
  state text,
  version bigint,
  entered_at timestamptz
)
LANGUAGE plpgsql
VOLATILE
SECURITY INVOKER
SET search_path = pg_catalog, sm
AS $fn$
BEGIN
  PERFORM pg_catalog.set_config('sm.engine_write', '1', true);
  RETURN QUERY
    INSERT INTO sm.instance_engine (
      doc_type, doc_id, machine_id, state, version, entered_at
    ) VALUES (
      p_doc_type, p_doc_id, p_machine_id, p_state, 1, pg_catalog.now()
    )
    RETURNING sm.instance_engine.machine_id,
              sm.instance_engine.state,
              sm.instance_engine.version,
              sm.instance_engine.entered_at;
  PERFORM pg_catalog.set_config('sm.engine_write', '', true);
END
$fn$;
ALTER FUNCTION sm.spawn_instance(text, uuid, uuid, text) OWNER TO datum_owner;
REVOKE ALL ON FUNCTION sm.spawn_instance(text, uuid, uuid, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION sm.spawn_instance(text, uuid, uuid, text) TO datum_app;

CREATE FUNCTION sm.transition_instance(
  p_doc_type text,
  p_doc_id uuid,
  p_from text,
  p_to text,
  p_version bigint
) RETURNS TABLE (
  machine_id uuid,
  state text,
  version bigint,
  entered_at timestamptz
)
LANGUAGE plpgsql
VOLATILE
SECURITY INVOKER
SET search_path = pg_catalog, sm
AS $fn$
BEGIN
  PERFORM pg_catalog.set_config('sm.engine_write', '1', true);
  RETURN QUERY
    UPDATE sm.instance_engine e
       SET state = p_to,
           version = e.version + 1,
           entered_at = pg_catalog.now()
     WHERE e.doc_type = p_doc_type
       AND e.doc_id = p_doc_id
       AND e.state = p_from
       AND e.version = p_version
    RETURNING e.machine_id, e.state, e.version, e.entered_at;
  PERFORM pg_catalog.set_config('sm.engine_write', '', true);
END
$fn$;
ALTER FUNCTION sm.transition_instance(text, uuid, text, text, bigint) OWNER TO datum_owner;
REVOKE ALL ON FUNCTION sm.transition_instance(text, uuid, text, text, bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION sm.transition_instance(text, uuid, text, text, bigint) TO datum_app;
