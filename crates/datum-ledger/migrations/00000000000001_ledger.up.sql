-- 0001_ledger: posting groups, postings, consumption, registries, deferred invariants.
-- Schema `ledger` is schema-class `app` (CONTRACT §8a): no DELETE. Projections live in
-- `transient`. Reversible. Tables owned by datum_owner. No ON DELETE CASCADE.
--
-- Byte-faithful to D2 §§4.1, 5.1–5.4 except:
--   * AMENDMENT A1 (D-W1-1): amount numeric(24,6); unit_cost_applied numeric(24,8);
--     consumption.amount numeric(24,6).
--   * stock_uom_id / posting.uom_id are bigint, not uuid: datum-core UnitId is i64 and
--     uom.unit.id is bigint (D1). Documented in the lane report.
--   * standard_cost / standard_currency on stock_item: D2 §5.3 names STANDARD costing
--     but does not place the standard amount; required by SPEC deliverable 4.
--   * Schema, grants, audit.attach, schema_class, and transient projections are
--     workspace obligations (CONTRACT §§5, 8a, SPEC items 1 and 7), not in D2 SQL.
-- Created by datum_migrate so CREATE TABLE can fire audit.attach_new_tables;
-- tables are then owned by datum_owner. D-2b-11.

SELECT
  pg_catalog.set_config('datum.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('datum.actor_kind',    'migration', true),
  pg_catalog.set_config('datum.actor_display', 'migration', true),
  pg_catalog.set_config('datum.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('datum.action',        'ledger.migrate', true),
  pg_catalog.set_config('datum.source_kind',   'migration', true);

CREATE SCHEMA IF NOT EXISTS ledger AUTHORIZATION datum_migrate;

REVOKE ALL ON SCHEMA ledger FROM PUBLIC;
GRANT USAGE ON SCHEMA ledger TO datum_app;
GRANT USAGE, CREATE ON SCHEMA ledger TO datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('ledger', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA ledger
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA ledger
  GRANT TRIGGER ON TABLES TO datum_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA ledger
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA ledger
  GRANT USAGE ON SEQUENCES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA ledger
  GRANT USAGE ON SEQUENCES TO datum_app;

CREATE TYPE ledger.group_kind AS ENUM
  ('MOVEMENT','ADJUSTMENT','TRANSFORMATION','VALUATION','REVERSAL');

CREATE TYPE ledger.measure       AS ENUM ('QUANTITY','VALUE');
CREATE TYPE ledger.boundary      AS ENUM
  ('SUPPLIER','CUSTOMER','SCRAP','ADJUSTMENT','ROUNDING','CONSUMED','PRODUCED');
CREATE TYPE ledger.cost_element  AS ENUM ('MATERIAL','LABOR','BURDEN','OUTSIDE');
CREATE TYPE ledger.value_account AS ENUM
  ('INVENTORY','WIP','COGS','SCRAP_EXPENSE','ADJUSTMENT_EXPENSE',
   'AP_ACCRUAL','LABOR_ABSORBED','BURDEN_ABSORBED','PPV','MFG_VARIANCE','ROUNDING');

GRANT USAGE ON TYPE ledger.group_kind TO datum_app, datum_migrate, datum_owner;
GRANT USAGE ON TYPE ledger.measure TO datum_app, datum_migrate, datum_owner;
GRANT USAGE ON TYPE ledger.boundary TO datum_app, datum_migrate, datum_owner;
GRANT USAGE ON TYPE ledger.cost_element TO datum_app, datum_migrate, datum_owner;
GRANT USAGE ON TYPE ledger.value_account TO datum_app, datum_migrate, datum_owner;

-- Kernel-owned registry. The items module writes it through a published interface;
-- the ledger never reads a module's tables (PLAN.md §6.6).
CREATE TABLE ledger.stock_item (
  item_id            uuid PRIMARY KEY,
  stock_uom_id       bigint   NOT NULL,
  stock_scale        smallint NOT NULL CHECK (stock_scale BETWEEN 0 AND 8),
  residual_tolerance numeric(24,8) NOT NULL CHECK (residual_tolerance >= 0),
  cost_method        text     NOT NULL CHECK (cost_method IN ('FIFO','MOVING_AVG','STANDARD')),
  standard_cost      numeric(24,6),
  standard_currency  smallint,
  UNIQUE (item_id, stock_uom_id, stock_scale, residual_tolerance),
  CONSTRAINT standard_when_needed CHECK (
    cost_method <> 'STANDARD'
    OR (standard_cost IS NOT NULL AND standard_currency IS NOT NULL)
  )
);
ALTER TABLE ledger.stock_item OWNER TO datum_owner;

CREATE TABLE ledger.location (
  location_id    uuid PRIMARY KEY,
  boundary_class ledger.boundary,
  UNIQUE (location_id, boundary_class)
);
ALTER TABLE ledger.location OWNER TO datum_owner;

CREATE TABLE ledger.posting_group (
  group_id          uuid PRIMARY KEY,
  kind              ledger.group_kind NOT NULL,
  posted_at         timestamptz NOT NULL DEFAULT clock_timestamp(),
  created_xid       xid8        NOT NULL DEFAULT pg_current_xact_id(),
  actor_id          uuid NOT NULL,
  source_kind       text NOT NULL,
  source_id         uuid,
  work_order_id     uuid,
  reason_code       text,
  reverses_group_id uuid,
  reverses_kind     ledger.group_kind,

  UNIQUE (group_id, kind),
  UNIQUE (group_id, work_order_id),

  CONSTRAINT reason_required
    CHECK (kind <> 'ADJUSTMENT' OR reason_code IS NOT NULL),
  CONSTRAINT wo_required
    CHECK (kind <> 'TRANSFORMATION' OR work_order_id IS NOT NULL),
  CONSTRAINT reversal_shape
    CHECK ((kind = 'REVERSAL') = (reverses_group_id IS NOT NULL)),
  CONSTRAINT no_reversal_of_reversal
    CHECK (reverses_kind IS DISTINCT FROM 'REVERSAL'),

  FOREIGN KEY (reverses_group_id, reverses_kind)
    REFERENCES ledger.posting_group (group_id, kind)
);
ALTER TABLE ledger.posting_group OWNER TO datum_owner;

CREATE UNIQUE INDEX posting_group_reversed_once
  ON ledger.posting_group (reverses_group_id)
  WHERE reverses_group_id IS NOT NULL;

CREATE TABLE ledger.posting (
  posting_id  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  group_id    uuid    NOT NULL,
  kind        ledger.group_kind NOT NULL,
  measure     ledger.measure    NOT NULL,
  created_xid xid8    NOT NULL DEFAULT pg_current_xact_id(),

  item_id            uuid,
  uom_id             bigint,
  stock_scale        smallint,
  residual_tolerance numeric(24,8),
  location_id        uuid,
  boundary           ledger.boundary,
  lot_id             uuid,
  serial_id          uuid,
  quantity           numeric(24,8),

  account           ledger.value_account,
  cost_element      ledger.cost_element,
  cost_object_id    uuid,
  currency_id       smallint,
  amount            numeric(24,6),
  values_posting_id bigint,

  entered_quantity  numeric(24,8),
  entered_uom_id    bigint,
  conversion_factor numeric(38,18),
  unit_cost_applied numeric(24,8),

  UNIQUE (posting_id, group_id),

  CONSTRAINT measure_shape CHECK (
    (measure = 'QUANTITY'
       AND quantity IS NOT NULL AND item_id IS NOT NULL AND uom_id IS NOT NULL
       AND stock_scale IS NOT NULL AND residual_tolerance IS NOT NULL
       AND location_id IS NOT NULL
       AND amount IS NULL AND account IS NULL AND cost_element IS NULL
       AND currency_id IS NULL AND values_posting_id IS NULL)
    OR
    (measure = 'VALUE'
       AND amount IS NOT NULL AND account IS NOT NULL AND cost_element IS NOT NULL
       AND currency_id IS NOT NULL
       AND quantity IS NULL AND location_id IS NULL AND boundary IS NULL
       AND stock_scale IS NULL AND residual_tolerance IS NULL)
  ),

  CONSTRAINT no_zero_rows CHECK (COALESCE(quantity, amount) <> 0),

  CONSTRAINT quantity_exact_at_scale CHECK (
    measure <> 'QUANTITY' OR quantity = round(quantity, stock_scale::int)
  ),

  CONSTRAINT boundary_permitted CHECK (
    boundary IS NULL
    OR (kind = 'MOVEMENT'       AND boundary IN ('SUPPLIER','CUSTOMER'))
    OR (kind = 'ADJUSTMENT'     AND boundary IN ('SCRAP','ADJUSTMENT','ROUNDING'))
    OR (kind = 'TRANSFORMATION' AND boundary IN ('CONSUMED','PRODUCED'))
    OR  kind = 'REVERSAL'
  ),
  CONSTRAINT valuation_moves_no_matter CHECK (
    kind <> 'VALUATION' OR measure = 'VALUE'
  ),

  CONSTRAINT rounding_is_dust CHECK (
    boundary IS DISTINCT FROM 'ROUNDING' OR abs(quantity) <= residual_tolerance
  ),

  CONSTRAINT value_attaches_to_matter CHECK (
    measure <> 'VALUE'
    OR kind = 'VALUATION'
    OR account NOT IN ('INVENTORY','WIP')
    OR values_posting_id IS NOT NULL
  ),
  CONSTRAINT wip_names_its_cost_object CHECK (
    account IS DISTINCT FROM 'WIP' OR cost_object_id IS NOT NULL
  ),

  FOREIGN KEY (group_id, kind)
    REFERENCES ledger.posting_group (group_id, kind),
  FOREIGN KEY (item_id, uom_id, stock_scale, residual_tolerance)
    REFERENCES ledger.stock_item (item_id, stock_uom_id, stock_scale, residual_tolerance),
  FOREIGN KEY (location_id, boundary)
    REFERENCES ledger.location (location_id, boundary_class),
  FOREIGN KEY (group_id, cost_object_id)
    REFERENCES ledger.posting_group (group_id, work_order_id),
  FOREIGN KEY (values_posting_id, group_id)
    REFERENCES ledger.posting (posting_id, group_id)
);
ALTER TABLE ledger.posting OWNER TO datum_owner;

CREATE INDEX posting_group_idx   ON ledger.posting (group_id);
CREATE INDEX posting_values_idx  ON ledger.posting (values_posting_id)
  WHERE values_posting_id IS NOT NULL;
CREATE INDEX posting_balance_idx ON ledger.posting (item_id, location_id, lot_id)
  WHERE measure = 'QUANTITY';
-- D2 prints `USING brin (created_xid)`; xid8 has no default BRIN operator class
-- on PostgreSQL 15–18 (42704). Btree preserves the index's purpose (scan by xid).
CREATE INDEX posting_xid_idx     ON ledger.posting (created_xid);

CREATE TABLE ledger.consumption (
  consuming_posting_id bigint NOT NULL REFERENCES ledger.posting (posting_id),
  consumed_posting_id  bigint NOT NULL REFERENCES ledger.posting (posting_id),
  group_id             uuid   NOT NULL REFERENCES ledger.posting_group (group_id),
  quantity             numeric(24,8) NOT NULL CHECK (quantity <> 0),
  amount               numeric(24,6) NOT NULL,
  PRIMARY KEY (consuming_posting_id, consumed_posting_id),
  CONSTRAINT no_self_consumption CHECK (consuming_posting_id <> consumed_posting_id)
);
ALTER TABLE ledger.consumption OWNER TO datum_owner;

CREATE INDEX consumption_forward_idx ON ledger.consumption (consumed_posting_id);
CREATE INDEX consumption_group_idx   ON ledger.consumption (group_id);

CREATE OR REPLACE FUNCTION ledger.enforce_group_invariants()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = ledger, pg_catalog
AS $fn$
DECLARE
  gid uuid := COALESCE(NEW.group_id, OLD.group_id);
  g   ledger.posting_group%ROWTYPE;
  bad record;
BEGIN
  SELECT * INTO g FROM ledger.posting_group WHERE group_id = gid;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'ledger: group % has postings but no header', gid
      USING ERRCODE = 'ZL000';
  END IF;

  ------------------------------------------------------------------ P0
  PERFORM 1 FROM ledger.posting
    WHERE group_id = gid AND created_xid <> g.created_xid LIMIT 1;
  IF FOUND THEN
    RAISE EXCEPTION 'ledger: group % was extended by a later transaction', gid
      USING ERRCODE = 'ZL001',
            HINT = 'a posting group is one atomic fact; post a new group instead';
  END IF;

  ------------------------------------------------------------------ P1
  SELECT p.item_id, p.uom_id, sum(p.quantity) AS net INTO bad
    FROM ledger.posting p
   WHERE p.group_id = gid AND p.measure = 'QUANTITY'
   GROUP BY p.item_id, p.uom_id
  HAVING sum(p.quantity) <> 0
   LIMIT 1;
  IF FOUND THEN
    RAISE EXCEPTION
      'ledger: quantity not conserved in % group %: item % uom % nets %',
      g.kind, gid, bad.item_id, bad.uom_id, bad.net
      USING ERRCODE = 'ZL002';
  END IF;

  ------------------------------------------------------------------ P2-A
  SELECT p.currency_id, sum(p.amount) AS net INTO bad
    FROM ledger.posting p
   WHERE p.group_id = gid AND p.measure = 'VALUE'
   GROUP BY p.currency_id
  HAVING sum(p.amount) <> 0
   LIMIT 1;
  IF FOUND THEN
    RAISE EXCEPTION
      'ledger: value not conserved in % group %: currency % nets %',
      g.kind, gid, bad.currency_id, bad.net
      USING ERRCODE = 'ZL003';
  END IF;

  ------------------------------------------------------------------ P2-B (MOVEMENT only)
  IF g.kind = 'MOVEMENT' THEN
    SELECT p.currency_id, p.cost_element, sum(p.amount) AS net INTO bad
      FROM ledger.posting p
     WHERE p.group_id = gid AND p.measure = 'VALUE'
     GROUP BY p.currency_id, p.cost_element
    HAVING sum(p.amount) <> 0
     LIMIT 1;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: MOVEMENT group % reclassifies cost element % by % (currency %)',
        gid, bad.cost_element, bad.net, bad.currency_id
        USING ERRCODE = 'ZL004',
              HINT = 'a movement carries cost, it does not re-elementise it; '
                     'post the intake as a separate VALUATION group';
    END IF;
  END IF;

  ------------------------------------------------------------------ P3
  IF g.kind IN ('MOVEMENT','ADJUSTMENT','TRANSFORMATION') THEN
    SELECT p.posting_id, p.quantity, e.qty, e.amt, v.amt AS vamt INTO bad
      FROM ledger.posting p
      CROSS JOIN LATERAL (
        SELECT COALESCE(sum(c.quantity), 0) AS qty,
               COALESCE(sum(c.amount),   0) AS amt
          FROM ledger.consumption c
         WHERE c.consuming_posting_id = p.posting_id
      ) e
      CROSS JOIN LATERAL (
        SELECT COALESCE(sum(x.amount), 0) AS amt
          FROM ledger.posting x
         WHERE x.group_id = gid
           AND x.measure  = 'VALUE'
           AND x.values_posting_id = p.posting_id
      ) v
     WHERE p.group_id = gid
       AND p.measure  = 'QUANTITY'
       AND p.boundary IS NULL
       AND p.quantity < 0
       AND (e.qty <> -p.quantity OR e.amt <> -v.amt)
     LIMIT 1;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: posting % in group % withdraws %, but cost layers account for % '
        'quantity and % value against % posted',
        bad.posting_id, gid, bad.quantity, bad.qty, bad.amt, bad.vamt
        USING ERRCODE = 'ZL005',
              HINT = 'every withdrawal must name the postings it came out of';
    END IF;
  END IF;

  ------------------------------------------------------------------ P4
  IF g.kind = 'REVERSAL' THEN
    SELECT * INTO bad FROM (
      SELECT measure, item_id, uom_id, location_id, boundary, lot_id, serial_id,
             account, cost_element, cost_object_id, currency_id,
             COALESCE(sum(quantity), 0) AS q,
             COALESCE(sum(amount),   0) AS a
        FROM ledger.posting
       WHERE group_id IN (gid, g.reverses_group_id)
       GROUP BY 1,2,3,4,5,6,7,8,9,10,11
      HAVING COALESCE(sum(quantity), 0) <> 0 OR COALESCE(sum(amount), 0) <> 0
    ) t LIMIT 1;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: reversal % is not the exact negation of %: residual qty % value %',
        gid, g.reverses_group_id, bad.q, bad.a
        USING ERRCODE = 'ZL006';
    END IF;

    PERFORM 1 FROM (
      SELECT c.consumed_posting_id
        FROM ledger.consumption c
       WHERE c.group_id IN (gid, g.reverses_group_id)
       GROUP BY c.consumed_posting_id
      HAVING sum(c.quantity) <> 0 OR sum(c.amount) <> 0
    ) t;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: reversal % does not restore the cost layers consumed by %',
        gid, g.reverses_group_id
        USING ERRCODE = 'ZL007';
    END IF;
  END IF;

  RETURN NULL;
END;
$fn$;
ALTER FUNCTION ledger.enforce_group_invariants() OWNER TO datum_owner;
GRANT EXECUTE ON FUNCTION ledger.enforce_group_invariants() TO datum_app, datum_migrate, datum_owner;

CREATE CONSTRAINT TRIGGER posting_group_invariants
  AFTER INSERT OR UPDATE OR DELETE ON ledger.posting
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW
  EXECUTE FUNCTION ledger.enforce_group_invariants();

CREATE CONSTRAINT TRIGGER consumption_group_invariants
  AFTER INSERT OR UPDATE OR DELETE ON ledger.consumption
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW
  EXECUTE FUNCTION ledger.enforce_group_invariants();

-- Rebuildable caches (PLAN §6.1 / SPEC item 7). No audit trigger. DELETE expected.
CREATE TABLE transient.balance_projection (
  item_id     uuid           NOT NULL,
  location_id uuid           NOT NULL,
  lot_id      uuid,
  serial_id   uuid,
  uom_id      bigint         NOT NULL,
  quantity    numeric(24,8)  NOT NULL,
  CONSTRAINT balance_projection_slice
    UNIQUE NULLS NOT DISTINCT (item_id, location_id, lot_id, serial_id, uom_id)
);
ALTER TABLE transient.balance_projection OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE transient.balance_projection TO datum_app;

CREATE TABLE transient.layer_projection (
  posting_id     bigint        PRIMARY KEY,
  remaining_qty  numeric(24,8) NOT NULL,
  remaining_amt  numeric(24,6) NOT NULL
);
ALTER TABLE transient.layer_projection OWNER TO datum_owner;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE transient.layer_projection TO datum_app;

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA ledger TO datum_app;
REVOKE DELETE ON ALL TABLES IN SCHEMA ledger FROM PUBLIC, datum_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA ledger TO datum_app;

GRANT SELECT (item_id) ON TABLE ledger.posting TO datum_owner;

SELECT audit.attach('ledger.stock_item'::regclass);
SELECT audit.attach('ledger.location'::regclass);
SELECT audit.attach('ledger.posting_group'::regclass);
SELECT audit.attach('ledger.posting'::regclass);
SELECT audit.attach('ledger.consumption'::regclass);
