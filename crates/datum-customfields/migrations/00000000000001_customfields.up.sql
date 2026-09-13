-- 0001_customfields: typed extension fields (schema class app).
-- Reversible. Audited via audit.attach. No jsonb value storage.

CREATE SCHEMA IF NOT EXISTS customfields AUTHORIZATION datum_owner;

REVOKE ALL ON SCHEMA customfields FROM PUBLIC;
GRANT USAGE ON SCHEMA customfields TO datum_app, datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('customfields', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA customfields
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA customfields
  GRANT USAGE ON SEQUENCES TO datum_app;

CREATE TABLE customfields.definition (
  definition_id     uuid        NOT NULL,
  version             integer     NOT NULL CHECK (version >= 1),
  entity              text        NOT NULL CHECK (entity <> ''),
  key                 text        NOT NULL CHECK (key <> ''),
  field_type          text        NOT NULL CHECK (field_type IN (
                        'string', 'text', 'integer', 'decimal', 'bool', 'date', 'enum', 'reference'
                      )),
  label               text        NOT NULL,
  validation_rule     text        NOT NULL DEFAULT '',
  required            boolean     NOT NULL DEFAULT false,
  indexed             boolean     NOT NULL DEFAULT false,
  owner_module        text        NOT NULL CHECK (owner_module <> ''),
  status              text        NOT NULL CHECK (status IN ('active', 'retired')),
  effective_from      timestamptz NOT NULL DEFAULT pg_catalog.now(),
  effective_to        timestamptz     NULL,
  PRIMARY KEY (definition_id, version),
  CONSTRAINT definition_effective_range
    CHECK (effective_to IS NULL OR effective_to >= effective_from)
);

CREATE UNIQUE INDEX definition_active_entity_key
  ON customfields.definition (entity, key)
  WHERE status = 'active' AND effective_to IS NULL;

ALTER TABLE customfields.definition OWNER TO datum_owner;

CREATE TABLE customfields.value_string (
  definition_id       uuid    NOT NULL,
  record_id           uuid    NOT NULL,
  definition_version  integer NOT NULL,
  value               text    NOT NULL,
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_string OWNER TO datum_owner;

CREATE TABLE customfields.value_text (
  definition_id       uuid    NOT NULL,
  record_id           uuid    NOT NULL,
  definition_version  integer NOT NULL,
  value               text    NOT NULL,
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_text OWNER TO datum_owner;

CREATE TABLE customfields.value_integer (
  definition_id       uuid    NOT NULL,
  record_id           uuid    NOT NULL,
  definition_version  integer NOT NULL,
  value               bigint  NOT NULL,
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_integer OWNER TO datum_owner;

CREATE TABLE customfields.value_decimal (
  definition_id       uuid           NOT NULL,
  record_id           uuid           NOT NULL,
  definition_version  integer        NOT NULL,
  value               numeric(38, 18) NOT NULL,
  scale               smallint       NOT NULL CHECK (scale BETWEEN 0 AND 18),
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_decimal OWNER TO datum_owner;

CREATE TABLE customfields.value_bool (
  definition_id       uuid    NOT NULL,
  record_id           uuid    NOT NULL,
  definition_version  integer NOT NULL,
  value               boolean NOT NULL,
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_bool OWNER TO datum_owner;

CREATE TABLE customfields.value_date (
  definition_id       uuid    NOT NULL,
  record_id           uuid    NOT NULL,
  definition_version  integer NOT NULL,
  value               date    NOT NULL,
  precision           text    NOT NULL CHECK (precision IN ('day', 'month', 'year')),
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_date OWNER TO datum_owner;

CREATE TABLE customfields.value_enum (
  definition_id       uuid    NOT NULL,
  record_id           uuid    NOT NULL,
  definition_version  integer NOT NULL,
  value               text    NOT NULL,
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_enum OWNER TO datum_owner;

CREATE TABLE customfields.value_reference (
  definition_id       uuid    NOT NULL,
  record_id           uuid    NOT NULL,
  definition_version  integer NOT NULL,
  ref_entity          text    NOT NULL CHECK (ref_entity <> ''),
  ref_id              uuid    NOT NULL,
  PRIMARY KEY (definition_id, record_id),
  FOREIGN KEY (definition_id, definition_version)
    REFERENCES customfields.definition (definition_id, version)
);
ALTER TABLE customfields.value_reference OWNER TO datum_owner;

SELECT audit.attach('customfields.definition'::regclass);
SELECT audit.attach('customfields.value_string'::regclass);
SELECT audit.attach('customfields.value_text'::regclass);
SELECT audit.attach('customfields.value_integer'::regclass);
SELECT audit.attach('customfields.value_decimal'::regclass);
SELECT audit.attach('customfields.value_bool'::regclass);
SELECT audit.attach('customfields.value_date'::regclass);
SELECT audit.attach('customfields.value_enum'::regclass);
SELECT audit.attach('customfields.value_reference'::regclass);

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA customfields TO datum_app;
REVOKE DELETE ON ALL TABLES IN SCHEMA customfields FROM PUBLIC, datum_app;
