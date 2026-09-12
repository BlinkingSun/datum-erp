-- 0001_jobs: durable queue (transient working state + audited run history).
-- jobs.job / jobs.schedule live in schema transient (D-W1-2: DELETE expected, no audit).
-- jobs.run_log lives in schema app (append-only, audited). Reversible.
-- Tables are owned by datum_owner (NOLOGIN). No ON DELETE CASCADE.

CREATE TABLE transient.job (
    id             uuid        PRIMARY KEY,
    kind           text        NOT NULL CHECK (kind <> ''),
    payload        jsonb       NOT NULL,
    state          text        NOT NULL CHECK (state IN (
                       'queued', 'running', 'succeeded', 'failed', 'cancelled'
                   )),
    attempts       integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts   integer     NOT NULL DEFAULT 25 CHECK (max_attempts >= 1),
    run_after      timestamptz NOT NULL DEFAULT now(),
    locked_by      text            NULL,
    locked_at      timestamptz     NULL,
    progress_pct   smallint    NOT NULL DEFAULT 0 CHECK (progress_pct >= 0 AND progress_pct <= 100),
    progress_note  text            NULL,
    result         jsonb           NULL,
    last_error     text            NULL,
    requested_by   uuid        NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE transient.job OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE transient.job TO datum_app;

CREATE INDEX job_due ON transient.job (run_after, created_at)
    WHERE state = 'queued';
CREATE INDEX job_running_lock ON transient.job (locked_at)
    WHERE state = 'running';

CREATE TABLE app.run_log (
    id          uuid        PRIMARY KEY,
    job_id      uuid        NOT NULL,
    attempt     integer     NOT NULL CHECK (attempt > 0),
    started_at  timestamptz NOT NULL,
    finished_at timestamptz     NULL,
    outcome     text            NULL CHECK (outcome IS NULL OR outcome IN (
                    'succeeded', 'failed', 'cancelled'
                )),
    error       text            NULL
);
ALTER TABLE app.run_log OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE ON TABLE app.run_log TO datum_app;

CREATE INDEX run_log_job ON app.run_log (job_id, attempt);

CREATE TABLE transient.schedule (
    id           uuid        PRIMARY KEY,
    kind         text        NOT NULL CHECK (kind <> ''),
    cron_expr    text        NOT NULL CHECK (cron_expr <> ''),
    payload      jsonb       NOT NULL DEFAULT '{}'::jsonb,
    enabled      boolean     NOT NULL DEFAULT true,
    requested_by uuid        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE transient.schedule OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE transient.schedule TO datum_app;

-- Event trigger audit_attach skips schema transient and attaches app.run_log.
-- Explicit attach is idempotent if the trigger already fired.
SELECT audit.attach('app.run_log'::regclass);

-- Invoker: datum_app holds DELETE on transient.job (D-W1-2). Kind name stays jobs.prune_finished.
CREATE FUNCTION transient.prune_finished(older_than_days integer) RETURNS bigint
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $fn$
DECLARE n bigint;
BEGIN
  IF older_than_days < 1 THEN
    RAISE EXCEPTION 'jobs.prune_finished: older_than_days must be >= 1' USING ERRCODE = '22023';
  END IF;
  DELETE FROM transient.job
   WHERE state IN ('succeeded', 'failed', 'cancelled')
     AND created_at < now() - make_interval(days => older_than_days);
  GET DIAGNOSTICS n = ROW_COUNT;
  RETURN n;
END
$fn$;
ALTER FUNCTION transient.prune_finished(integer) OWNER TO datum_owner;
REVOKE ALL ON FUNCTION transient.prune_finished(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION transient.prune_finished(integer) TO datum_app, datum_migrate, datum_owner;
