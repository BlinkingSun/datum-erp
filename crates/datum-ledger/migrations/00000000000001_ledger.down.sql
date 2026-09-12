-- Reverse 0001_ledger.

DROP TRIGGER IF EXISTS consumption_group_invariants ON ledger.consumption;
DROP TRIGGER IF EXISTS posting_group_invariants ON ledger.posting;
DROP FUNCTION IF EXISTS ledger.enforce_group_invariants();

DROP TABLE IF EXISTS ledger.consumption;
DROP TABLE IF EXISTS ledger.posting;
DROP TABLE IF EXISTS ledger.posting_group;
DROP TABLE IF EXISTS ledger.stock_item;
DROP TABLE IF EXISTS ledger.location;

DROP TABLE IF EXISTS transient.layer_projection;
DROP TABLE IF EXISTS transient.balance_projection;

DROP TYPE IF EXISTS ledger.value_account;
DROP TYPE IF EXISTS ledger.cost_element;
DROP TYPE IF EXISTS ledger.boundary;
DROP TYPE IF EXISTS ledger.measure;
DROP TYPE IF EXISTS ledger.group_kind;

DELETE FROM datum.schema_class WHERE nspname = 'ledger';

DROP SCHEMA IF EXISTS ledger;
