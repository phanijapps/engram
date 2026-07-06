# Plan: demo-reimagine

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec,
> this document is allowed to change as you learn. When it changes
> substantially, note why in the changelog at the bottom.

## Approach

Three independent workstreams inside the existing `demo/frontend` and
`demo/backend` packages. The prune (T1) is done first because it deletes the
old `/graph` (Three.js) view that the hero graph (T2–T4) replaces, so there is
no window where two graph implementations coexist. The MCP server (T5–T6) is
fully independent of the frontend and can proceed in parallel. The riskiest part
is the graph's community/containment logic — that is isolated into pure,
unit-tested functions so the force-rendering layer stays a thin presentational
shell verified by manual QA.

## Constraints

- `docs/architecture/reference.md`: the demo is the top TypeScript
  transport/ergonomics layer over the Rust core; this work adds no behavior that
  belongs in the core and touches no Rust.
- AGENTS.md: TypeScript must not redefine domain truth; the graph consumes the
  existing `/knowledge/graph-data` shape and the MCP server reuses the existing
  `@engram/node` transport factories.
- Pre-approved new deps: `graphology` + `graphology-communities-louvain`
  (frontend) only.

## Design (LLD)

### Design decisions

- **One 2D graph, no 3D.** `react-force-graph-2d` (d3-force) is the only graph
  renderer; the Three.js/Fibonacci-sphere `Graph3D` is deleted. Rejected keeping
  3D: the reference aesthetic is 2D community-clustered, and two graph stacks is
  the redundancy being pruned. Traces to: AC "renders via react-force-graph-2d",
  AC "no @react-three/three import".
- **Community = Louvain over calls/mentions + derived containment.** Containment
  edges (method→class→module, inferred from shared `sourcePath` + qualified
  name) are added with higher weight before running Louvain, so a class and its
  methods cluster. Rejected color-by-`kind` (not a cluster) and color-by-repo
  (too coarse). Traces to: AC "class + same-sourcePath methods share community".
- **Structural tier drives size + label, degree modulates within tier.**
  `kind → tier` is a fixed mapping (module/file/repo = L1, class/struct/trait =
  L2, method/function = L3, concept = L2-dim). Traces to: AC "module > class >
  method sizes; only L1/L2 always-labeled".
- **HTTP/SSE MCP over the SDK Web-standard transport.** The `/mcp` route uses
  `WebStandardStreamableHTTPServerTransport` (stateless — `sessionIdGenerator:
  undefined`), returning `transport.handleRequest(c.req.raw)` directly from Hono.
  Rejected stdio (an earlier iteration): the user wants an HTTP endpoint clients
  connect to, matching the original spec's HTTP/SSE intent. Traces to: AC "POST
  /mcp handles initialize", AC "initialize + tools/list + tools/call".

### Component / module decomposition

| Component | Location | Responsibility |
|---|---|---|
| `graph-model.ts` | `demo/frontend/src/lib/graph-model.ts` (new) | Pure: `tierForKind`, `assignCommunities(nodes, edges)`, `highlightSet(edges, id)`, palette mapping. No React. |
| `Graph` view | `demo/frontend/src/routes/graph.tsx` (rewrite) | Fetch `/knowledge/graph-data`, call graph-model, render `ForceGraph2D`. |
| `KnowledgeGraph.tsx` | `demo/frontend/src/components/knowledge-graph.tsx` (new) | The `ForceGraph2D` wrapper: node/label draw, cluster force, legend; reads highlight set from `graph-model`. |
| `app-sidebar.tsx` | `demo/frontend/src/components/layout/app-sidebar.tsx` (edit) | `NAV_ITEMS` trimmed to 5 (the command palette derives its list from `NAV_ITEMS`). |
| `router.tsx` | `demo/frontend/src/router.tsx` (edit) | Route table trimmed to 5 views. |
| `mcp-executors.ts` | `demo/backend/src/mcp-executors.ts` (new) | The four tool executor functions (index_repo, get_job, search, agentic_search) taking injected transports. Single source of truth. |
| `mcp-tools.ts` | `demo/backend/src/mcp-tools.ts` (new) | `registerEngramTools(server, deps)` — registers the executors as SDK tools; used by the `/mcp` HTTP route + the protocol test. |
| `/mcp` route | `demo/backend/src/app.ts` (edit) | `app.all("/mcp")` → fresh `McpServer` + `WebStandardStreamableHTTPServerTransport` (stateless) per request; `registerEngramTools`; returns `handleRequest(c.req.raw)`. |
| `scan-defaults.ts` | `demo/backend/src/scan-defaults.ts` (new) | Shared `SCAN_SCOPE`/`SCAN_POLICY`/`SCAN_ACTOR` so the HTTP ingest route and MCP path send the ingest engine identical, policy-bearing payloads. |

### Data & schema

No schema or contract change. The graph consumes the existing
`/knowledge/graph-data` node/edge shape. Containment is derived in
`graph-model.ts` from `node.sourcePath` + `node.name` (qualified-name prefix),
never persisted.

### Interfaces & contracts

MCP tool schemas are declared in code via the SDK (zod input shapes); there is
no `contracts/` artifact (MCP is not one of the repo's contract types). The four
tool names and their argument shapes match the existing hand-rolled definitions
in `app.ts` so behavior is preserved.

### State & control flow

Graph view: fetch → `assignCommunities` + `tierForKind` decorate nodes →
`ForceGraph2D` renders → hover sets a highlight set (node + neighbors) that the
node/link paint functions read. MCP: `POST /mcp` → fresh `McpServer` +
`registerEngramTools` → `WebStandardStreamableHTTPServerTransport.handleRequest`
handles `initialize`/`tools/*` over HTTP (stateless, per-request).

### Failure, edge cases & resilience

- Empty graph (no entities) → view renders an empty-state, no crash.
- Node with no `sourcePath` → its own singleton containment (no false grouping).
- MCP tool throws → SDK returns an error result, the backend stays up.
- `/mcp` is stateless (no session store) → no cross-request state to corrupt;
  each request builds a fresh server + transport.

## Tasks

### T1: Prune to five views + drop the Three.js stack

**Depends on:** none

**Touches:** `demo/frontend/src/routes/{explorer,ingest,knowledge,repo-index}.tsx`,
`demo/frontend/src/Graph3D.tsx`, `demo/frontend/src/components/graph-3d-card.tsx`,
`demo/frontend/src/graph3d.css`, `demo/frontend/src/router.tsx`,
`demo/frontend/src/components/layout/app-sidebar.tsx`,
`demo/frontend/src/components/layout/command-palette.tsx`,
`demo/frontend/package.json`

**Tests:**
- Goal-based: after deletion, `grep -rE "routes/(explorer|ingest|knowledge|repo-index)|Graph3D|graph-3d-card|graph3d\.css|@react-three|react-force-graph-3d|from ['\"]three['\"]" demo/frontend/src` returns no references (AC: all four routes gone, 3D files deleted, no `three` import). The `routes/knowledge` anchor avoids matching the new `knowledge-graph.tsx` component.
- Goal-based: `pnpm --filter demo-frontend typecheck` passes (AC: typecheck clean).
- Goal-based: `NAV_ITEMS` contains exactly the 5 labels in order, and the command palette (which derives its list from `NAV_ITEMS`) renders exactly those 5 (AC: nav lists exactly 5).

**Approach:**
- Delete the four route files and remove their entries from `router.tsx`.
- Delete `Graph3D.tsx`, `graph-3d-card.tsx`, `graph3d.css`; leave `routes/graph.tsx` as a stub that T2 rewrites (or delete its 3D imports now).
- Trim `NAV_ITEMS` (app-sidebar.tsx) and the command palette list to Dashboard, Graph, Chat, Memory, Belief.
- Remove `three`, `@react-three/fiber`, `@react-three/drei`, `react-force-graph-3d`, `@types/three` from `demo/frontend/package.json`; run install.
- Bundled cleanup (same-area, mechanical): remove only the shadcn components whose *only* importers were the four deleted views (orphaned by this change) — leave pre-existing unused components alone. The `demo/README.md` view list is handled in T6.

**Done when:** the three goal-based checks pass and the app boots to the 5-view sidebar (Graph temporarily empty until T2).

---

### T2: Pure graph model — tiers, communities, palette

**Depends on:** none

**Touches:** `demo/frontend/src/lib/graph-model.ts` (new),
`demo/frontend/src/lib/graph-model.test.ts` (new),
`demo/frontend/package.json` (+ graphology deps)

**Tests:**
- `tierForKind("module") > tierForKind("class") > tierForKind("function")` by base size; module/class marked `alwaysLabel: true`, function `false` (AC: tier sizes + labels).
- `tierForKind("concept")` → class tier, `alwaysLabel: false`; `tierForKind("totally-unknown")` → method tier, smallest size, `alwaysLabel: false` (AC: fallback tier).
- `assignCommunities` with a class and two methods sharing a `sourcePath` → all three share the same community membership, with the Louvain `rng` seeded so the result is deterministic (AC: containment grouping, seeded).
- `assignCommunities` on two disconnected call-clusters → the two clusters have *different* community membership (assert cluster-A members ≠ cluster-B members, not raw id values).
- Node with empty `sourcePath` → not grouped with unrelated nodes (singleton containment).
- `highlightSet(edges, hoveredId)` → the hovered id plus its direct neighbors, for a known adjacency (AC: hover highlight set is a pure unit-tested function).
- Palette maps community id → a stable color; ids beyond palette length wrap deterministically.

**Approach:**
- Add `graphology` + `graphology-communities-louvain` to `demo/frontend/package.json`.
- `tierForKind(kind): { tier: 1|2|3, baseSize: number, alwaysLabel: boolean }` — fixed mapping; `concept` → tier 2 (not always-labeled); any unrecognised kind → tier 3 (smallest, not labeled).
- `deriveContainmentEdges(nodes)` — for each method/function node, link to the class/module node sharing its `sourcePath` (and qualified-name prefix where present); weight higher than call edges.
- `assignCommunities(nodes, edges)` — build a `graphology` graph from call/mention edges + containment edges, run `communities-louvain` **with a fixed `rng` seed** for determinism, return `Map<nodeId, communityId>`.
- `highlightSet(edges, hoveredId)` — pure adjacency lookup returning `Set<nodeId>` (hovered + direct neighbors).
- `colorForCommunity(id)` — index into the theme palette with wraparound.

**Done when:** `pnpm --filter demo-frontend test graph-model` is green on all cases above.

---

### T3: KnowledgeGraph component — force render, tiers, labels, hover

**Depends on:** T2

**Touches:** `demo/frontend/src/components/knowledge-graph.tsx` (new)

**Tests:**
- Manual QA (visual): the concrete observables from the spec Objective — module/class hubs large and always-labeled, methods small and unlabeled until hover, clusters colored per community and corresponding to modules/classes.
- Goal-based: component compiles and imports only `react-force-graph-2d` (no three/@react-three) — covered by T1's grep + typecheck.

**Approach:**
- `ForceGraph2D` with `nodeCanvasObject`: radius from `tierForKind(...).baseSize` scaled by degree; fill from `colorForCommunity`; draw label when `alwaysLabel` or `globalScale`/hover threshold met.
- `linkColor`/`linkWidth` preserve the deterministic-vs-LLM provenance cue already used.
- Hover: call `highlightSet(edges, hoveredId)` from `graph-model` (pure, unit-tested in T2); dim non-highlighted nodes/links in the paint functions.
- Cluster separation: add a weak per-community centering force via `d3Force('x'/'y')` keyed on community id, plus tuned charge/link-distance.
- Small legend (community swatches + "size = degree" hint).

**Done when:** manual QA against an indexed repo confirms the class-level reading described in the spec Objective's concrete observables.

---

### T4: Wire the Graph route to the new component

**Depends on:** T1, T2, T3

**Touches:** `demo/frontend/src/routes/graph.tsx` (rewrite)

**Tests:**
- Goal-based: `pnpm --filter demo-frontend build` passes (AC: build clean).
- Manual QA: `/graph` fetches `/knowledge/graph-data`, decorates nodes via graph-model, renders `KnowledgeGraph`; structural-only toggle still filters.

**Approach:**
- Keep the existing fetch of `POST /knowledge/graph-data` and the structural-only toggle.
- Replace `buildGraphData`/`Graph3D` usage with `assignCommunities` + `tierForKind` decoration feeding `<KnowledgeGraph>`.

**Done when:** build passes and the hero graph renders end-to-end from real data.

---

### T5: Shared tool executors + SDK registration, reused by route + server + test

**Depends on:** none

**Touches:** `demo/backend/src/mcp-executors.ts` (new),
`demo/backend/src/mcp-tools.ts` (new), `demo/backend/src/mcp-tools.test.ts` (new),
`demo/backend/src/app.ts` (refactor `/mcp` route to call the executors)

**Tests:**
- In-memory SDK `Client`↔`Server` (`InMemoryTransport`): `initialize` returns a result advertising `tools` capability (AC: initialize).
- `tools/list` returns exactly `index_repo`, `get_job`, `search`, `agentic_search` (AC: tools/list).
- `tools/call` for each tool returns an SDK-compliant `content` result, with injected transport stubs returning canned data (AC: tools/call each tool).

**Approach:**
- `mcp-executors.ts`: extract the four tool bodies currently inline in `app.ts` (`/mcp` `tools/call` branch, app.ts:557-604) into named executor functions `(deps, args) => result`, where `deps` are the ingest/knowledge/qa transport factories (injected so tests pass stubs). Single source of truth.
- Refactor the existing `/mcp` route in `app.ts` to call these executors instead of its inline copies — the route stays (Ask-first honored), but the logic no longer has a second copy.
- `mcp-tools.ts`: `registerEngramTools(server, deps)` registers each executor via `server.registerTool(name, { inputSchema }, handler)` using zod shapes matching the current tool definitions; handlers wrap the executor result as `{ content: [{ type: "text", text: ... }] }`.

**Done when:** `pnpm --filter demo-backend test mcp-tools` is green and the `/mcp` route calls the shared executors (no inline tool-body duplication remains).

---

### T6: Streamable HTTP `/mcp` route + README

**Depends on:** T5

**Touches:** `demo/backend/src/app.ts` (`/mcp` route), `README.md`
(Connect via MCP), `demo/README.md`

**Tests:**
- HTTP-level (`mcp-http.test.ts`): `app.request("/mcp")` with an `initialize`
  POST returns 200 and a valid MCP result advertising `tools` capability
  (AC: POST /mcp handles initialize).
- Live smoke: start the backend, `curl`/`fetch` `initialize` + `tools/list` +
  `tools/call index_repo` over the real socket; assert the handshake, the four
  tools, and a jobId with no Policy error.
- Manual QA: add the `/mcp` URL to Copilot's `http` transport; `tools/list` and
  a `tools/call` succeed (AC: works in Copilot — deferred manual QA).

**Approach:**
- `app.all("/mcp")`: `new McpServer(...)`; `registerEngramTools(server,
  buildToolDeps(SCAN_SCOPE))`; `new WebStandardStreamableHTTPServerTransport({
  sessionIdGenerator: undefined })`; `await server.connect(transport)`; return
  `transport.handleRequest(c.req.raw)`. Replaces the hand-rolled JSON-RPC branches.
- Rewrite the root `README.md` "Connect via MCP": HTTP config for Copilot
  (`type: http`, `url: http://localhost:8787/mcp`), remove the "handshake gap"
  admission. Update `demo/README.md` view list to the 5 views (done in T1/T6).

**Done when:** the HTTP-level test + live smoke pass and the README documents the
HTTP endpoint.

## Rollout

Pure demo-layer change; no infra, no migration, no flag. The graph and MCP are
additive/replacement within the demo packages. Rollback = revert the PR; no
persisted state changes (the graph is read-only over existing data, the MCP
server reads the existing DB). The native addon must be built for the MCP server
to run — an existing requirement, documented in the README.

## Risks

- **Louvain community instability** — small graphs can produce singleton
  communities; mitigated by the containment edges pulling structural members
  together and by wraparound palette. Tunable in manual QA.
- **Force-layout perf at the 500-node cap** — `react-force-graph-2d` handles
  this range; if sluggish, lower the cap or freeze the layout after cooldown.
- **Stateless MCP per-request cost** — a fresh `McpServer` + transport is built
  per `/mcp` request. Fine at demo scale; if it matters, cache a server or move
  to a session-managed transport.

## Changelog

- 2026-07-04: initial plan
- 2026-07-04: follow-up defect fixes + transport change. (1) `index_repo` sent
  `startScanJob` no `policy` → Rust "invalid type: null, expected struct Policy";
  fixed by a shared `scan-defaults.ts` (SCAN_SCOPE/POLICY/ACTOR) used by both the
  HTTP ingest route and the MCP path, and `indexRepo` now sends the full payload
  (regression test in `mcp-executors.test.ts`). (2) Added a `/healthz` alias
  (`/health` was already correct; the bad ref was in external guidance). (3) Per
  user, swapped the MCP transport from stdio to **Streamable HTTP** (SDK
  Web-standard transport, stateless) on `/mcp`; deleted `mcp.ts` + the `bin`;
  `mcp-tools.ts`/`mcp-executors.ts`/`mcp-deps.ts` are reused by the HTTP route.
  Verified: 29 backend tests + HTTP-level test + a live socket smoke
  (initialize + tools/list + real index_repo).
- 2026-07-04: implemented. Light review (demo work): one adversarial pass.
  Reconciled the tier map to include `project`/`organization` (tier 1) and
  `variable`/`api` (method tier) — updated the spec AC to match and added tests.
  Rebuilt the `@engram/node` wrapper + native addon (stale artifacts predating
  this work) so the backend typechecks and all 27 backend tests pass. Two
  manual-QA ACs (visual render, live Copilot) deferred to
  `docs/backlog.md#demo-reimagine-manual-qa` — mechanical proxies (unit tests,
  in-memory MCP protocol tests, stdout-clean initialize smoke) are green.
- 2026-07-04: spec-review pass 1 — AC13 retargeted to root README (+ separate
  demo/README AC); added `@types/three` to removal AC; added unknown-kind
  fallback-tier AC + test; seeded Louvain RNG and switched cluster test to
  membership assertion; extracted `highlightSet` into the pure graph-model with
  its own TDD case; introduced shared `mcp-executors.ts` so the retained `/mcp`
  route and the stdio server share one tool implementation; fixed T7→T6, grep
  pattern for bare `three`, palette-derives-from-NAV_ITEMS test wording, and
  dropped the unanchored "reference aesthetic" phrase.
