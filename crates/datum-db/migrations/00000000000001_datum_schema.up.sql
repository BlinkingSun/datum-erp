-- 0001_datum_schema: schema datum (class app) and schema_history.
-- Audited once datum-audit exists. Reversible.
-- Requires CREATE on the database (production: inherited from datum_owner).
-- Tables are owned by datum_owner (NOLOGIN), never the login that ran DDL.
-- SET LOCAL ROLE is not used here: sqlx records the version in the same
-- transaction, and that INSERT must stay `datum_migrate`.

CREATE SCHEMA IF NOT EXISTS datum AUTHORIZATION datum_owner;

REVOKE ALL ON SCHEMA datum FROM PUBLIC;
GRANT USAGE ON SCHEMA datum TO datum_app;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA datum
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

CREATE TABLE datum.schema_history (
    crate       text        NOT NULL,
    version     bigint      NOT NULL,
    applied_at  timestamptz NOT NULL DEFAULT now(),
    app_version text        NOT NULL,
    PRIMARY KEY (crate, version)
);
ALTER TABLE datum.schema_history OWNER TO datum_owner;

CREATE TABLE datum.schema_class (
    nspname name PRIMARY KEY,
    class   text NOT NULL CHECK (class IN ('app', 'transient', 'audit'))
);
ALTER TABLE datum.schema_class OWNER TO datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES
    ('datum',     'app'),
    ('app',       'app'),
    ('transient', 'transient'),
    ('audit',     'audit')
ON CONFLICT (nspname) DO NOTHING;

GRANT SELECT, INSERT, UPDATE ON datum.schema_history TO datum_app;
GRANT SELECT, INSERT, UPDATE ON datum.schema_class TO datum_app;
