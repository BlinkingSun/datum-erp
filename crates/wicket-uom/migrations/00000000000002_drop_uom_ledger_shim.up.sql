-- Drop the uom-local SECURITY DEFINER shim over ledger.posting (R-2s-3).
-- Postings checks move to Rust via the ledger query seam (`ledger.has_postings`).

DROP TRIGGER IF EXISTS zz_uom_stock_immutable ON uom.item_stock;
DROP FUNCTION IF EXISTS uom.enforce_stock_item_immutable();

DROP FUNCTION IF EXISTS uom.item_has_postings(uuid);

DO $revoke$
BEGIN
  IF to_regclass('ledger.posting') IS NOT NULL THEN
    EXECUTE 'REVOKE SELECT (item_id) ON TABLE ledger.posting FROM wicket_owner';
  END IF;
END
$revoke$;
