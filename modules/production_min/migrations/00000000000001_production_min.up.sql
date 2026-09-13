-- 0001_production_min: work orders and completions in schema-class `app`.
-- Issued-component records are app-class (history). Lots and serials are uuid
-- entity references, never text. Reversible. Audited via audit.attach.
-- No ON DELETE CASCADE. No DELETE on app tables.

SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'production.migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS production_min AUTHORIZATION wicket_migrate;

REVOKE ALL ON SCHEMA production_min FROM PUBLIC;
GRANT USAGE ON SCHEMA production_min TO wicket_app;
GRANT USAGE, CREATE ON SCHEMA production_min TO wicket_migrate, wicket_owner;

INSERT INTO wicket.schema_class (nspname, class) VALUES ('production_min', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA production_min
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA production_min
  GRANT TRIGGER ON TABLES TO wicket_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE wicket_owner IN SCHEMA production_min
  GRANT SELECT, INSERT, UPDATE ON TABLES TO wicket_app;

CREATE TABLE production_min.work_order (
  id                      uuid PRIMARY KEY,
  number                  text,
  item_id                 uuid NOT NULL,
  quantity_ordered_amount numeric(24,8) NOT NULL,
  quantity_ordered_uom_id bigint NOT NULL,
  quantity_ordered_dimension text NOT NULL,
  revision                text NOT NULL,
  status                  text NOT NULL CHECK (status IN (
                            'draft', 'released', 'in_process', 'completed', 'cancelled'
                          )),
  wip_location_id         uuid,
  released_at             timestamptz,
  completed_at            timestamptz,
  version                 bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  application_version     text NOT NULL,
  configuration_version   text NOT NULL DEFAULT '',
  CONSTRAINT work_order_dimension_known CHECK (
    quantity_ordered_dimension IN ('Count','Length','Mass','Time','Volume','Area')
  ),
  CONSTRAINT work_order_number_when_released CHECK (
    status IN ('draft', 'cancelled') OR number IS NOT NULL
  )
);
ALTER TABLE production_min.work_order OWNER TO wicket_owner;
CREATE UNIQUE INDEX work_order_number_uidx
  ON production_min.work_order (number)
  WHERE number IS NOT NULL;

CREATE TABLE production_min.issue_line (
  id                      uuid PRIMARY KEY,
  work_order_id           uuid NOT NULL REFERENCES production_min.work_order (id),
  inventory_document_id   uuid NOT NULL,
  item_id                 uuid NOT NULL,
  lot_id                  uuid,
  serial_id               uuid,
  quantity_amount         numeric(24,8) NOT NULL,
  quantity_uom_id         bigint NOT NULL,
  quantity_dimension      text NOT NULL,
  amount                  numeric(24,6),
  currency_id             integer,
  application_version     text NOT NULL,
  configuration_version   text NOT NULL DEFAULT '',
  CONSTRAINT issue_line_dimension_known CHECK (
    quantity_dimension IN ('Count','Length','Mass','Time','Volume','Area')
  )
);
ALTER TABLE production_min.issue_line OWNER TO wicket_owner;
CREATE INDEX issue_line_wo_idx ON production_min.issue_line (work_order_id);

CREATE TABLE production_min.completion (
  id                      uuid PRIMARY KEY,
  work_order_id           uuid NOT NULL REFERENCES production_min.work_order (id),
  finished_lot_id         uuid NOT NULL,
  quantity_good_amount    numeric(24,8) NOT NULL,
  quantity_good_uom_id    bigint NOT NULL,
  quantity_good_dimension text NOT NULL,
  quantity_scrap_amount   numeric(24,8) NOT NULL,
  quantity_scrap_uom_id   bigint NOT NULL,
  quantity_scrap_dimension text NOT NULL,
  group_id                uuid NOT NULL,
  application_version     text NOT NULL,
  configuration_version   text NOT NULL DEFAULT '',
  CONSTRAINT completion_good_dimension_known CHECK (
    quantity_good_dimension IN ('Count','Length','Mass','Time','Volume','Area')
  ),
  CONSTRAINT completion_scrap_dimension_known CHECK (
    quantity_scrap_dimension IN ('Count','Length','Mass','Time','Volume','Area')
  )
);
ALTER TABLE production_min.completion OWNER TO wicket_owner;
CREATE INDEX completion_wo_idx ON production_min.completion (work_order_id);

SELECT audit.attach('production_min.work_order'::regclass);
SELECT audit.attach('production_min.issue_line'::regclass);
SELECT audit.attach('production_min.completion'::regclass);

GRANT SELECT, INSERT, UPDATE ON production_min.work_order TO wicket_app;
GRANT SELECT, INSERT, UPDATE ON production_min.issue_line TO wicket_app;
GRANT SELECT, INSERT, UPDATE ON production_min.completion TO wicket_app;
REVOKE DELETE ON production_min.work_order, production_min.issue_line, production_min.completion
  FROM PUBLIC, wicket_app;

ALTER SCHEMA production_min OWNER TO wicket_owner;
