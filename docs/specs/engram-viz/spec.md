# Spec: engram-viz — code-graph visualization workspace

- **Status:** Shipped (T1-T5); T7-T9 Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** none (new standalone module under `engram-viz/`)
- **Brief:** none (authored directly — inspired by memtrace screenshots in `/home/videogamer/Documents/screenshots/`)
- **Contract:** none — a web app, not a Rust library contract.
- **Shape:** mixed (backend service + frontend UI)

> **Spec contract:** this document defines what "done" means.

## Objective

A web-based, single-page **code-graph visualization workspace** that turns an indexed repository into an interactive knowledge graph a developer explores at the class level. The graph is the hero — always visible, full-viewport — with community-colored nodes sized by centrality, calls-edges, and overlay panels that slide in: a left sidebar (repo selector, stats, insights), a right node-detail panel (CODE / INFO / HISTORY tabs), a top search bar, and a bottom timeline drawer. The backend is a thin Hono REST server that proxies engram's codegraph operations via `@engram/node` N-API; the frontend is React + Vite + `react-force-graph-2d`.

## Directory structure

```
engram-viz/
  backend/     Hono REST server (TypeScript, Node)
  frontend/    React + Vite SPA (TypeScript)
```

Clean code. No prototype or demo carry-over — fresh modules.

## Boundaries

### Always do
- Thin backend: one Hono route per codegraph operation; JSON in → JSON out; no business logic in the backend.
- The backend uses `@engram/node` (`NativeKnowledgeEngine`) for ALL data operations — it does not call Rust directly or manage SQLite.
- The frontend is a pure SPA — it talks ONLY to the Hono REST API; it has no direct N-API or SQLite access.
- Graph data is ONE payload: nodes + edges + community labels, fetched once on repo load, rendered by `react-force-graph-2d`.
- Node click → lazy-fetch the detail (code, info, timeline) — don't pre-load all details.

### Ask first
- Add WebSocket / SSE for live re-index updates.
- Add multi-repo overlay (two repos on one graph).
- Add auth / multi-user.

### Never do
- Put business logic in the Hono backend (it's a transport, not a second implementation).
- Add a database to the backend — the data lives in the codegraph SQLite db via N-API.
- Use Three.js / 3D — the screenshots show 2D; `react-force-graph-2d` only.
- Add routing / multi-page navigation — this is a single-page workspace.

## Testing Strategy
- **Backend routes — goal-based check.** Each route returns the expected JSON shape for a known indexed repo. Verified by `curl` / integration test against a running server.
- **Frontend components — manual QA.** The graph renders, node click opens the panel, insights highlight nodes, timeline renders. Visual QA against the screenshots.
- **Build — goal-based check.** `pnpm --filter engram-viz-backend build && pnpm --filter engram-viz-frontend build` both succeed.
- **Smoke — manual QA.** Start backend + frontend, load the page, see the graph.

## Acceptance Criteria

### Phase 1 (T1-T5) — Shipped
- [x] `engram-viz/backend/` is a Hono server on `:3001` that exposes REST routes: `POST /api/scan`, `GET /api/graph`, `GET /api/insights`, `GET /api/node/:id`, `GET /api/timeline/:id`, `GET /api/search`, `GET /api/stats`. Each proxies to `@engram/node` N-API. Builds with `tsc`.
- [x] `engram-viz/frontend/` is a React + Vite SPA that renders a full-viewport `react-force-graph-2d` graph with community-colored nodes, centrality-sized nodes, and calls-edges.
- [x] Left sidebar: repo selector dropdown, node/edge counts, and an insights list (dead code, central symbols, bridge symbols — each clickable to highlight nodes in the graph).
- [x] Node click opens a right slide-in panel with three tabs: CODE (source text), INFO (kind, file, centrality score, community), HISTORY (temporal events — when introduced/modified).
- [x] Top search bar: BM25 keyword search via `/api/search` → results list, clicking a result focuses the node.
- [x] Bottom timeline drawer: repo-wide temporal view (symbols introduced over time) via `/api/timeline`.
- [x] The app starts with `cd engram-viz/backend && pnpm dev` (backend) + `cd engram-viz/frontend && pnpm dev` (frontend, Vite proxies to :3001). Loading the page shows the graph after scanning a repo.
- [x] No prototype/demo code carried over — clean modules.

### Phase 2 (T7) — Polish + Limitations
- [ ] **README** (`engram-viz/README.md`) with setup, dev, and architecture overview.
- [ ] **Graph controls**: zoom-to-fit button, node-count limiter (top-N by degree + "show all"), community filter (toggle communities visible/hidden).
- [ ] **Node hover tooltips**: show name + kind + file on hover.
- [ ] **Collapsible left sidebar**: toggle to maximize graph space.
- [ ] **Search warmup fix**: persist the Tantivy lexical index to disk (or pre-build on backend startup in a background task) so the first search isn't ~120s. At minimum, pre-warm the index on server boot.

### Phase 3 (T8) — Taxonomy view
- [ ] `GET /api/taxonomy` route: lists concept schemes + concepts (via `listConceptsJson` + `getConceptSchemeJson` N-API methods). Returns `{ schemes: [...], concepts: [...] }` with broader/narrower/related relations.
- [ ] **Taxonomy panel** in the frontend: a collapsible panel (or sidebar tab) showing concept schemes as a tree (broader → narrower). Clicking a concept highlights entities tagged with that concept in the graph.
- [ ] If no taxonomy data is indexed, the panel shows an honest empty state ("No taxonomy concepts indexed — scan a repo with concept extraction enabled").

### Phase 4 (T9) — Ontology view
- [ ] `GET /api/ontology` route: lists ontology classes + properties + validation findings (via `getOntologyJson` N-API). Returns `{ classes: [...], properties: [...], findings: [...] }`.
- [ ] **Ontology panel** in the frontend: class definitions (entity types in this codebase), property definitions, and validation findings (entities that don't conform to the ontology — e.g., a Function with no file).
- [ ] **Entity-kind legend**: the graph shows an EntityKind legend (Function, Struct, Trait, etc.) — clicking a kind filters the graph to only that kind.

### Phase 5 (T10) — Advanced graph features
- [ ] **Blast radius**: when a node is selected, highlight its transitive callers (blast_radius via N-API) in a distinct color.
- [ ] **Dependency path**: a search-to-search path finder — select two nodes, see the shortest call path between them (dependency_path via N-API).
- [ ] **Node grouping**: group nodes by EntityKind (Function/Struct/Trait/...) or by file path, with collapsible groups.

## Assumptions
- Technical: `@engram/node` exports `NativeKnowledgeEngine` with 47 N-API methods (scan, dead_code, central_symbols, bridge_symbols, call_communities, search_code, temporal_*, list_entities, list_relationships, etc.) (source: `bindings/node/src/knowledge.rs`).
- Technical: `react-force-graph-2d` is the proven graph library (used in the demo; the screenshots show 2D) (source: demo/frontend/package.json).
- Technical: the codegraph SQLite db at `~/.engram/codegraph-mem-alpha.db` holds the indexed graph (source: MCP server config).
- Design: single-page graph-centric workspace — graph always visible, panels overlay (source: screenshots + user confirmation 2026-07-11).
- Process: fresh modules under `engram-viz/` — no prototype/demo carry-over (source: user instruction 2026-07-11).
