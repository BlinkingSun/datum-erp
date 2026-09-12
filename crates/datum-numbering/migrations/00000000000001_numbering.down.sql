-- Reverse of 0001_numbering.

DROP FUNCTION IF EXISTS numbering.next_number(text, text);
DROP FUNCTION IF EXISTS numbering.define(text, text, text);
DROP FUNCTION IF EXISTS numbering.server_now();
DROP FUNCTION IF EXISTS numbering.render(text, bigint, timestamptz);
DROP FUNCTION IF EXISTS numbering.bind_exempt();
DROP FUNCTION IF EXISTS numbering.register_exempt();
DROP TABLE IF EXISTS numbering.counter;
DROP SCHEMA IF EXISTS numbering CASCADE;

DO $c$
BEGIN
  IF to_regclass('datum.schema_class') IS NOT NULL THEN
    DELETE FROM datum.schema_class WHERE nspname = 'numbering';
  END IF;
  IF to_regclass('audit.exempt') IS NOT NULL THEN
    DELETE FROM audit.exempt
     WHERE nspname = 'numbering'
       AND relname = 'counter'
       AND decided_by = 'datum-numbering/0001';
  END IF;
END
$c$;
