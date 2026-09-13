-- Reverse 0003_parent_group_link.

DROP FUNCTION IF EXISTS ledger.children_of(uuid);

ALTER TABLE ledger.posting_group DROP COLUMN IF EXISTS parent_group_id;
