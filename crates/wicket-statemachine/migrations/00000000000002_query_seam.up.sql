-- 0002_query_seam: published live-state reads for kernel crates that must
-- not SELECT sm.instance / sm.machine themselves (R-2s-3; documents previously
-- used format('%I.%I','sm','instance') — fail-class). Invoker-rights (NOT
-- SECURITY DEFINER — R-2s-8 does not exempt wicket-statemachine). wicket_app
-- already has SELECT on sm.instance / sm.machine. GRANT is role-scoped
-- (REVOKE PUBLIC). No audit.require_context: ReadPool has no actor. Reversible.

CREATE FUNCTION sm.current_state(p_doc_type text, p_doc_id uuid)
RETURNS text
LANGUAGE sql
STABLE
SECURITY INVOKER
SET search_path = pg_catalog, sm
AS $$
  SELECT i.state
    FROM sm.instance i
   WHERE i.doc_type = p_doc_type
     AND i.doc_id = p_doc_id
$$;
ALTER FUNCTION sm.current_state(text, uuid) OWNER TO wicket_owner;
REVOKE ALL ON FUNCTION sm.current_state(text, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION sm.current_state(text, uuid) TO wicket_app;

CREATE FUNCTION sm.instance_exists(p_doc_type text, p_doc_id uuid)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY INVOKER
SET search_path = pg_catalog, sm
AS $$
  SELECT EXISTS (
    SELECT 1
      FROM sm.instance i
     WHERE i.doc_type = p_doc_type
       AND i.doc_id = p_doc_id
  )
$$;
ALTER FUNCTION sm.instance_exists(text, uuid) OWNER TO wicket_owner;
REVOKE ALL ON FUNCTION sm.instance_exists(text, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION sm.instance_exists(text, uuid) TO wicket_app;

CREATE FUNCTION sm.machine_id_for(p_doc_type text)
RETURNS uuid
LANGUAGE sql
STABLE
SECURITY INVOKER
SET search_path = pg_catalog, sm
AS $$
  SELECT m.id
    FROM sm.machine m
   WHERE m.doc_type = p_doc_type
$$;
ALTER FUNCTION sm.machine_id_for(text) OWNER TO wicket_owner;
REVOKE ALL ON FUNCTION sm.machine_id_for(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION sm.machine_id_for(text) TO wicket_app;
