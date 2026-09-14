# 0010. One capability registry generates the router, the document, and the agent surface

Audience: contributor. Status: absent.

Status:   **Proposed**
Date:     2026-09-13
Decider:  project owner

## Context

Four goals were added to the project (`GOALS.md`): the system must be legible to an AI
agent, every function must have an API, a shop must be able to migrate off an incumbent
ERP, and both implementation and contribution must be followable by a stranger.

Five independent analyses of the tree were run, each blind to the others, each on a
different slice. Three of them arrived at the same root cause from three different
directions:

- The agent-legibility analysis found that a building agent that trusts the compiled-in
  catalogue implements the wrong state machine, because `compiled_in()` inlines a stub
  manifest that does not match `modules/production_min/module.toml:25-52`
  (`crates/wicket-module/src/manifest.rs:433-466`).
- The API-coverage analysis found three catalogues that already disagree: the
  hand-written axum table (`crates/wicket-server/src/http.rs:18-128`), a second
  hand-written table that generates the OpenAPI document
  (`crates/wicket-server/src/openapi.rs:29-397`), and the module manifests that the
  server never reads (`crates/wicket-module/src/kernel.rs:215-221`).
- The governance analysis found the same dual manifest source and concluded that a
  contributor can edit `module.toml` correctly and still leave the composition root
  wrong (`crates/wicket-module/src/manifest.rs:408-567`).

All three also found, separately, that `[[routes]]` declares a `method` which the parser
discards (`crates/wicket-module/src/manifest.rs:107-114`, `:323-331`). A route catalogue
that cannot tell a read from a write cannot drive a coverage gate.

The existing parity test does not catch any of this. `openapi_matches_router`
(`crates/wicket-server/tests/slice.rs:1543-1568`) compares the served document to
`mounted_operations()`, and both sides are derived from the same table. It is circular:
it proves the table equals itself and never reads the router.

The consequence is that Goals 1 and 2 cannot be satisfied independently. An agent that
asks the system what it can do gets one answer from the manifest, a different answer
from the document, and a third from the router. A coverage test has no authority to
walk. A migration has no catalogue to map onto.

## Decision

**There is one capability registry. It is derived from the module manifests, it carries
the HTTP method, and the router, the OpenAPI document, and the agent introspection
surface are all generated from it.**

Concretely, and in this order:

1. `module.toml` is the single source of truth for a module's identity, permissions,
   machines, jobs, subscriptions and routes. `compiled_in()` reads those files rather
   than embedding copies of them.
2. `[[routes]]` carries `method`, and the manifest parser stores it.
3. A capability table is assembled from the manifests plus the kernel's own operations.
   Each row names the capability, its kind, its HTTP method and path, the permission it
   needs, and the machine edge, job kind or CLI verb it fulfils.
4. The HTTP router is generated from that table. A hand-written mount that the table did
   not produce fails a lint.
5. The OpenAPI document is generated from the same table, and the parity test reads the
   router rather than a second copy of the list.
6. The agent introspection surface publishes that same table, plus the legal outgoing
   edges for a live record, and is hashed into the configuration manifest.

Handlers stay hand-written. This decision governs where a capability is **declared and
mounted**, not how it is implemented.

## Consequences

**What it costs.** The composition root is rewritten once, and every module manifest is
touched to add `method`. The three existing catalogues must be collapsed, which is a
breaking change to internal structure even though it is invisible on the wire. The
duplicate work-order prefixes (`crates/wicket-server/src/http.rs:48-72`) must be
resolved rather than left as an alias, which is a wire-visible change. Generating the
router adds a build-time step that contributors must understand before they can add an
endpoint, raising the floor for a first contribution.

The real cost is that a capability becomes harder to add casually. Mounting a route will
require declaring it. That is the point, and it will feel like friction to the person
who just wants to expose one handler.

**What it buys.** Goals 1 and 2 stop being two projects and become one. Drift between
what the system does and what it says it does becomes impossible rather than merely
discouraged, because there is one source and not three. The coverage gate that Goal 2
needs has something to walk. The agent surface that Goal 1 needs has something to
publish. The migration that Goal 3 needs has a catalogue to map onto. The pull-request
gate that Goal 4 needs has a mechanical question to ask.

It also retires a class of bug that has already occurred three times in this tree and
was found three times independently.

**What it does not do.** It does not make the API complete. Mounting the capabilities
that exist but are unreachable is separate work, sequenced in `TODO.md`. It does not
decide which internal functions are exempt; that is the allowlist, and its growth is
governed by `GOALS.md` API-10.

## Alternatives considered

**Keep three catalogues and add a reverse-diff lint.** Cheapest. A lint extracts mounts
from the router and compares them to the document and the manifests. It was the planned
approach before this analysis. It detects drift but does not prevent it, and it leaves
the agent surface with no single thing to publish. Rejected as a destination, accepted
as the first step: the lint is worth landing immediately because it makes the current
drift visible while the registry is built.

**Generate everything from the handlers with a procedural macro.** Attractive, and it
keeps declaration next to implementation. Rejected because the manifest must remain
readable by a program that is not compiling the tree, which is what a building agent and
a migration mapping both need. A macro puts the truth inside the binary.

**Keep the router hand-written and generate only the document.** This is roughly the
status quo and it is what produced the circular test. Rejected: the router is the thing
that is actually true at runtime, so it cannot be the only artifact nobody checks.

**Do nothing until a module ships from outside the project.** Rejected. The drift is
already present with six first-party modules and one maintainer. It will not get cheaper
to fix with strangers in the tree.

## Revisit if

Modules become runtime-loaded plugins rather than compiled-in crates
(`docs/03-module-system.md:162-184` defers this), because a registry assembled at build
time would then be assembled at load time and the generation step changes shape.

Or if the capability table grows a second legitimate consumer whose needs conflict with
HTTP, such as a streaming or batch transport, in which case the table may need to
describe capabilities independently of their HTTP binding rather than as routes.
