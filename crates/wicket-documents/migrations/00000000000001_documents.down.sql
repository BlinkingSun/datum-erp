-- Reverse 0001_documents.

DROP TRIGGER IF EXISTS link_immutable ON documents.link;
DROP TRIGGER IF EXISTS attachment_immutable ON documents.attachment;
DROP TRIGGER IF EXISTS blob_immutable ON documents.blob;
DROP TRIGGER IF EXISTS revision_guard ON documents.revision;
DROP TRIGGER IF EXISTS document_guard ON documents.document;

DROP FUNCTION IF EXISTS documents.refuse_mutation();
DROP FUNCTION IF EXISTS documents.revision_guard();
DROP FUNCTION IF EXISTS documents.document_guard();

DROP TABLE IF EXISTS documents.link;
DROP TABLE IF EXISTS documents.attachment;
DROP TABLE IF EXISTS documents.blob;
DROP TABLE IF EXISTS documents.revision;
DROP TABLE IF EXISTS documents.document;

DELETE FROM wicket.schema_class WHERE nspname = 'documents';

DROP SCHEMA IF EXISTS documents;
