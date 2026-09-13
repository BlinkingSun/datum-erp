-- 0001_documents: controlled document masters, revisions, blobs, links.
-- Schema class app (CONTRACT §8a): no DELETE, no ON DELETE CASCADE.
-- Created by datum_migrate so CREATE TABLE can fire audit.attach_new_tables;
-- tables are then owned by datum_owner. Reversible.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'documents.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS documents AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA documents FROM PUBLIC;
GRANT USAGE ON SCHEMA documents TO datum_app;
GRANT USAGE, CREATE ON SCHEMA documents TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('documents', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA documents
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA documents
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA documents
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

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
ALTER TABLE documents.document OWNER TO datum_owner;

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
ALTER TABLE documents.revision OWNER TO datum_owner;

CREATE INDEX revision_document ON documents.revision (document_id);

CREATE TABLE documents.blob (
  hash        bytea        PRIMARY KEY CHECK (octet_length(hash) = 32),
  byte_size   bigint       NOT NULL CHECK (byte_size >= 0),
  stored_at   timestamptz  NOT NULL DEFAULT pg_catalog.now()
);
ALTER TABLE documents.blob OWNER TO datum_owner;

CREATE TABLE documents.attachment (
  attachment_id uuid        PRIMARY KEY,
  revision_id   uuid        NOT NULL REFERENCES documents.revision (revision_id),
  blob_hash     bytea       NOT NULL REFERENCES documents.blob (hash),
  filename      text        NOT NULL CHECK (filename <> ''),
  media_type    text        NOT NULL CHECK (media_type <> ''),
  byte_size     bigint      NOT NULL CHECK (byte_size >= 0)
);
ALTER TABLE documents.attachment OWNER TO datum_owner;

CREATE TABLE documents.link (
  link_id       uuid        PRIMARY KEY,
  revision_id   uuid        NOT NULL REFERENCES documents.revision (revision_id),
  entity        text        NOT NULL CHECK (entity <> ''),
  record_id     uuid        NOT NULL,
  kind          text        NOT NULL CHECK (kind <> '')
);
ALTER TABLE documents.link OWNER TO datum_owner;

-- Live machine state for doc_type = 'document'. search_path is pg_catalog, pg_temp
-- (R-2s-8), so the sm.instance identifier is qualified through format('%I').
CREATE FUNCTION documents.live_machine_state(p_doc_id uuid) RETURNS text
LANGUAGE plpgsql VOLATILE SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE
  live_state text;
BEGIN
  EXECUTE format(
    'SELECT state FROM %I.%I WHERE doc_type = $1 AND doc_id = $2',
    'sm',
    'instance'
  )
  INTO live_state
  USING 'document', p_doc_id;
  RETURN live_state;
END
$fn$;
ALTER FUNCTION documents.live_machine_state(uuid) OWNER TO datum_owner;

CREATE FUNCTION documents.document_guard() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF NEW.document_id IS DISTINCT FROM OLD.document_id
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.number IS DISTINCT FROM OLD.number
       OR NEW.title IS DISTINCT FROM OLD.title
       OR NEW.retention_class IS DISTINCT FROM OLD.retention_class
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
      RAISE EXCEPTION 'documents.document is immutable except status and legal_hold'
        USING ERRCODE = 'P0001';
    END IF;
    IF NEW.status IS DISTINCT FROM OLD.status THEN
      IF NEW.status = 'Obsolete' AND (NEW.legal_hold OR OLD.legal_hold) THEN
        RAISE EXCEPTION 'legal hold refuses Obsolete'
          USING ERRCODE = 'P0001';
      END IF;
      IF NEW.status IS DISTINCT FROM documents.live_machine_state(NEW.document_id) THEN
        RAISE EXCEPTION 'documents.document.status must match the live machine instance'
          USING ERRCODE = 'P0001';
      END IF;
    END IF;
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION documents.document_guard() OWNER TO datum_owner;

CREATE TRIGGER document_guard
  BEFORE UPDATE ON documents.document
  FOR EACH ROW EXECUTE FUNCTION documents.document_guard();

CREATE FUNCTION documents.revision_guard() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE
  held boolean;
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF NEW.revision_id IS DISTINCT FROM OLD.revision_id
       OR NEW.document_id IS DISTINCT FROM OLD.document_id
       OR NEW.label IS DISTINCT FROM OLD.label
       OR NEW.supersedes_revision_id IS DISTINCT FROM OLD.supersedes_revision_id
       OR NEW.content_manifest IS DISTINCT FROM OLD.content_manifest
       OR NEW.retention_class IS DISTINCT FROM OLD.retention_class
       OR NEW.effective_from IS DISTINCT FROM OLD.effective_from
       OR NEW.effective_until IS DISTINCT FROM OLD.effective_until
       OR NEW.effective_from_precision IS DISTINCT FROM OLD.effective_from_precision
       OR NEW.effective_until_precision IS DISTINCT FROM OLD.effective_until_precision
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
      RAISE EXCEPTION 'documents.revision is immutable except status'
        USING ERRCODE = 'P0001';
    END IF;
    IF NEW.status IS DISTINCT FROM OLD.status THEN
      IF NEW.status = 'Obsolete' THEN
        SELECT d.legal_hold INTO held
          FROM documents.document d
         WHERE d.document_id = NEW.document_id;
        IF COALESCE(held, false) THEN
          RAISE EXCEPTION 'legal hold refuses Obsolete'
            USING ERRCODE = 'P0001';
        END IF;
      END IF;
      IF NEW.status IS DISTINCT FROM documents.live_machine_state(NEW.document_id) THEN
        RAISE EXCEPTION 'documents.revision.status must match the live machine instance'
          USING ERRCODE = 'P0001';
      END IF;
    END IF;
  END IF;
  RETURN NEW;
END
$fn$;
ALTER FUNCTION documents.revision_guard() OWNER TO datum_owner;

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
ALTER FUNCTION documents.refuse_mutation() OWNER TO datum_owner;

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

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA documents TO datum_app;
REVOKE DELETE ON ALL TABLES IN SCHEMA documents FROM PUBLIC, datum_app;
REVOKE UPDATE ON TABLE documents.blob FROM PUBLIC, datum_app;
REVOKE UPDATE ON TABLE documents.attachment FROM PUBLIC, datum_app;
REVOKE UPDATE ON TABLE documents.link FROM PUBLIC, datum_app;
