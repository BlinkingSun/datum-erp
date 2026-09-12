-- 0002_server: persist granted permission keys on the HTTP session so GET
-- handlers can authorize without opening a write transaction.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'server.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

ALTER TABLE server_transient.http_session
  ADD COLUMN IF NOT EXISTS permissions text[] NOT NULL DEFAULT '{}';
