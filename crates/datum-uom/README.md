# datum-uom

Unit master, conversion engine (`UnitCatalog` + `UnitConverter`), and the ledger
boundary helper `to_stock`. Factors may be global, item-scoped, or lot-pinned.

## Public API (`src/lib.rs`)

- `UomCatalog` — per-transaction catalog loaded from `uom.*`
- `load_catalog` — load catalog inside `tx`
- `split_with_policy` — split a `Converted` with the item's rounding policy
- `Error` / `Result`
- `pin_lot_factor` — write a lot-scoped factor with open effectivity
- `Operation` / `apply_rounding_policy` — stock/issue/etc. rounding
- `StockConversion` — canonical stock qty, entered provenance, factor, residual
- `to_stock` — convert entered qty to the item stock unit in `tx`
- `convert` — `UnitConverter::convert` against a loaded catalog
- `load_unit` — load a `UnitId` from `uom.unit` on a read pool
- `MIGRATOR` — `placeholder` + `0001_uom`
- Re-exports from `datum-core`: `ConversionContext`, `Converted`, `DimensionKind`,
  `Rounding`, `UnitCatalog`, `UnitConverter`, `UnitId`

`to_stock` signature: `&mut Tx`, `&UomCatalog` (caller-held; reloaded inside),
`ItemId`, `AnyQuantity`, `&ConversionContext` → `StockConversion<D>`.

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_uom` — schema `uom` (app): `uom.unit`, `uom.item_stock`,
  `uom.factor`, `uom.rounding_policy` (all app)

## Tests (`tests/`)

- `convert_within_dimension_returns_converted_with_residual`
- `effectivity_picks_the_factor_in_force` / `inverse_factor_is_inferred_exactly`
- `repin_closes_the_open_pin` / `lot_factor_beats_item_factor_beats_global`
- `pin_then_to_stock_in_one_tx` / `to_stock_with_and_without_lot_pin`
- `seeded_units_have_right_dimensions` / `stock_unit_immutable_while_postings_exist`
- `every_uom_table_is_audited` / `writes_go_through_tx`
- `round_trip_ab_a_within_one_ulp_at_stock_scale_8` / `round_half_even_at_stock_scale`
- `cross_dimension_is_unrepresentable` (trybuild)
- `reversible_migration_drops_btree_gist`

## Frozen / seams

Frozen: `to_stock` signature (held through ledger landing) and `UnitConverter`
(CONTRACT §4, §6 convert traits). Ledger is landed: `ledger.posting` is the
source of truth for `uom.item_has_postings` when present; the function returns
false when that relation is absent. The unused catalog parameter on `to_stock`
is kept until a later cleanup. `as_of` is still transaction_timestamp, not an
explicit argument.
