# datum-identity

Principals, login and signing credentials, sessions, and RBAC. Writes go
through `datum_db::Tx`. SQL uses `sqlx::query` / `query_as` / `query_scalar`
(CONTRACT §5a as amended).

## Public API (`src/lib.rs`)

- `Error` / `Result` — `UsernameReused`, `Inactive`, `NotFound`, `InvalidCredentials`, `Lockout`, reset errors, `Crypto`, `Bundle`
- `MIGRATOR` — `placeholder` + `0001_identity`
- `UserId` / `RoleId` — identity-local ids (not the core identifier family for users)
- `LOCKOUT_AFTER` / `LOCKOUT_SECS` — 5 failures, 15 minutes
- `ARGON2ID_M_KIB` / `ARGON2ID_T` / `ARGON2ID_P` — pinned Argon2id parameters
- `hash_password` / `hash_password_with_params` / `verify_password` — PHC encoded
- `CredentialKind` — login vs signing
- `set_login_credential` / `set_signing_credential` — store hashes (separate columns)
- `verify_login_secret` / `verify_signing` — check secrets
- `request_reset` / `complete_reset` — two-principal reset (D3 §9)
- `SYSTEM_ID` / `MIGRATION_ID` — well-known principal uuids
- `Principal` / `PrincipalKind` / `PrincipalStatus` — loaded principal
- `seed_builtins` / `create_principal` / `deactivate_principal` / `load_principal` / `rename_principal`
- `PasswordProvider` / `Provider` / `Session` / `login` / `reauth_signing`
- `rbac` — `RoleBundle`, `load_bundles`, `seed_bundles`, `assign_role`, `has_permission`

`Principal::display_name_at` reads `identity.display_name_history` (inv. 15).

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_identity` — schema `identity` (app): `principal`, `username_history`,
  `display_name_history`, `login_credential`, `signing_credential`, `credential_reset`,
  `role`, `role_permission`, `principal_role`; `transient.session` (transient)

## Tests (`tests/`)

- `principal_cannot_be_deleted` / `username_is_never_reused` (inv. 13)
- `display_name_at_returns_name_as_of` / `display_name_at_before_first_history_row` (inv. 15)
- `signing_credential_is_separate_from_login` (inv. 14)
- `reset_requires_two_principals` / `lockout_after_n_failures`
- `argon2id_parameters_are_pinned_and_tested`
- `has_permission_via_role_bundle` / `login_records_device_and_ip`
- `every_identity_table_is_audited` / `audit_fk_validates_existing_rows`
- `hash_columns_are_redacted_in_audit` / `writes_go_through_tx`
- `migrate_down_then_up` / `identity_tables_owned_by_datum_owner` / `builtins_exist_after_migrate`

## Frozen / seams

Frozen: principal/credential/session signatures other crates call; inv. 13–14.
Inv. 15 has `display_name_at` but no signature snapshot — that is Wave 2b
`datum-esign`. Builtin `migration`/`system` INSERTs in 0001 run before
`zz_audit_row` on the composition path (FINDINGS-0 #7); crate-local tests attach
privileged first.
