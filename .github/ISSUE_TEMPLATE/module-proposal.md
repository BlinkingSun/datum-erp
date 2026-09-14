---
name: Module proposal
about: Propose a new module under modules/
title: ''
labels: 'type:module-proposal, area:module'
assignees: ''
---

<!-- Modules are compiled-in workspace crates, not runtime plugins
     (docs/03-module-system.md section 7). A module contribution is a crate
     pull request and must ship the full set in CONTRIBUTING.md section 5. -->

## Module identity

- Proposed identifier (`mod-<name>`):
- Crate name (`wicket-mod-<name>`):
- Schema name (`<name>`):
- Regulated (`true` or `false`):

## What it owns

<!-- Which entities and which postings. A module that stores a running quantity
     is rejected: inventory is postings (PLAN.md section 6 invariant 1). -->

## Catalog position

<!-- Which phase in docs/04-module-catalog.md, and why now. -->

## Dependencies

<!-- Which kernel crates. A new third-party dependency requires a request for
     comment and an allowlist edit in the same change. -->

## Signature declarations

<!-- Every machine edge needs a total signature declaration. Under the regulated
     profile a required edge with no signature gate fails at startup. -->
