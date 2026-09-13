# Locations module — validation protocol

Executable tests (commit mode, `WICKET_REQUIRE_PG=1`):

- `install_seeds_seven_boundary_locations_once`
- `boundary_class_is_immutable`
- `tree_has_no_cycles`
- `deactivate_refused_while_on_hand`
- `ensure_wip_is_idempotent_per_work_order`
- `registry_row_matches_location`
- `every_locations_table_is_audited_and_owned_by_wicket_owner`
- `writes_go_through_tx`
- `http_manifest_binds_view_and_edit`
- `list_paginates_by_cursor`
- `list_tree_filters_inactive_by_default`
- `deactivate_after_plain_install`
- `generated_and_seeded_ids_are_uuid_v7`

Fixtures: `WC-LATHE-03`, `QUARANTINE`, `WO-2026-1847`, `MDS-450-M4x12` (`PLAN.md` §3).

Run: `cargo test -p wicket-mod-locations`
