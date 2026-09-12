-- Reverse of 0001_events. Drop delivery first (FK to app.event). No CASCADE.

DROP INDEX IF EXISTS transient.delivery_due;
DROP TABLE IF EXISTS transient.delivery;

DROP TABLE IF EXISTS app.subscription;

DROP INDEX IF EXISTS app.event_occurred_at;
DROP INDEX IF EXISTS app.event_name;
DROP TABLE IF EXISTS app.event;
