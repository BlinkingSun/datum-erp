# Locations module — validation protocol

Executable tests (commit mode, `DATUM_REQUIRE_PG=1`):

- `install_seeds_seven_boundary_locations_once`
- `boundary_class_is_immutable`
- `tree_has_no_cycles`
- `deactivate_refused_while_on_hand`
- `ensure_wip_is_idempotent_per_work_order`
- `registry_row_matches_location`
- `every_locations_table_is_audited_and_owned_by_datum_owner`
- `writes_go_through_tx`

Run: `cargo test -p datum-mod-locations`
