-- 0001_events: transactional outbox.
-- events.event (logical) lives in schema app: append-only, audited, never deleted (D-W1-2).
-- events.subscription lives in schema app.
-- events.delivery lives in schema transient: deleted after delivery and retention.
-- Tables are owned by wicket_owner (NOLOGIN). No ON DELETE CASCADE.

CREATE TABLE app.event (
    id           uuid        PRIMARY KEY,
    name         text        NOT NULL CHECK (name <> ''),
    version      smallint    NOT NULL CHECK (version >= 1),
    occurred_at  timestamptz NOT NULL,
    actor_id     uuid        NOT NULL,
    source_kind  text        NOT NULL CHECK (source_kind <> ''),
    doc_type     text            NULL,
    doc_id       uuid            NULL,
    payload      jsonb       NOT NULL,
    tx_xid       xid8        NOT NULL
);
ALTER TABLE app.event OWNER TO wicket_owner;
GRANT SELECT, INSERT, UPDATE ON TABLE app.event TO wicket_app;

CREATE INDEX event_name ON app.event (name);
CREATE INDEX event_occurred_at ON app.event (occurred_at);

CREATE TABLE app.subscription (
    subscriber text NOT NULL CHECK (subscriber <> ''),
    name       text NOT NULL CHECK (name <> ''),
    enabled    bool NOT NULL DEFAULT true,
    PRIMARY KEY (subscriber, name)
);
ALTER TABLE app.subscription OWNER TO wicket_owner;
GRANT SELECT, INSERT, UPDATE ON TABLE app.subscription TO wicket_app;

CREATE TABLE transient.delivery (
    event_id        uuid        NOT NULL REFERENCES app.event (id),
    subscriber      text        NOT NULL CHECK (subscriber <> ''),
    attempts        integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    delivered_at    timestamptz     NULL,
    last_error      text            NULL,
    PRIMARY KEY (event_id, subscriber)
);
ALTER TABLE transient.delivery OWNER TO wicket_owner;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE transient.delivery TO wicket_app;

CREATE INDEX delivery_due ON transient.delivery (next_attempt_at)
    WHERE delivered_at IS NULL;
