-- 0002: drop the items-side has_postings shim (R-2s-3 / R-2s-8).
-- D2 R5 is served by wicket_ledger::has_postings on the caller's Tx.
-- DROP FUNCTION also removes GRANT EXECUTE on the function.

DROP FUNCTION IF EXISTS items.item_has_postings(uuid);
