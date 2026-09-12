-- 0003_parent_group_link: optional parent posting group (R-2s-6). Reversible.
-- `ledger.enforce_group_invariants` and transient projections are unchanged:
-- invariants fold postings/consumption by group_id only; projections never read
-- posting_group metadata.

ALTER TABLE ledger.posting_group
  ADD COLUMN parent_group_id uuid
  REFERENCES ledger.posting_group (group_id);

CREATE INDEX posting_group_parent_idx ON ledger.posting_group (parent_group_id)
  WHERE parent_group_id IS NOT NULL;

CREATE FUNCTION ledger.children_of(p_parent_group_id uuid)
RETURNS SETOF uuid
LANGUAGE plpgsql
STABLE
SET search_path = pg_catalog, ledger, audit
AS $fn$
BEGIN
  PERFORM audit.require_context();
  RETURN QUERY
    SELECT g.group_id
      FROM ledger.posting_group g
     WHERE g.parent_group_id = p_parent_group_id
     ORDER BY g.posted_at, g.group_id;
END
$fn$;
ALTER FUNCTION ledger.children_of(uuid) OWNER TO datum_owner;
REVOKE ALL ON FUNCTION ledger.children_of(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION ledger.children_of(uuid) TO datum_app;
