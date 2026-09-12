-- 0001_uom: unit master, conversion factors, rounding policy, item stock registry.
-- App-class schema `uom`. Reversible. Audited via audit.attach per table.

CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE SCHEMA IF NOT EXISTS uom AUTHORIZATION datum_owner;

REVOKE ALL ON SCHEMA uom FROM PUBLIC;
GRANT USAGE ON SCHEMA uom TO datum_app, datum_migrate, datum_owner;

INSERT INTO datum.schema_class (nspname, class) VALUES ('uom', 'app')
ON CONFLICT (nspname) DO UPDATE SET class = EXCLUDED.class;

ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA uom
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;
ALTER DEFAULT PRIVILEGES FOR ROLE datum_owner IN SCHEMA uom
  GRANT USAGE ON SEQUENCES TO datum_app;

CREATE TABLE uom.unit (
  id             bigint      PRIMARY KEY,
  code           text        NOT NULL UNIQUE,
  name           text        NOT NULL,
  dimension      text        NOT NULL CHECK (dimension IN
                    ('Count', 'Length', 'Mass', 'Time', 'Volume', 'Area')),
  symbol         text        NOT NULL,
  scale_default  smallint    NOT NULL CHECK (scale_default BETWEEN 0 AND 8)
);
ALTER TABLE uom.unit OWNER TO datum_owner;

CREATE TABLE uom.item_stock (
  item_id              uuid           PRIMARY KEY,
  stock_unit_id        bigint         NOT NULL REFERENCES uom.unit (id),
  stock_scale          smallint       NOT NULL CHECK (stock_scale BETWEEN 0 AND 8),
  residual_tolerance   numeric(24, 8) NOT NULL DEFAULT 0 CHECK (residual_tolerance >= 0)
);
ALTER TABLE uom.item_stock OWNER TO datum_owner;

-- Seam until ledger.posting exists: tests and pre-ledger immutability checks read this.
CREATE TABLE uom.posting_stub (
  item_id uuid NOT NULL PRIMARY KEY
);
ALTER TABLE uom.posting_stub OWNER TO datum_owner;
COMMENT ON TABLE uom.posting_stub IS
  'Pre-ledger seam: uom.item_has_postings reads ledger.posting when present, else this table.';

CREATE TABLE uom.factor (
  id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  from_unit       bigint         NOT NULL REFERENCES uom.unit (id),
  to_unit         bigint         NOT NULL REFERENCES uom.unit (id),
  item_id         uuid               NULL,
  lot_id          uuid               NULL,
  numerator       numeric(38, 18) NOT NULL,
  denominator     numeric(38, 18) NOT NULL CHECK (denominator <> 0),
  effective_from  timestamptz    NOT NULL,
  effective_to    timestamptz        NULL,
  CHECK (effective_to IS NULL OR effective_to > effective_from)
);
ALTER TABLE uom.factor OWNER TO datum_owner;

CREATE TABLE uom.rounding_policy (
  item_id    uuid   NOT NULL,
  unit_id    bigint NOT NULL REFERENCES uom.unit (id),
  operation  text   NOT NULL,
  rule       text   NOT NULL CHECK (rule IN ('HalfUp', 'HalfEven', 'TowardZero', 'AwayFromZero')),
  PRIMARY KEY (item_id, unit_id, operation)
);
ALTER TABLE uom.rounding_policy OWNER TO datum_owner;

ALTER TABLE uom.factor ADD CONSTRAINT factor_no_overlap EXCLUDE USING gist (
  from_unit WITH =,
  to_unit WITH =,
  (COALESCE(item_id, '00000000-0000-0000-0000-000000000000'::uuid)) WITH =,
  (COALESCE(lot_id, '00000000-0000-0000-0000-000000000000'::uuid)) WITH =,
  tstzrange(effective_from, effective_to, '[)') WITH &&
);

INSERT INTO uom.unit (id, code, name, dimension, symbol, scale_default) VALUES
  (1, 'EA',  'Each',     'Count',  'ea', 0),
  (2, 'MM',  'Millimetre','Length', 'mm', 2),
  (3, 'IN',  'Inch',     'Length', 'in', 4),
  (4, 'FT',  'Foot',     'Length', 'ft', 4),
  (5, 'KG',  'Kilogram', 'Mass',   'kg', 4),
  (6, 'LB',  'Pound',    'Mass',   'lb', 4),
  (7, 'MIN', 'Minute',   'Time',   'min', 0),
  (8, 'HR',  'Hour',     'Time',   'hr', 2);

INSERT INTO uom.factor (from_unit, to_unit, item_id, lot_id, numerator, denominator, effective_from)
VALUES (3, 4, NULL, NULL, 1, 12, '-infinity'::timestamptz);

CREATE FUNCTION uom.item_has_postings(p_item_id uuid) RETURNS boolean
LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path = pg_catalog, uom, ledger
AS $uom$
BEGIN
  IF to_regclass('ledger.posting') IS NOT NULL THEN
    RETURN EXISTS (
      SELECT 1 FROM ledger.posting p WHERE p.item_id = p_item_id
    );
  END IF;
  RETURN EXISTS (
    SELECT 1 FROM uom.posting_stub s WHERE s.item_id = p_item_id
  );
END
$uom$;
ALTER FUNCTION uom.item_has_postings(uuid) OWNER TO datum_owner;

CREATE FUNCTION uom.enforce_stock_item_immutable() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, uom
AS $uom$
BEGIN
  IF TG_OP = 'UPDATE'
     AND (
       NEW.stock_unit_id IS DISTINCT FROM OLD.stock_unit_id
       OR NEW.stock_scale IS DISTINCT FROM OLD.stock_scale
       OR NEW.residual_tolerance IS DISTINCT FROM OLD.residual_tolerance
     )
     AND uom.item_has_postings(OLD.item_id)
  THEN
    RAISE EXCEPTION 'datum: item stock measure immutable while postings exist'
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$uom$;
ALTER FUNCTION uom.enforce_stock_item_immutable() OWNER TO datum_owner;

CREATE TRIGGER zz_uom_stock_immutable
  BEFORE UPDATE ON uom.item_stock
  FOR EACH ROW EXECUTE FUNCTION uom.enforce_stock_item_immutable();

SELECT audit.attach('uom.unit'::regclass);
SELECT audit.attach('uom.item_stock'::regclass);
SELECT audit.attach('uom.factor'::regclass);
SELECT audit.attach('uom.rounding_policy'::regclass);

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA uom TO datum_app;
REVOKE DELETE ON ALL TABLES IN SCHEMA uom FROM PUBLIC, datum_app;

GRANT SELECT ON uom.posting_stub TO datum_app;
GRANT EXECUTE ON FUNCTION uom.item_has_postings(uuid) TO datum_owner, datum_migrate;

DO $grant$
BEGIN
  IF to_regclass('ledger.posting') IS NOT NULL THEN
    EXECUTE 'GRANT SELECT (item_id) ON TABLE ledger.posting TO datum_owner';
  END IF;
END
$grant$;
