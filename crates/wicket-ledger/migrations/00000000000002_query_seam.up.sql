-- 0002_query_seam: published "does this item have postings" / "is any quantity
-- on hand at this location" helpers. Invoker-rights (NOT SECURITY DEFINER):
-- wicket_app already has SELECT on ledger.posting; audit.require_context()
-- fails closed (42501) when the caller did not begin through Tx::begin.
-- GRANT is role-scoped (REVOKE PUBLIC). Reversible.

CREATE FUNCTION ledger.has_postings(p_item_id uuid)
RETURNS boolean
LANGUAGE plpgsql
STABLE
SET search_path = pg_catalog, ledger, audit
AS $fn$
BEGIN
  PERFORM audit.require_context();
  RETURN EXISTS (
    SELECT 1 FROM ledger.posting p WHERE p.item_id = p_item_id
  );
END
$fn$;
ALTER FUNCTION ledger.has_postings(uuid) OWNER TO wicket_owner;
REVOKE ALL ON FUNCTION ledger.has_postings(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION ledger.has_postings(uuid) TO wicket_app;

CREATE FUNCTION ledger.has_quantity_at(p_location_id uuid)
RETURNS boolean
LANGUAGE plpgsql
STABLE
SET search_path = pg_catalog, ledger, audit
AS $fn$
BEGIN
  PERFORM audit.require_context();
  RETURN EXISTS (
    SELECT 1
      FROM ledger.posting p
     WHERE p.measure = 'QUANTITY'
       AND p.location_id = p_location_id
     GROUP BY p.item_id, p.lot_id, p.serial_id, p.uom_id
    HAVING SUM(p.quantity) > 0
  );
END
$fn$;
ALTER FUNCTION ledger.has_quantity_at(uuid) OWNER TO wicket_owner;
REVOKE ALL ON FUNCTION ledger.has_quantity_at(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION ledger.has_quantity_at(uuid) TO wicket_app;
