-- 0001_numbering: transactional gap-free counters (D3 §8) and exemption.
-- Schema `numbering` is schema-class `app` (CONTRACT §8a): no DELETE.
-- Reversible. Tables owned by wicket_owner.

CREATE SCHEMA IF NOT EXISTS numbering AUTHORIZATION wicket_owner;

REVOKE ALL ON SCHEMA numbering FROM PUBLIC;
GRANT USAGE ON SCHEMA numbering TO wicket_app, wicket_migrate, wicket_owner;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA numbering
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA numbering
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('numbering', 'app')
ON CONFLICT (nspname) DO NOTHING;

CREATE FUNCTION numbering.register_exempt() RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  IF to_regclass('audit.exempt') IS NULL THEN
    RAISE EXCEPTION 'numbering: audit.exempt is missing; migrate wicket-audit first';
  END IF;
  INSERT INTO audit.exempt (relid, nspname, relname, reason, decided_by)
  VALUES (
    NULL,
    'numbering',
    'counter',
    'every allocation would otherwise write an audit row whose entire evidentiary content is "a counter went up," while the allocation is already evidenced by the audited document that carries the number.',
    'wicket-numbering/0001'
  )
  ON CONFLICT (nspname, relname) DO UPDATE
    SET reason = EXCLUDED.reason,
        decided_by = EXCLUDED.decided_by;
END
$fn$;
ALTER FUNCTION numbering.register_exempt() OWNER TO wicket_owner;
REVOKE ALL ON FUNCTION numbering.register_exempt() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION numbering.register_exempt() TO wicket_migrate, wicket_owner;
SELECT numbering.register_exempt();

CREATE TABLE numbering.counter (
  doc_type   text   NOT NULL CHECK (doc_type <> ''),
  period_key text   NOT NULL DEFAULT '',
  next_value bigint NOT NULL CHECK (next_value >= 1),
  format     text   NOT NULL CHECK (format <> ''),
  PRIMARY KEY (doc_type, period_key)
);
ALTER TABLE numbering.counter OWNER TO wicket_owner;

COMMENT ON TABLE numbering.counter IS
  'Gap-free document counters. Allocated by UPDATE ... RETURNING in the caller transaction (D3 §8). Cancellation is a document status; this crate does not void or reuse a committed number.';

GRANT SELECT, INSERT, UPDATE ON numbering.counter TO wicket_app;

CREATE FUNCTION numbering.bind_exempt() RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $fn$
BEGIN
  UPDATE audit.exempt
     SET relid = 'numbering.counter'::regclass
   WHERE nspname = 'numbering' AND relname = 'counter';
END
$fn$;
ALTER FUNCTION numbering.bind_exempt() OWNER TO wicket_owner;
REVOKE ALL ON FUNCTION numbering.bind_exempt() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION numbering.bind_exempt() TO wicket_migrate, wicket_owner;
SELECT numbering.bind_exempt();

CREATE FUNCTION numbering.render(fmt text, allocated bigint, ts timestamptz)
RETURNS text
LANGUAGE plpgsql IMMUTABLE SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE
  result text;
  m text[];
  token text;
  width int;
  digits text;
BEGIN
  IF fmt IS NULL OR fmt = '' THEN
    RAISE EXCEPTION 'numbering: empty format' USING ERRCODE = '22023';
  END IF;
  IF allocated IS NULL OR allocated < 0 THEN
    RAISE EXCEPTION 'numbering: padding overflow allocated=% width=%', allocated, 0
      USING ERRCODE = '22003';
  END IF;

  result := fmt;
  result := replace(result, '{yyyy}', to_char(ts AT TIME ZONE 'UTC', 'YYYY'));
  result := replace(result, '{yy}', to_char(ts AT TIME ZONE 'UTC', 'YY'));
  result := replace(result, '{mm}', to_char(ts AT TIME ZONE 'UTC', 'MM'));

  LOOP
    m := regexp_match(result, '\{(0+)\}');
    EXIT WHEN m IS NULL;
    token := '{' || m[1] || '}';
    width := length(m[1]);
    digits := allocated::text;
    IF length(digits) > width THEN
      RAISE EXCEPTION 'numbering: padding overflow allocated=% width=%', allocated, width
        USING ERRCODE = '22003';
    END IF;
    result := replace(result, token, lpad(digits, width, '0'));
  END LOOP;

  IF result ~ '\{[^}]*\}' THEN
    RAISE EXCEPTION 'numbering: invalid template leftover token in %', fmt
      USING ERRCODE = '22023';
  END IF;
  RETURN result;
END
$fn$;
ALTER FUNCTION numbering.render(text, bigint, timestamptz) OWNER TO wicket_owner;

CREATE FUNCTION numbering.define(p_doc_type text, p_format text, p_reset text)
RETURNS text
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE
  ts timestamptz := now();
  period text;
  existing text;
BEGIN
  IF p_doc_type IS NULL OR p_doc_type = '' THEN
    RAISE EXCEPTION 'numbering: empty doc_type' USING ERRCODE = '22023';
  END IF;
  IF p_format IS NULL OR p_format = '' THEN
    RAISE EXCEPTION 'numbering: empty format' USING ERRCODE = '22023';
  END IF;
  IF p_reset NOT IN ('never', 'yearly', 'monthly') THEN
    RAISE EXCEPTION 'numbering: invalid reset_policy %', p_reset USING ERRCODE = '22023';
  END IF;

  PERFORM numbering.render(p_format, 1, ts);

  period := CASE p_reset
    WHEN 'never' THEN ''
    WHEN 'yearly' THEN to_char(ts AT TIME ZONE 'UTC', 'YYYY')
    WHEN 'monthly' THEN to_char(ts AT TIME ZONE 'UTC', 'YYYY-MM')
  END;

  SELECT c.format INTO existing
    FROM numbering.counter c
   WHERE c.doc_type = p_doc_type
   LIMIT 1;
  IF existing IS NOT NULL AND existing IS DISTINCT FROM p_format THEN
    RAISE EXCEPTION 'numbering: sequence % already defined with a different format', p_doc_type
      USING ERRCODE = '22023';
  END IF;

  INSERT INTO numbering.counter (doc_type, period_key, next_value, format)
  VALUES (p_doc_type, period, 1, p_format)
  ON CONFLICT (doc_type, period_key) DO NOTHING;

  PERFORM pg_catalog.set_config('numbering.out', p_doc_type, true);
  RETURN p_doc_type;
END
$fn$;
ALTER FUNCTION numbering.define(text, text, text) OWNER TO wicket_owner;

CREATE FUNCTION numbering.next_number(p_doc_type text, p_reset text)
RETURNS text
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE
  ts timestamptz := now();
  period text;
  fmt text;
  allocated bigint;
  rendered text;
BEGIN
  IF p_doc_type IS NULL OR p_doc_type = '' THEN
    RAISE EXCEPTION 'numbering: empty doc_type' USING ERRCODE = '22023';
  END IF;
  IF p_reset NOT IN ('never', 'yearly', 'monthly') THEN
    RAISE EXCEPTION 'numbering: invalid reset_policy %', p_reset USING ERRCODE = '22023';
  END IF;

  SELECT c.format INTO fmt
    FROM numbering.counter c
   WHERE c.doc_type = p_doc_type
   LIMIT 1;
  IF fmt IS NULL THEN
    RAISE EXCEPTION 'numbering: unknown sequence %', p_doc_type USING ERRCODE = '22023';
  END IF;

  period := CASE p_reset
    WHEN 'never' THEN ''
    WHEN 'yearly' THEN to_char(ts AT TIME ZONE 'UTC', 'YYYY')
    WHEN 'monthly' THEN to_char(ts AT TIME ZONE 'UTC', 'YYYY-MM')
  END;

  LOOP
    INSERT INTO numbering.counter (doc_type, period_key, next_value, format)
    VALUES (p_doc_type, period, 1, fmt)
    ON CONFLICT (doc_type, period_key) DO NOTHING;

    UPDATE numbering.counter AS c
       SET next_value = c.next_value + 1
     WHERE c.doc_type = p_doc_type AND c.period_key = period
     RETURNING c.next_value - 1 INTO allocated;

    EXIT WHEN FOUND;
  END LOOP;

  rendered := numbering.render(fmt, allocated, ts);
  PERFORM pg_catalog.set_config('numbering.out', rendered, true);
  RETURN rendered;
END
$fn$;
ALTER FUNCTION numbering.next_number(text, text) OWNER TO wicket_owner;

CREATE FUNCTION numbering.server_now() RETURNS text
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE t text;
BEGIN
  t := to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS.US');
  PERFORM pg_catalog.set_config('numbering.out', t, true);
  RETURN t;
END
$fn$;
ALTER FUNCTION numbering.server_now() OWNER TO wicket_owner;

REVOKE ALL ON FUNCTION numbering.render(text, bigint, timestamptz) FROM PUBLIC;
REVOKE ALL ON FUNCTION numbering.define(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION numbering.next_number(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION numbering.server_now() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION numbering.render(text, bigint, timestamptz)
  TO wicket_app, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION numbering.define(text, text, text)
  TO wicket_app, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION numbering.next_number(text, text)
  TO wicket_app, wicket_migrate, wicket_owner;
GRANT EXECUTE ON FUNCTION numbering.server_now()
  TO wicket_app, wicket_migrate, wicket_owner;
