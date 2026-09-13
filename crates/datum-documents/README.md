# datum-documents

Controlled documents as a kernel capability. Schema `documents` (class `app`):
masters, reconstructible revisions, content-addressed immutable blobs, links to
the records they govern, and an approval machine registered with
`datum-statemachine`. No rendering (that is `datum-print`).

This crate does not read another crate's schema (R-2s-3). Live machine state
goes through `datum_statemachine::current_state` / `instance_exists` on the
sealed `Tx`.

## Schema

| Table | Role |
|---|---|
| `documents.document` | Master: `kind`, gap-free `number`, `title`, `status` (insert-time snapshot; live value is the machine), `retention_class`, `legal_hold` |
| `documents.revision` | Version chain via `supersedes_revision_id`. Insert-only. Effectivity `from`/`until` with precision. Retention class stamped at insert. |
| `documents.blob` | SHA-256 primary key, `byte_size`, `stored_at`. Never updated, never deleted. |
| `documents.attachment` | `revision_id`, blob hash, filename, media type, size. Insert-only. |
| `documents.link` | `revision_id` → `entity` + `record_id` + link kind. Insert-only. |

`datum_app` has no `DELETE`. `ON DELETE CASCADE` is absent. Bytes live on disk
(`BlobStore` / `FsBlobStore`); the database holds the hash.

## Machine

`document_machine(profile)` — `doc_type = "document"`.

| Edge | From | To | Permission | `regulated-device` | `plain-shop` |
|---|---|---|---|---|---|
| `submit` | Draft | InReview | `documents.edit` | NotRequired | NotRequired |
| `approve` | InReview | Approved | `documents.approve` | **Required** meaning `Approved` | NotRequired |
| `make_effective` | Approved | Effective | `documents.release` | **Required** meaning `Responsible` | NotRequired |
| `revise` | Effective | Draft | `documents.edit` | NotRequired | NotRequired |
| `supersede` | Effective | Superseded | `documents.release` | NotRequired | NotRequired |
| `obsolete` | Effective | Obsolete | `documents.release` | NotRequired | NotRequired |
| `void` | Draft | Void | `documents.edit` | NotRequired | NotRequired |

Status changes go through `Engine::transition`. `load` overlays live status from
`current_state`. BEFORE UPDATE triggers refuse any `status` write (a raw status
UPDATE is fail-class) and refuse `Obsolete` / `Superseded` while `legal_hold` is
true. `set_legal_hold(true)` refuses `obsolete` and `supersede`. Under
`NoSignatures` a Required edge returns `SignatureError::NoProvider` and writes
nothing.

## Blob layout

`DATUM_BLOB_ROOT/<aa>/<bb>/<hex>` where `aa`/`bb` are the first two hex pairs of
the SHA-256. Write-once, `fsync`, `verify_blob` recomputes the digest.

`attach` inserts the `documents.blob` row in the Tx, then `put`s bytes. After a
rollback the composition root calls `BlobStore::discard_uncommitted` (or
`discard_unreferenced_blob`) so no orphan file remains.

## API

Composition-root shape. Extra `&Engine` / `&dyn BlobStore` / `&dyn SignatureGate`
parameters versus the SPEC argument list are injected by the kernel install
graph (lane `2b2-glue` / `datum-module`). This crate does not bind them itself.

| Function | SPEC list | This crate |
|---|---|---|
| `create` | `(tx, kind, title, retention_class)` | `create(tx, engine, kind, title, retention_class)` — number allocated late; spawn `Draft` |
| `new_revision` | `(tx, doc, label, manifest)` | same — chain predecessor; overlapping effectivity refused |
| `attach` | `(tx, rev, bytes, filename, media_type)` | `attach(tx, store, rev, bytes, filename, media_type)` — DB row then `put` |
| `link` | `(tx, rev, entity, record_id, kind)` | same |
| `transition` | `(tx, doc, edge, ctx, signature)` | `transition(tx, engine, gate, doc, edge, ctx, signature)` — kernel machine |
| `history` | `(tx, doc)` | same — rebuild chain from `revision` rows |
| `effective_at` | `(tx, doc, ts)` | same — `[from, until)`; `NULL` from is unbounded past; `NULL` until is unbounded future; both `NULL` is not in force |
| `verify_blob` | `(hash)` | `verify_blob(store, hash)` |
| `set_legal_hold` | `(tx, doc, bool)` | same — blocks Obsolete and Superseded when true |
| `document_machine(profile)` | composition-root registration | same |
| `manifest(profile)` | permissions + event schemas | same |

Permissions: `documents.view`, `documents.edit`, `documents.approve`,
`documents.release`.

### Events

Exported schemas (not published here):

- `documents.revision_created` v1 — `document_id`, `revision_id`, `label`
- `documents.effective` v1 — `document_id`, `revision_id`, `effective_from`

Emission is the composition root's: `datum-module` (kernel install / lane
`2b2-glue`) registers the schemas and publishes. This crate does not depend on
`datum-events`.

## Wire shape (`docs/10`)

```json
{
  "id": "01932c5a-8b10-7001-8000-000000000010",
  "kind": "SOP",
  "number": "SOP-0001",
  "title": "Work instruction — MDS-450-M4x12",
  "status": "Effective",
  "retention_class": "quality-record",
  "legal_hold": false,
  "revision": {
    "id": "01932c5a-8b10-7001-8000-000000000011",
    "label": "A",
    "supersedes": null,
    "status": "Effective",
    "effective_from": "2024-08-02T00:00:00Z",
    "effective_until": null,
    "precision": "day",
    "attachments": [
      {
        "id": "01932c5a-8b10-7001-8000-000000000012",
        "filename": "SOP-0001-A.pdf",
        "media_type": "application/pdf",
        "byte_size": 18432,
        "hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      }
    ]
  }
}
```

Attachments use a separate upload route (not a JSON body on floor POSTs);
`docs/10` §8 names this crate as the owner.
