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
- `KERNEL_ORDER` and `run_migrations` / `migrate_prefix` / `migrate_suffix` /
  `install_kernel` (privileged before identity seeds).
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
| `Kernel::transition` | wraps the executor with the profile `SignatureGate`; one `GroupBuilder` is `bind_tx`'d, hooks contribute, `datum_ledger::post` writes the group in the same `Tx` |
| `Kernel::signature_gate` | bound from the profile TOML `gate` field (`NoSignatures` until `datum-esign`) |
| `Kernel::posting_sink` / `bind_sink` | `PostingSink` factory; unfinalized Drop poisons via `datum_ledger::commit` |
| `Kernel::to_stock` / `convert` | `datum_uom` on the caller's `Tx` (a lot factor pinned earlier in that `Tx` is honoured) |
| `Kernel::publish_event` | outbox insert in the caller's `Tx` |
| `Kernel::dispatch_tick` / `worker_tick` | live events dispatcher + jobs worker as the service principal |
| `export_manifest` / `verify` | hashed configuration manifest |

Registries (machines, routes, event subscriptions, job kinds, permissions) are
populated from module manifests (`docs/03` §2 / §3), not from constants in this
crate. `Kernel::build` freezes only after every enabled module has registered.
Registration after freeze is `datum_statemachine::Error::Frozen`.

`enable_genealogy_bridge` is wired when `mod-genealogy` is enabled; worker ticks
run `genealogy.refresh` under the system service principal.

Writes go through `datum_db::Tx`. Session-protocol SQL stays out of this crate
(CONTRACT §5a / §5a.1).
