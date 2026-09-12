# datum-module

Composition root: module registry, profiles, and kernel wiring. Wires
`datum_core::PostingSink` (`datum_ledger::GroupBuilder`) and
`datum_core::SignatureGate` (`NoSignatures` until `datum-esign`).

## Public API (`src/lib.rs`)

- `ConfigurationManifest` / `ManifestModule` / `export_manifest` / `verify` — `docs/03` §8 dump
- `Error` / `Result`
- `Kernel` — `build`, `posting_sink`, `signature_gate`, `gate_is_noop`, `hook_order`, `module_order`, `catalog`, `profile_id`
- `edges_from_registry` / `module_nodes` / `posting_sink` / `startup_fails_if_required_meets_no_signatures`
- `ModuleManifest` / `compiled_in` / `compiled_in_graph`
- `CONTRACT_KERNEL_EDGES` / `KERNEL_ORDER` / `MIGRATE_PREFIX` — CONTRACT §4 graph
- `ModuleNode` / `topological_order` / `is_topological_sort`
- `kernel_crates` / `kernel_migrators` / `migrate_prefix` / `migrate_suffix` / `run_migrations` / `attach_kernel_audit`
- `DELTA_ALLOWED` / `GateBinding` / `Profile` / `ProfileId` / `ProfileModule` / `SignatureEdge`
- `delta_keys` / `profile_does_not_rewrite_edges`
- `InstalledRow` / `install` / `uninstall` / `enable` / `disable` / `upgrade` / `list_installed`
- `Range` / `Version` — semver for manifests
- `manifest_export` — `export` / `verify` / `ConfigurationManifest`
- `MIGRATOR` — `placeholder` + `0001_module`

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_module` — schema `module` (app): `module.installed`,
  `module.configuration`, `module.install_log`

## Tests (`tests/`)

- `kernel_order_is_a_topological_sort_of_contract_graph`
- `both_profiles_carry_eleven_keys_and_load` / `profiles_delta_is_subset_of_allowed_keys`
- `plain_shop_required_signature_set_is_empty` / `signature_edges_come_from_registry_not_toml`
- `startup_fails_release_required_edge_with_no_signatures`
- `hook_order_matches_statemachine_hook_order`
- `schema_history_trigger_is_present`
- `install_runs_migrations_in_one_transaction_and_records`
- `enable_closes_over_dependencies` / `disable_depended_on_module_is_refused_naming_dependents`
- `disable_never_drops_tables` / `manifest_hash_changes_when_enabled_set_changes`
- `configuration_manifest_round_trips_and_verifies` / `writes_go_through_tx`
- `list_installed_after_kernel_build`

## Frozen / seams

Frozen: `KERNEL_ORDER` / `CONTRACT_KERNEL_EDGES` (CONTRACT §4), `posting_sink`
factory, `startup_fails_if_required_meets_no_signatures` (CONTRACT §6.3).
Seams (FINDINGS-0 #1/#2): `Kernel::build` freezes with zero `register_machine`;
gate is `NoSignatures` (Wave 2b `datum-esign`); profile `gate` TOML field is not
the bound gate; `enable` has no profile check; key-10 currency/UOM/tz discarded;
`enable_genealogy_bridge` unused; `install` is registry DML, not module migrations
in the install Tx. `0001_module.down.sql` exists; no `migrate_down_then_up`
(FINDINGS-0 #4).
