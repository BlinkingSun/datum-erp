-- 01-database.sql — datum_test_template and datum_test, owned by datum_owner.
-- Idempotent. Template is marked datistemplate; both have sane connection limits.

\set ON_ERROR_STOP on

SELECT format('CREATE DATABASE %I OWNER datum_owner', 'datum_test_template')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = 'datum_test_template')\gexec

SELECT format('CREATE DATABASE %I OWNER datum_owner', 'datum_test')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = 'datum_test')\gexec

-- Cloned case databases inherit the template limit; two pools x max_connections(2) fit in 8.
ALTER DATABASE datum_test_template WITH IS_TEMPLATE true;
ALTER DATABASE datum_test_template CONNECTION LIMIT 8;
ALTER DATABASE datum_test CONNECTION LIMIT 20;
