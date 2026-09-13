# datum-module

Composition root: module registry, installation profiles, and the assembled
`Kernel` handle Wave 2s (`datum-server` / first-party modules) drives.
Frozen public API for Wave 2s.

## What this crate owns

- Parsed `module.toml` (`ModuleManifest`) and the two
  installation profiles (`profiles/regulated-device.toml`,
  `profiles/plain-shop.toml`).
- Lifecycle: `install` / `enable` / `disable` / `upgrade` against
  `module.installed` (`docs/03` §6). Disable never drops; disabling a depended-on
  module is refused with the dependents named.
- `CANONICAL_ORDER` / `KERNEL_ORDER` and `install_upto` (the one harness
  entry point; `install_privileged` once after `datum-audit`, never dropped).
  `install_kernel` is `install_upto(..., "datum-module")`; `install_slice` is
  `install_upto(..., "datum-server")`. `KERNEL_AUDIT_RELS` includes the five
  `documents.*` tables and the three `print.*` tables so `audit_trigger_matrix`
  covers the product migrate path.
- Configuration manifest export/verify (`docs/03` §8).
- The composed kernel path (ADDENDUM 1).

## Kernel (Wave 2s)

```text
Kernel::builder(pool, profile)
    .register_machine(machine)?          // before freeze
    .register_hook(module, doc, edge, h) // before freeze
    .apply_manifest(&module_toml)?       // machines, routes, events, jobs
    .build().await?                      // freeze, persist, gate, events, jobs

Kernel::build(pool, profile).await?      // same, compiled-in catalog only
```

| Method | Role |
|---|---|
| `Kernel::spawn` | `Engine::spawn` after freeze |
| `Kernel::create_document` | `datum_documents::create` on the frozen engine (number allocated late) |
| `Kernel::new_document_revision` | insert revision and publish `documents.revision_created` in the same `Tx` |
| `Kernel::transition` | `esign::prepare` (when a token is present) then `Engine::transition` with the prepared gate; `datum.esign_id` is stamped so every audit row of the transition carries the signature id; a refusal aborts and `esign::log_refusal` is written on a fresh Tx. A successful `document.make_effective` also publishes `documents.effective` |
| `Kernel::signature_gate` | bound sync gate: `NoSignatures` under plain-shop, esign `Invalid` (never `NoProvider`) under regulated-device; `Engine::transition` callers do not choose the provider |
| `Kernel::signature_gate_factory` | `GateFactory` bound from the profile TOML `gate` field (`NoSignatures` or `datum-esign`) |
| `Kernel::posting_sink` / `bind_sink` | `PostingSink` factory; unfinalized Drop poisons via `datum_ledger::commit` |
| `Kernel::to_stock` / `convert` | `datum_uom` on the caller's `Tx` (a lot factor pinned earlier in that `Tx` is honoured) |
| `Kernel::publish_event` | outbox insert in the caller's `Tx` |
| `Kernel::dispatch_tick` / `worker_tick` | live events dispatcher + jobs worker as the service principal |
| `export_manifest` / `verify` | hashed configuration manifest |

Registries (machines, routes, event subscriptions, job kinds, permissions) are
populated from module manifests (`docs/03` §2 / §3), not from constants in this
crate, plus the kernel `document` machine (`datum_documents::document_machine`)
so regulated `approve` / `make_effective` are `Required` and go through
`Kernel::transition`'s prepared `GateFactory`. `Kernel::build` freezes only after
every enabled module has registered. Registration after freeze is
`datum_statemachine::Error::Frozen`.

`enable_genealogy_bridge` is wired when `mod-genealogy` is enabled; worker ticks
run `genealogy.refresh` under the system service principal.

Writes go through `datum_db::Tx`. Session-protocol SQL stays out of this crate
(CONTRACT §5a / §5a.1).
