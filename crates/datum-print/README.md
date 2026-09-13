# datum-print

Deterministic HTML/PDF rendering for signed records. Schema `print` (class `app`):
`template` (versioned bodies + content hash), `render_log` (record ref, hashes,
template/renderer versions, optional archived blob hash), `install` (installation
profile id stamped at boot).

## Determinism

Rendering depends only on the record projection, template version, renderer
version, esign manifestation snapshots, and the stamped installation profile —
never wall-clock time or caller identity. Footer stamps `app_version` and
`config_version` from the bound transaction. `config_version` is the profile
**spec** version (`1.0.0` in both shipped profiles), not the profile id.

Document-revision fields and signature blocks come from the published
`datum_documents` / `datum_esign` render reads (`revision_for_render`,
`attachments_for_render`, `manifestation_for_record`). This crate does not
SELECT those crates' tables (R-2s-3).

## Template versioning

Bodies live as files under `templates/` and are seeded into `print.template`
(`body_hash` + `semantic_version`). A content change is a new row (bumped
integer `version` and semantic version). Customer overrides are a later spec.

## Formats

HTML is mandatory, deterministic, and has no external assets. PDF is mandatory
and produced by workspace-pinned `pdf-writer` 0.12 with fixed object ids and no
Info/ID/CreationDate timestamps.

## 11.50(b) gating

Signature blocks (printed name, executed UTC + signer zone, meaning, content
hash) and the `UNSIGNED` marker are emitted only when `print.install.profile_id`
is `regulated-device`. Plain-shop renders no signature block. The composition
root must call [`set_installation_profile`] with `profile.id` at boot — the
same moment it records `module.configuration`. Do not infer the profile from
`datum.config_version`.

## API (`Tx`)

| Function | Purpose |
|----------|---------|
| `render` | Produce `Rendered` bytes + `output_hash`; writes `render_log` |
| `archive` | Store bytes via caller-supplied `datum_documents::BlobStore` (same pattern as `documents::attach`); idempotent per output hash. No process-global blob root. |
| `manifestation_block` | `datum_esign::manifestation_for_record` (D-2b-2 snapshots, no live identity join) |
| `log` | List `render_log` rows for a record version |
| `set_installation_profile` | Boot stamp for 11.50(b) gating |

## Wire shapes (docs/10)

**POST** `/api/v1/print/render` (transport in `datum-server`):

```json
{
  "record": { "table": "documents.revision", "id": "…", "version": 1 },
  "format": "html",
  "template_id": "document_revision"
}
```

Response: `{ "output_hash": "<hex>", "template_version": 1, "renderer_version": "0.1.0", "bytes_base64": "…" }`.

**POST** `/api/v1/print/archive`:

```json
{
  "record": { "table": "…", "id": "…", "version": 1 },
  "output_hash": "<hex>"
}
```

Response: `{ "blob_hash": "<hex>" }`.
