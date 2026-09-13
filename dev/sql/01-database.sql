-- 01-database.sql — template and case databases, owned by datum_owner.
-- Idempotent. Template is marked datistemplate; both have sane connection limits.
--
-- psql -v template=… -v dbname=… (just db-reset). Defaults keep CI unchanged:
--   template = datum_test_template
--   dbname   = datum_test

\set ON_ERROR_STOP on

\if :{?template}
\else
\set template datum_test_template
\endif
\if :{?dbname}
\else
\set dbname datum_test
\endif

SELECT format('CREATE DATABASE %I OWNER datum_owner', :'template')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'template')\gexec

SELECT format('CREATE DATABASE %I OWNER datum_owner', :'dbname')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'dbname')\gexec

-- Cloned case databases inherit the template limit; two pools x max_connections(2) fit in 8.
ALTER DATABASE :"template" WITH IS_TEMPLATE true;
ALTER DATABASE :"template" CONNECTION LIMIT 8;
ALTER DATABASE :"dbname" CONNECTION LIMIT 20;
