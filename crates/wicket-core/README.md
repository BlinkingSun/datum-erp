# wicket-core

Kernel primitives with no database and no async: identifiers, quantities,
money, residuals, and the two inversion traits other crates implement
(`PostingSink`, `SignatureGate`). Domain math uses `Quantity`; plumbing uses
`AnyQuantity`. Residuals are un-ignorable (`into_exact` / `split`).

## Public API (`src/lib.rs`)

Modules: `actor`, `convert`, `error`, `id`, `money`, `posting`, `quantity`,
`residual`, `signature`, `units`.

- `Identifier` — opaque uuid v7 newtype (`generate` / `from_uuid` / `as_uuid`)
- `ItemId` / `LotId` / `SerialId` / `LocationId` / `UserId` / `SignatureId` — typed ids; no cross-conversion (CONTRACT §6.1)
- `Actor` / `ActorKind` — authenticated actor (`User` / `ServicePrincipal`)
- `Error` / `Result` — shared kernel error (`Invariant`, `Overflow`, wrappers, `Unimplemented`)
- `ConversionContext` — item and optional lot for a conversion factor
- `UnitCatalog` / `UnitConverter` — unit master and within-dimension convert (implemented by `wicket-uom`)
- `Money` / `MoneyWire` / `UnitCost` / `MoneyError` — money at scale 6; wire form; cost
- `MONEY_MAX_SCALE` / `RATE_MAX_SCALE` — scale caps
- `GroupKind` / `Boundary` / `CostElement` / `ValueAccount` — posting enums (CONTRACT §6.2)
- `PostingHandle` — group-local index of a contributed quantity intent
- `PostingId` — immutable `ledger.posting.posting_id` (not a handle)
- `PostingGroupHeader` — group metadata; actor and timestamps are not here
- `QuantityPosting` / `ValuePosting` / `ConsumptionPosting` / `PostingIntent` — intents
- `PostingError` — sink failures including `Unfinalized` / `NoSink`
- `PostingSink` — `contribute` then one `finalize` (CONTRACT §6.2)
- `NoPostings` — test sink; `contribute`/`finalize` → `NoSink`
- `AnyQuantity` / `Quantity` / `QuantityError` / `QUANTITY_MAX_SCALE` — typed vs plumbing qty
- `Converted` / `Extended` / `Scaled` / `Settled` — residual wrappers
- `ResidualError` / `Rounding` — residual demand and rounding rules
- `SignatureMeaning` / `PermissionKey` / `RecordRef` / `SignatureRequirement` / `SignatureToken` — gate inputs (CONTRACT §6.3)
- `SignatureError` / `SignatureGate` — verify-only gate; minting is `wicket-esign`
- `NoSignatures` — `verify` → `NoProvider`
- `UnitId` / `CurrencyId` / `UnitRef` / `Dimension` / `DimensionKind` — unit identity
- `CountDim` / `LengthDim` / `MassDim` / `TimeDim` / `VolumeDim` / `AreaDim` — dimension markers

## Migrations

None. This crate has no SQL.

## Tests (`tests/`)

- `api_surface_compiles` — frozen names exist with stated signatures
- `try_add_commutative` / `try_add_associative` / `negate_is_involution`
- `split_reconstructs_original` — residual split reconstitutes the product
- `money_allocate_sums_and_zero_weights`
- `any_quantity_serde_roundtrip_scale8` / `accepted_quantity_roundtrips_through_decimal_string`
- `identifier_display_fromstr_roundtrip`
- trybuild: `converted_cannot_be_read_without_exit`, `unit_ref_is_the_only_constructor`,
  `posting_id_is_not_a_handle`, `length_plus_mass_does_not_compile`,
  `money_plus_quantity_does_not_compile`

## Frozen / seams

Frozen: CONTRACT §6 (identifiers, actor, `Error`, `PostingSink`, `SignatureGate`)
and `research/decisions/core-quantity.md`. Downstream crates may name only the
§5 stub subset until they need more. `SignatureGate` is implemented by
`wicket-esign` in Wave 2b; core ships `NoSignatures`. `UnitConverter` is
implemented by `wicket-uom`. `PostingSink` is implemented by `wicket-ledger`
(`GroupBuilder`).
