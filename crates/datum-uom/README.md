# datum-uom

Unit master, conversion engine, and ledger boundary helper (`to_stock`).

## Frozen API (until `datum-ledger` lands)

`to_stock` keeps the signature integrated at b62bfd6: it takes `&mut Tx`, a caller-held
`&UomCatalog` (reloaded inside the function), `ItemId`, `AnyQuantity`, and `&ConversionContext`.
Post-ledger cleanup (drop the unused catalog parameter, thread explicit `as_of`) is out of scope
for cycle-2.
