# datum-documents

Controlled documents as a kernel capability. Schema `documents` (class `app`):
masters, reconstructible revisions, content-addressed immutable blobs, links to
the records they govern, and an approval machine registered with
`datum-statemachine`. No rendering (that is `datum-print`).

## Schema

| Table | Role |
|---|---|
| `documents.document` | Master: `kind`, gap-free `number`, `title`, `status` (machine only), `retention_class`, `legal_hold` |
| `documents.revision` | Version chain via `supersedes_revision_id`. In-place update is `status` only. Effectivity `from`/`until` with precision. Retention class stamped at insert. |
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

Status changes go through `Engine::transition`. The denormalized `status` column
is written only after the engine mutates `sm.instance`; BEFORE UPDATE triggers
refuse a status write that does not match the live instance (a raw status UPDATE
is fail-class) and refuse `Obsolete` while `legal_hold` is true.
`legal_hold = true` refuses `obsolete`. Under `NoSignatures` a Required edge
returns `SignatureError::NoProvider` and writes nothing.

## Blob layout

`DATUM_BLOB_ROOT/<aa>/<bb>/<hex>` where `aa`/`bb` are the first two hex pairs of
the SHA-256. Write-once, `fsync`, `verify_blob` recomputes the digest.

## API

| Function | Purpose |
|---|---|
| `create(tx, engine, kind, title, retention_class)` | Number allocated late inside the Tx; spawn `Draft` |
| `new_revision(tx, doc, label, manifest)` | Chain predecessor; overlapping effectivity refused |
| `attach(tx, store, rev, bytes, filename, media_type)` | Dedup blob by hash |
| `link(tx, rev, entity, record_id, kind)` | Governed-record link |
| `transition(tx, engine, gate, doc, edge, ctx, signature)` | Kernel machine |
| `history(tx, doc)` | Rebuild chain from `revision` rows |
| `effective_at(tx, doc, ts)` | Revision whose window contains `ts` |
| `verify_blob(store, hash)` | Detect on-disk corruption |
| `set_legal_hold(tx, doc, bool)` | Blocks Obsolete when true |
| `document_machine(profile)` | Composition-root registration |
| `manifest(profile)` | Permissions + event schemas |

Permissions: `documents.view`, `documents.edit`, `documents.approve`,
`documents.release`. Events: `documents.revision_created`, `documents.effective`.

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
