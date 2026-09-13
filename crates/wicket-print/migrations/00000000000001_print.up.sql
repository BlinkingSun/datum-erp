-- 0001_print: templates, render audit log, installation profile stamp. Schema class app.

SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'print.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS print AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA print FROM PUBLIC;
GRANT USAGE ON SCHEMA print TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA print TO wicket_migrate, wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('print', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA print
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA print
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA print
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

-- Boot stamp: composition root writes the installation profile id here.
-- `wicket.config_version` is spec_version (invariant 17), not the profile id.
CREATE TABLE print.install (
  singleton   char(1) PRIMARY KEY CHECK (singleton = 'x'),
  profile_id  text    NOT NULL CHECK (profile_id IN ('plain-shop', 'regulated-device'))
);
ALTER TABLE print.install OWNER TO wicket_owner;

CREATE TABLE print.template (
  template_id       text        NOT NULL,
  version           integer     NOT NULL CHECK (version >= 1),
  semantic_version  text        NOT NULL CHECK (semantic_version <> ''),
  body              text        NOT NULL,
  body_hash         bytea       NOT NULL CHECK (octet_length(body_hash) = 32),
  effective_from    timestamptz NOT NULL DEFAULT pg_catalog.now(),
  effective_until   timestamptz     NULL,
  PRIMARY KEY (template_id, version),
  CONSTRAINT template_effective_range CHECK (
    effective_until IS NULL OR effective_until > effective_from
  )
);
ALTER TABLE print.template OWNER TO wicket_owner;

CREATE TABLE print.render_log (
  render_id           uuid        PRIMARY KEY,
  record_table        text        NOT NULL,
  record_id           uuid        NOT NULL,
  record_version      bigint      NOT NULL,
  record_content_hash bytea       NOT NULL CHECK (octet_length(record_content_hash) = 32),
  template_id         text        NOT NULL,
  template_version    integer     NOT NULL,
  renderer_version    text        NOT NULL,
  output_format       text        NOT NULL CHECK (output_format IN ('html', 'pdf')),
  output_hash         bytea       NOT NULL CHECK (octet_length(output_hash) = 32),
  blob_hash           bytea           NULL CHECK (blob_hash IS NULL OR octet_length(blob_hash) = 32),
  created_at          timestamptz NOT NULL DEFAULT pg_catalog.now(),
  FOREIGN KEY (template_id, template_version)
    REFERENCES print.template (template_id, version)
);
ALTER TABLE print.render_log OWNER TO wicket_owner;

CREATE INDEX render_log_record ON print.render_log (record_table, record_id, record_version);

SELECT audit.attach('print.install'::regclass);
SELECT audit.attach('print.template'::regclass);
SELECT audit.attach('print.render_log'::regclass);

ALTER SCHEMA print OWNER TO wicket_owner;
