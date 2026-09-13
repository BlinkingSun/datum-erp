-- 0001_documents: controlled document masters, revisions, blobs, links.
-- Schema class app (CONTRACT §8a): no DELETE, no ON DELETE CASCADE.
-- Created by wicket_migrate so CREATE TABLE can fire audit.attach_new_tables;
-- tables are then owned by wicket_owner. Reversible.

SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'documents.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS documents AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA documents FROM PUBLIC;
GRANT USAGE ON SCHEMA documents TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA documents TO wicket_migrate, wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('documents', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA documents
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA documents
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA documents
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

CREATE TABLE documents.document (
  document_id       uuid        PRIMARY KEY,
  kind              text        NOT NULL CHECK (kind <> ''),
  number            text        NOT NULL CHECK (number <> ''),
  title             text        NOT NULL,
  status            text        NOT NULL CHECK (status IN (
                      'Draft', 'InReview', 'Approved', 'Effective',
                      'Superseded', 'Obsolete', 'Void'
                    )),
  retention_class   text        NOT NULL CHECK (retention_class <> ''),
  legal_hold        boolean     NOT NULL DEFAULT false,
  created_at        timestamptz NOT NULL DEFAULT pg_catalog.now(),
  UNIQUE (kind, number)
);
ALTER TABLE documents.document OWNER TO wicket_owner;

CREATE TABLE documents.revision (
  revision_id               uuid        PRIMARY KEY,
  document_id               uuid        NOT NULL REFERENCES documents.document (document_id),
  label                     text        NOT NULL CHECK (label <> ''),
  supersedes_revision_id    uuid            NULL REFERENCES documents.revision (revision_id),
  content_manifest          jsonb       NOT NULL,
  status                    text        NOT NULL CHECK (status IN (
                              'Draft', 'InReview', 'Approved', 'Effective',
                              'Superseded', 'Obsolete', 'Void'
                            )),
  retention_class           text        NOT NULL CHECK (retention_class <> ''),
  effective_from            timestamptz     NULL,
  effective_until           timestamptz     NULL,
  effective_from_precision  text            NULL CHECK (
                              effective_from_precision IS NULL
                              OR effective_from_precision IN ('day', 'month', 'year')
                            ),
  effective_until_precision text            NULL CHECK (
                              effective_until_precision IS NULL
                              OR effective_until_precision IN ('day', 'month', 'year')
                            ),
  created_at                timestamptz NOT NULL DEFAULT pg_catalog.now(),
  UNIQUE (document_id, label),
  CONSTRAINT revision_effective_range CHECK (
    effective_until IS NULL
    OR effective_from IS NULL
    OR effective_until > effective_from
  ),
  CONSTRAINT revision_from_precision CHECK (
    (effective_from IS NULL AND effective_from_precision IS NULL)
    OR (effective_from IS NOT NULL AND effective_from_precision IS NOT NULL)
  )
);
ALTER TABLE documents.revision OWNER TO wicket_owner;

CREATE INDEX revision_document ON documents.revision (document_id);

CREATE TABLE documents.blob (
  hash        bytea        PRIMARY KEY CHECK (octet_length(hash) = 32),
  byte_size   bigint       NOT NULL CHECK (byte_size >= 0),
  stored_at   timestamptz  NOT NULL DEFAULT pg_catalog.now()
);
ALTER TABLE documents.blob OWNER TO wicket_owner;

CREATE TABLE documents.attachment (
  attachment_id uuid        PRIMARY KEY,
  revision_id   uuid        NOT NULL REFERENCES documents.revision (revision_id),
  blob_hash     bytea       NOT NULL REFERENCES documents.blob (hash),
  filename      text        NOT NULL CHECK (filename <> ''),
  media_type    text        NOT NULL CHECK (media_type <> ''),
  byte_size     bigint      NOT NULL CHECK (byte_size >= 0)
);
ALTER TABLE documents.attachment OWNER TO wicket_owner;

CREATE TABLE documents.link (
  link_id       uuid        PRIMARY KEY,
  revision_id   uuid        NOT NULL REFERENCES documents.revision (revision_id),
  entity        text        NOT NULL CHECK (entity <> ''),
  record_id     uuid        NOT NULL,
  kind          text        NOT NULL CHECK (kind <> '')
);
ALTER TABLE documents.link OWNER TO wicket_owner;

-- Live status is the machine (wicket_statemachine::current_state on Tx).
-- The status column is the insert-time snapshot and is immutable here: a raw
-- UPDATE cannot skip the graph, and this crate never names another schema.
CREATE FUNCTION documents.document_guard() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF NEW.status IS DISTINCT FROM OLD.status THEN
      IF NEW.status IN ('Obsolete', 'Superseded')
         AND (NEW.legal_hold OR OLD.legal_hold) THEN
        RAISE EXCEPTION 'legal hold refuses Obsolete and Superseded'
          USING ERRCODE = 'P0001';
      END IF;
      RAISE EXCEPTION 'documents.document.status is assigned by the machine only'
        USING ERRCODE = 'P0001';
    END IF;
    IF NEW.document_id IS DISTINCT FROM OLD.document_id
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.number IS DISTINCT FROM OLD.number
       OR NEW.title IS DISTINCT FROM OLD.title
       OR NEW.retention_class IS DISTINCT FROM OLD.retention_class
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
      RAISE EXCEPTION 'documents.document is immutable except legal_hold'
        USING ERRCODE = 'P0001';
    END IF;
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION documents.document_guard() OWNER TO wicket_owner;

CREATE TRIGGER document_guard
  BEFORE UPDATE ON documents.document
  FOR EACH ROW EXECUTE FUNCTION documents.document_guard();

CREATE FUNCTION documents.revision_guard() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE
  held boolean;
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF NEW.status IS DISTINCT FROM OLD.status
       AND NEW.status IN ('Obsolete', 'Superseded') THEN
      SELECT d.legal_hold INTO held
        FROM documents.document d
       WHERE d.document_id = NEW.document_id;
      IF COALESCE(held, false) THEN
        RAISE EXCEPTION 'legal hold refuses Obsolete and Superseded'
          USING ERRCODE = 'P0001';
      END IF;
    END IF;
    RAISE EXCEPTION 'documents.revision is insert-only'
      USING ERRCODE = 'P0001';
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION documents.revision_guard() OWNER TO wicket_owner;

CREATE TRIGGER revision_guard
  BEFORE UPDATE ON documents.revision
  FOR EACH ROW EXECUTE FUNCTION documents.revision_guard();

CREATE FUNCTION documents.refuse_mutation() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  RAISE EXCEPTION 'documents.% is insert-only', TG_TABLE_NAME
    USING ERRCODE = 'P0001';
END
$fn$;
ALTER FUNCTION documents.refuse_mutation() OWNER TO wicket_owner;

CREATE TRIGGER blob_immutable
  BEFORE UPDATE ON documents.blob
  FOR EACH ROW EXECUTE FUNCTION documents.refuse_mutation();

CREATE TRIGGER attachment_immutable
  BEFORE UPDATE ON documents.attachment
  FOR EACH ROW EXECUTE FUNCTION documents.refuse_mutation();

CREATE TRIGGER link_immutable
  BEFORE UPDATE ON documents.link
  FOR EACH ROW EXECUTE FUNCTION documents.refuse_mutation();

SELECT audit.attach('documents.document'::regclass);
SELECT audit.attach('documents.revision'::regclass);
SELECT audit.attach('documents.blob'::regclass);
SELECT audit.attach('documents.attachment'::regclass);
SELECT audit.attach('documents.link'::regclass);

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA documents TO wicket_app;
REVOKE DELETE ON ALL TABLES IN SCHEMA documents FROM PUBLIC, wicket_app;
REVOKE UPDATE ON TABLE documents.blob FROM PUBLIC, wicket_app;
REVOKE UPDATE ON TABLE documents.attachment FROM PUBLIC, wicket_app;
REVOKE UPDATE ON TABLE documents.link FROM PUBLIC, wicket_app;
