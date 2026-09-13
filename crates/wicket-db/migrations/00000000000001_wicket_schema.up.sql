-- 0001_wicket_schema: schema wicket (class app) and schema_history.
-- Audited once wicket-audit exists. Reversible.
-- Requires CREATE on the database (production: inherited from wicket_owner).
-- Tables are owned by wicket_owner (NOLOGIN), never the login that ran DDL.
-- SET LOCAL ROLE is not used here: sqlx records the version in the same
-- transaction, and that INSERT must stay `wicket_migrate`.

CREATE SCHEMA IF NOT EXISTS wicket AUTHORIZATION wicket_owner;

REVOKE ALL ON SCHEMA wicket FROM PUBLIC;
GRANT USAGE ON SCHEMA wicket TO wicket_app;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA wicket
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

CREATE TABLE wicket.schema_history (
    crate       text        NOT NULL,
    version     bigint      NOT NULL,
    applied_at  timestamptz NOT NULL DEFAULT now(),
    app_version text        NOT NULL,
    PRIMARY KEY (crate, version)
);
ALTER TABLE wicket.schema_history OWNER TO wicket_owner;

CREATE TABLE wicket.schema_class (
    nspname name PRIMARY KEY,
    class   text NOT NULL CHECK (class IN ('app', 'transient', 'audit'))
);
ALTER TABLE wicket.schema_class OWNER TO wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES
    ('wicket',     'app'),
    ('app',       'app'),
    ('transient', 'transient'),
    ('audit',     'audit')
ON CONFLICT (nspname) DO NOTHING;

GRANT SELECT, INSERT, UPDATE ON wicket.schema_history TO wicket_app;
GRANT SELECT, INSERT, UPDATE ON wicket.schema_class TO wicket_app;
