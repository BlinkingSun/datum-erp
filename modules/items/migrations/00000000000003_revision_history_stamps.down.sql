-- Reverse of 0003.

ALTER TABLE items.item_revision_history
  DROP COLUMN IF EXISTS application_version,
  DROP COLUMN IF EXISTS configuration_version;
