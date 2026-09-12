-- 00-init-roles.sql — five roles per CONTRACT §8a / D3 §1.1. Idempotent.
-- Dev-only passwords `datum` on the two LOGIN roles. Never used in production.

\set ON_ERROR_STOP on

DO $$ BEGIN
  CREATE ROLE datum_owner       NOLOGIN;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  CREATE ROLE datum_audit_row   NOLOGIN;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  CREATE ROLE datum_audit_event NOLOGIN;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  CREATE ROLE datum_migrate     LOGIN;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  CREATE ROLE datum_app         LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

GRANT datum_owner TO datum_migrate;
REVOKE SET ON PARAMETER session_replication_role FROM datum_app;  -- PG 15+

-- Dev-only passwords `datum` on the two LOGIN roles. Never used in production.
ALTER ROLE datum_migrate LOGIN PASSWORD 'datum';
ALTER ROLE datum_app     LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD 'datum';
