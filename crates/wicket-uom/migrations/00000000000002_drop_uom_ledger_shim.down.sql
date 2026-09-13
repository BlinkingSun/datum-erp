-- Reverse 0002_drop_uom_ledger_shim.up.sql.
--
-- Does NOT recreate uom.item_has_postings or uom.enforce_stock_item_immutable
-- bodies that read ledger.* / transient.* (R-2s-3). Down only restores the
-- column grant 0001 issued for the removed shim; immutability is enforced in
-- Rust via the ledger query seam after 0002.

DO $grant$
BEGIN
  IF to_regclass('ledger.posting') IS NOT NULL THEN
    EXECUTE 'GRANT SELECT (item_id) ON TABLE ledger.posting TO wicket_owner';
  END IF;
END
$grant$;
