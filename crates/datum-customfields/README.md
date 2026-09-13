# datum-customfields

Kernel extension mechanism: typed, validated, audited fields on any app-class entity without altering the entity table. Schema `customfields` (class `app`).

## API

| Function | Purpose |
|---|---|
| `define` | Create or version a field definition |
| `retire` | Retire a definition (owner module or `customfields.define`) |
| `set` / `get` | Write/read typed values on a record |
| `list_for_record` | Active definitions with values present |
| `definitions_for` | Active definitions for an entity |
| `validate` | Check a value against a definition |
| `register_from_manifest` | Idempotent `[[custom-fields]]` registration |

## Schema

- `customfields.definition` — effectivity-versioned metadata (no `jsonb`).
- `customfields.value_*` — one table per type (`string`, `text`, `integer`, `decimal`, `bool`, `date`, `enum`, `reference`), keyed by `(definition_id, record_id)`, audited via `zz_audit_row`.

## Validation registry

| Rule | Define-time | Set-time |
|---|---|---|
| `gs1-gtin` | string fields | GTIN check digit |
| `regex:<pattern>` | non-empty pattern | pattern match |
| `range:<min>..<max>` | integer/decimal | inclusive bounds |
| `enum` / `enum:a,b,c` | enum fields | allowed members |
| `length:<max>` | string/text | max length |

Unknown rule names are refused at define time. Failures at set time return `Error::ValidationFailed` naming the rule.

## Not a JSON blob

Regulated shops need every extension field audited like a native column. A single `jsonb` column hides changes from the audit trail and cannot enforce per-type indexes or validation. This crate stores each type in its own table with the kernel audit trigger.

## Wire shape (`docs/10`)

```json
{
  "key": "udi_device_identifier",
  "type": "string",
  "value": "4006381333931",
  "definition_version": 1
}
```

## Tests

Named commit-mode tests in `tests/customfields.rs` and `tests/migrate.rs` run under both `plain-shop` and `regulated-device` profile labels (configuration version on the write context).
