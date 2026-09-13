-- 90-gc.sql — drop orphaned datum_t_* case databases (not run by db-reset; use just db-gc).
-- Requires psql variable gc_minutes (just db-gc sets it from DATUM_DB_GC_MIN, default 60).
-- -v template=… -v dbname=… exclude the standing pair (defaults: datum_test_template, datum_test).

\set ON_ERROR_STOP on

\if :{?gc_minutes}
\else
\set gc_minutes 60
\endif
\if :{?template}
\else
\set template datum_test_template
\endif
\if :{?dbname}
\else
\set dbname datum_test
\endif

CREATE TEMP TABLE datum_gc_params (thresh_minutes integer NOT NULL);
INSERT INTO datum_gc_params (thresh_minutes) VALUES (:gc_minutes);

CREATE TEMP TABLE datum_gc_report (
  action text NOT NULL,
  datname name NOT NULL,
  detail text NOT NULL
);

INSERT INTO datum_gc_report (action, datname, detail)
WITH params AS (
  SELECT thresh_minutes FROM datum_gc_params
),
candidates AS (
  SELECT db.datname, d.description
  FROM pg_catalog.pg_database db
  LEFT JOIN pg_catalog.pg_shdescription d
    ON d.objoid = db.oid
   AND d.classoid = 'pg_catalog.pg_database'::pg_catalog.regclass
  CROSS JOIN params p
  WHERE db.datname LIKE 'datum\_t\_%' ESCAPE '\'
    AND db.datname NOT IN (:'dbname', :'template')
),
parsed AS (
  SELECT
    c.datname,
    COALESCE(
      CASE
        WHEN c.description LIKE 'datum.harness_created_at=%' THEN
          (regexp_match(c.description, '^datum\.harness_created_at=(.+)$'))[1]::timestamptz
        ELSE NULL
      END,
      CASE
        WHEN (regexp_match(c.datname, '_([0-9a-f]+)_([0-9a-f]+)$'))[2] IS NOT NULL THEN
          to_timestamp(
            (
              'x' || (regexp_match(c.datname, '_([0-9a-f]+)_([0-9a-f]+)$'))[2]
            )::bit(64)::bigint::double precision / 1000000000.0
          )
        ELSE NULL
      END
    ) AS created_at,
    p.thresh_minutes
  FROM candidates c
  CROSS JOIN params p
),
scored AS (
  SELECT
    p.datname,
    p.created_at,
    p.thresh_minutes,
    EXTRACT(EPOCH FROM (now() - p.created_at)) / 60.0 AS age_minutes,
    (
      SELECT count(*)::integer
      FROM pg_catalog.pg_stat_activity a
      WHERE a.datname = p.datname
        AND a.pid <> pg_catalog.pg_backend_pid()
    ) AS conns
  FROM parsed p
)
SELECT
  CASE
    WHEN created_at IS NULL THEN 'skipped'
    WHEN conns > 0 THEN 'skipped'
    WHEN age_minutes < thresh_minutes THEN 'skipped'
    ELSE 'to_drop'
  END AS action,
  datname,
  CASE
    WHEN created_at IS NULL THEN 'no parseable creation time'
    WHEN conns > 0 THEN format(
      '%s active connection(s), created %s ago (%s min)',
      conns,
      created_at,
      round(age_minutes::numeric, 1)
    )
    WHEN age_minutes < thresh_minutes THEN format(
      'younger than %s min (%s min old)',
      thresh_minutes,
      round(age_minutes::numeric, 1)
    )
    ELSE format(
      '%s min old (threshold %s min)',
      round(age_minutes::numeric, 1),
      thresh_minutes
    )
  END AS detail
FROM scored
ORDER BY datname;

SELECT format('DROP DATABASE %I WITH (FORCE)', datname)
FROM datum_gc_report
WHERE action = 'to_drop'
ORDER BY datname
\gexec

UPDATE datum_gc_report SET action = 'dropped' WHERE action = 'to_drop';

\echo 'datum db-gc: dropped'
SELECT datname, detail FROM datum_gc_report WHERE action = 'dropped' ORDER BY datname;

\echo 'datum db-gc: skipped'
SELECT datname, detail FROM datum_gc_report WHERE action = 'skipped' ORDER BY datname;
