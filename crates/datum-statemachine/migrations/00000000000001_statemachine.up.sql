-- 0001_statemachine: declarative machines, edges, and per-document instance state.
-- Schema `sm` is schema-class `app` (CONTRACT §8a): no DELETE, no ON DELETE CASCADE.
-- Tables owned by datum_owner. Reversible.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'statemachine.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS sm AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA sm FROM PUBLIC;
GRANT USAGE ON SCHEMA sm TO datum_app;
GRANT USAGE, CREATE ON SCHEMA sm TO datum_migrate, datum_owner;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA sm
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA sm
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA sm
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

INSERT INTO datum.schema_class (nspname, class)
VALUES ('sm', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

CREATE TABLE sm.machine (
    id         uuid    PRIMARY KEY,
    doc_type   text    NOT NULL CHECK (doc_type <> ''),
    regulated  boolean NOT NULL,
    UNIQUE (doc_type)
);
ALTER TABLE sm.machine OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE ON TABLE sm.machine TO datum_app;

CREATE TABLE sm.state (
    machine_id uuid NOT NULL REFERENCES sm.machine (id),
    name       text NOT NULL CHECK (name <> ''),
    PRIMARY KEY (machine_id, name)
);
ALTER TABLE sm.state OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE ON TABLE sm.state TO datum_app;

CREATE TABLE sm.edge (
    machine_id          uuid    NOT NULL REFERENCES sm.machine (id),
    name                text    NOT NULL CHECK (name <> ''),
    from_state          text    NOT NULL,
    to_state            text    NOT NULL,
    permission          text    NOT NULL CHECK (permission <> ''),
    signature_kind      text    NOT NULL CHECK (signature_kind IN ('required', 'not_required')),
    meaning             text        NULL,
    sig_permission      text        NULL,
    not_required_reason text        NULL,
    hooks_allowed       boolean NOT NULL,
    PRIMARY KEY (machine_id, name),
    FOREIGN KEY (machine_id, from_state) REFERENCES sm.state (machine_id, name),
    FOREIGN KEY (machine_id, to_state)   REFERENCES sm.state (machine_id, name),
    CHECK (
        (signature_kind = 'required'
         AND meaning IS NOT NULL AND meaning <> ''
         AND sig_permission IS NOT NULL AND sig_permission <> ''
         AND not_required_reason IS NULL)
        OR
        (signature_kind = 'not_required'
         AND not_required_reason IS NOT NULL AND not_required_reason <> ''
         AND meaning IS NULL
         AND sig_permission IS NULL)
    )
);
ALTER TABLE sm.edge OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE ON TABLE sm.edge TO datum_app;

CREATE TABLE sm.instance (
    doc_type   text        NOT NULL CHECK (doc_type <> ''),
    doc_id     uuid        NOT NULL,
    machine_id uuid        NOT NULL REFERENCES sm.machine (id),
    state      text        NOT NULL,
    version    bigint      NOT NULL CHECK (version >= 1),
    entered_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (doc_type, doc_id),
    FOREIGN KEY (machine_id, state) REFERENCES sm.state (machine_id, name)
);
ALTER TABLE sm.instance OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE ON TABLE sm.instance TO datum_app;

ALTER SCHEMA sm OWNER TO datum_owner;
