# Spec: demo-reimagine

- **Status:** Shipped
- **Owner:** @phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** none
- **Contract:** none
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

The engram demo tells one "art of the possible" story through five views —
**Dashboard**, **Graph** (the hero), **Chat**, **Memory**, **Belief** — reachable
from the sidebar and command palette. Indexing a repository turns it into a
knowledge graph that a developer reads at the class level, asks questions of,
remembers, and reconciles.

Three outcomes define done:

1. **A pruned, coherent UI.** The sidebar and command palette list exactly the
   five views above. The Explorer, Ingest, Knowledge (ontology validation), and
   RepoIndex views are gone, along with the Three.js/3D graph stack that only
   the old graph used. The demo builds and type-checks with no dead routes,
   no orphaned nav entries, and no unused graph dependencies.

2. **A hero knowledge graph that reads at the class level.** A single 2D
   force-directed view (built on `react-force-graph-2d`) renders the scope's
   entities as a community-clustered network. Node size and label prominence
   encode structural level: modules/files/repositories are the largest,
   always-labeled nodes; classes/structs/traits are medium, always-labeled;
   methods/functions are small satellites labeled only on hover or zoom-in. No
   node is finer-grained than a method. Nodes are colored by cluster, and
   containment derived from `sourcePath` and qualified names keeps a class and
   its methods, and a module and its classes, visually grouped. Hovering a node
   highlights it and its neighbors and dims the rest; the existing
   structural-only toggle is preserved.

3. **An MCP server GitHub Copilot can use over HTTP.** The demo backend serves a
   spec-compliant **Streamable HTTP** MCP endpoint at `/mcp`, built on
   `@modelcontextprotocol/sdk`'s Web-standard transport. It completes the
   `initialize` handshake, answers `tools/list`, and executes `tools/call` for
   the four existing tools (`index_repo`, `get_job`, `search`, `agentic_search`).
   It runs stateless (no session store) and is reachable by any HTTP MCP client
   — GitHub Copilot's `http` transport — at `http://localhost:8787/mcp`. Tool
   requests go through the shared executors, so the ingest engine always receives
   a complete, policy-bearing payload.

## Boundaries

### Always do

- Keep the sidebar `NAV_ITEMS`, the command palette, and the router route table
  in sync — every listed view routes, every route is listed (there are no
  intentional non-nav utility routes after this change).
- Route all MCP protocol handling through `@modelcontextprotocol/sdk`
  (`McpServer` + `WebStandardStreamableHTTPServerTransport`); never hand-roll
  JSON-RPC framing.
- Register MCP tools once via `registerEngramTools` over the shared executors, so
  the ingest engine always receives a complete, policy-bearing payload.
- Derive node structural tier from entity `kind`; derive containment grouping
  from `sourcePath` + qualified name on the client.
- Keep the graph fed by the existing `POST /knowledge/graph-data` response shape.

### Ask first

- Adding any frontend or backend dependency beyond `graphology` +
  `graphology-communities-louvain` (frontend) — those two are pre-approved.
- Changing the `POST /knowledge/graph-data` request or response contract.
- Deleting or renaming backend routes (the deleted *views* keep their backend
  routes; removing a route is a separate decision).
- Changing the `/mcp` route's transport away from the SDK Web-standard
  Streamable HTTP transport (the hand-rolled JSON-RPC branches are replaced by
  it, not kept alongside).

### Never do

- Render any graph node finer-grained than a method/function (no lines,
  statements, or expression-level nodes).
- Reintroduce a second graph view or a second graph rendering library — one 2D
  graph on `react-force-graph-2d` is the only graph.
- Add a new top-level workspace package or module boundary for this work — it
  lives inside the existing `demo/frontend` and `demo/backend` packages.
- Hand-roll MCP JSON-RPC framing or the `initialize`/session handshake — the
  SDK transport owns the protocol.
- Break the accepted engram domain contracts or touch the Rust core.

## Testing Strategy

- **UI prune completeness — goal-based check.** `pnpm --filter demo-frontend
  typecheck` and `pnpm --filter demo-frontend build` pass; `grep` confirms the
  deleted routes, nav entries, and Three.js imports/dependencies are gone. A
  build that still references a deleted module fails, so the build is the gate.
- **Community + tier assignment — TDD.** The client functions that (a) assign
  each node a community id from the edge list plus derived containment and
  (b) map an entity `kind` to a structural tier and base size are pure
  (`nodes+edges → {nodeId: communityId}` and `kind → tier`), so they are unit
  tested directly, including the containment-grouping invariant (a class and its
  same-`sourcePath` methods land in one community).
- **Graph reads at the class level — manual QA.** Run the demo against an
  indexed repo and confirm the rendered graph matches the intent: module/class
  hubs labeled and large, methods small and unlabeled until hover, colored
  clusters that correspond to modules/classes. Recorded by observing the running
  app, not a unit assertion.
- **MCP protocol compliance — TDD.** An in-memory SDK `Client`↔`Server` pair
  drives `initialize`, `tools/list`, and `tools/call` for each of the four tools
  and asserts SDK-compliant responses, with the engram transports stubbed so the
  test is deterministic and needs no database or native addon.
- **MCP over HTTP — TDD + manual QA.** An HTTP-level test drives `initialize`
  against `POST /mcp` (the real route + Streamable HTTP transport). Manual QA:
  with the backend running, add the `/mcp` URL to Copilot's `http` transport and
  confirm `tools/list` and a `tools/call` succeed end-to-end.

## Acceptance Criteria

- [x] The sidebar `NAV_ITEMS` and the command palette each list exactly:
  Dashboard, Graph, Chat, Memory, Belief — in that order.
- [x] The routes `/explorer`, `/ingest`, `/knowledge`, and `/index` no longer
  exist in the router, and their route component files are deleted.
- [x] `Graph3D.tsx`, `graph-3d-card.tsx`, and `graph3d.css` are deleted, and
  `three`, `@react-three/fiber`, `@react-three/drei`, `react-force-graph-3d`,
  and `@types/three` are removed from `demo/frontend/package.json`.
- [x] `pnpm --filter demo-frontend typecheck` and `pnpm --filter demo-frontend
  build` both pass with no references to any deleted module or dependency.
- [x] The `/graph` view renders via `react-force-graph-2d`; there is no
  remaining `@react-three` or `three` import anywhere in `demo/frontend/src`.
- [x] Given a graph with entities of kinds module, class, and method, the tier
  function assigns module > class > method base sizes, and only module-tier and
  class-tier nodes are marked always-labeled.
- [x] The structural map is: tier 1 (largest, always-labeled) =
  module/file/repository/project/organization; tier 2 (medium, always-labeled) =
  class/struct/trait/interface/enum; tier 3 (smallest, not labeled) =
  method/function/variable/api. `concept` maps to the class tier but is not
  always-labeled. Any kind outside this map falls back to the method tier
  (smallest, not always-labeled).
- [x] Given two methods sharing a `sourcePath` with their class, the community
  assignment is deterministic across repeated runs and places the class and both
  methods in the same community (same membership).
- [x] The hovered-node highlight set (the node plus its direct neighbors) is
  produced by a pure function, unit-tested against a known adjacency; in the
  rendered graph, hovering highlights that set and dims all other nodes, and the
  structural-only toggle still filters to structural kinds.
- [x] Registered on an SDK server, the tools answer `initialize` with a result
  advertising `tools` capability and `tools/list` with the four tools
  `index_repo`, `get_job`, `search`, `agentic_search` (in-memory transport test).
- [x] A `tools/call` for each of the four tools returns an SDK-compliant
  `content` result (tool logic stubbed in the test).
- [x] `index_repo` forwards a non-null `policy` (and `actor`) to `startScanJob`,
  matching the HTTP `/ingest/jobs` payload — pinned by a regression test.
- [x] `POST /mcp` handles an `initialize` request via the Web-standard
  Streamable HTTP transport and returns a valid MCP result (HTTP-level test).
- [x] The root `README.md` "Connect via MCP" section documents the HTTP endpoint
  config for GitHub Copilot and no longer claims an unresolved handshake gap.
- [x] `demo/README.md`'s view list reflects the five current views (no stale
  Cytoscape / four-panel description).
- [ ] (manual QA) (deferred: demo-reimagine-manual-qa) Rendered against an
  indexed repo, the `/graph` view reads at the class level: module/class hubs
  large and always-labeled, methods small and unlabeled until hover, and nodes
  visibly colored by cluster.
- [ ] (manual QA) (deferred: demo-reimagine-manual-qa) With the backend running,
  the `/mcp` HTTP endpoint added to GitHub Copilot's `http` transport completes
  the handshake and answers a `tools/list` and at least one `tools/call`
  end-to-end (including a real `index_repo`).

## Assumptions

- Technical: `react-force-graph-2d@^1.29.1` is installed and retained; the
  Three.js stack (`three`, `@react-three/fiber`, `@react-three/drei`,
  `react-force-graph-3d`) is installed and removed by this feature
  (demo/frontend/package.json).
- Technical: `graphology` + `graphology-communities-louvain` are new frontend
  dependencies added by this feature (node_modules check — both absent;
  user confirmation 2026-07-04).
- Technical: `@modelcontextprotocol/sdk@^1.29.0` is installed (ESM), exposes
  `McpServer` (`server/mcp.js`) and `WebStandardStreamableHTTPServerTransport`
  (`server/webStandardStreamableHttp.js`) whose `handleRequest(req: Request)`
  returns a `Response` — the SDK's documented Hono usage (require.resolve + SDK
  JSDoc example).
- Technical: `POST /knowledge/graph-data` node output already includes
  `id,name,kind,graphId,degree,sourcePath,repo,aliases,confidence`, so
  containment is derivable client-side with no backend change
  (demo/backend/src/app.ts:311-321).
- Technical: MCP tools reach engram via `@engram/node` against `ENGRAM_DB` and a
  built native addon; the `/mcp` HTTP endpoint is served by the demo backend
  (stateless Streamable HTTP), sharing the backend's DB (demo/backend/src/engram.ts
  import chain).
- Process: `docs/architecture/reference.md` is the stack source of truth; the
  demo is its top TypeScript transport/ergonomics layer over the Rust core
  (docs/architecture/reference.md).
- Product: the demo targets an internal "art of the possible" audience
  (user framing 2026-07-04).
