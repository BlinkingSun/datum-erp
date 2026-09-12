-- 0003: inv. 17 stamps on append-only revision history (like items.item).

ALTER TABLE items.item_revision_history
  ADD COLUMN application_version text NOT NULL
    DEFAULT coalesce(nullif(current_setting('datum.app_version', true), ''), ''),
  ADD COLUMN configuration_version text NOT NULL
    DEFAULT coalesce(nullif(current_setting('datum.config_version', true), ''), '');
