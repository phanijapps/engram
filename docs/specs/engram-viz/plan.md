# Plan: engram-viz — code-graph visualization workspace

- **Spec:** [`spec.md`](spec.md)
- **Status:** Phase 1 Done (T1-T5); T7-T10 Draft

## Approach

Build engram-viz bottom-up: backend first (so the data flows), then the graph (the hero), then the panels (insights, node detail, timeline, search). Six tasks:

1. **Backend scaffold + engine wrapper.** Hono server on :3001 with a singleton `NativeKnowledgeEngine` wrapper. Scan + stats routes.
2. **Graph route + frontend scaffold.** `GET /api/graph` returns nodes + edges + communities. React + Vite + `react-force-graph-2d` renders it.
3. **Left sidebar: repo selector + insights.** Dead code, central/bridge symbols, communities — clickable to highlight.
4. **Right panel: node detail (CODE/INFO/HISTORY).** Node click → lazy-fetch detail.
5. **Search + timeline.** Top search bar; bottom timeline drawer.
6. **Polish + smoke.** Styling (Tailwind), responsive layout, dark mode, dev workflow.

## Tasks

### T1: Backend scaffold + engine wrapper + scan/stats routes
**Depends on:** none · **Mode:** goal-based
- `engram-viz/backend/` with `package.json` (hono, @engram/node, typescript), `tsconfig.json`.
- `src/lib/engine.ts`: singleton `NativeKnowledgeEngine` (opens the codegraph db; provides `scan(path)` + query methods that call N-API + parse JSON).
- `src/index.ts`: Hono app on `:3001` with CORS for the Vite dev server.
- `src/routes/scan.ts`: `POST /api/scan { path }` → engine.scan(path) → `{ files, entities, relationships }`.
- `src/routes/stats.ts`: `GET /api/stats` → `{ nodeCount, edgeCount }`.
- **Done when:** `curl localhost:3001/api/stats` returns JSON; `curl -X POST localhost:3001/api/scan -d '{"path":"codegraph/queries"}'` indexes a repo.

### T2: Graph route + frontend scaffold + hero graph
**Depends on:** T1 · **Mode:** goal-based
- `src/routes/graph.ts`: `GET /api/graph` → fetch entities (list_entities) + relationships (list_relationships) + communities (call_communities) → one payload `{ nodes: [...], links: [...], communities: { id: label } }`.
- `engram-viz/frontend/` with `package.json` (react, react-force-graph-2d, vite, tailwindcss, lucide-react, zustand), `vite.config.ts` (proxy :3001), `tsconfig.json`.
- `src/App.tsx`: full-viewport layout shell.
- `src/components/graph/GraphCanvas.tsx`: `react-force-graph-2d` rendering the `/api/graph` payload. Nodes colored by community; sized by a centrality proxy (degree). Edges = calls.
- `src/hooks/useGraphData.ts`: fetch + cache the graph payload.
- **Done when:** loading the page shows the force-directed graph for an indexed repo.

### T3: Left sidebar — repo selector + insights
**Depends on:** T2 · **Mode:** goal-based
- `src/routes/insights.ts`: `GET /api/insights` → dead_code + central_symbols + bridge_symbols in one payload.
- `src/components/sidebar/LeftSidebar.tsx`: repo selector dropdown (list_sources), node/edge counts, and three insight sections (dead code list, central symbols top-10, bridge symbols top-10). Clicking an insight item highlights/focuses the node in the graph.
- `src/lib/api.ts` + `src/lib/types.ts`: typed REST client.
- **Done when:** the sidebar shows insights and clicking a dead-code entry highlights the node.

### T4: Right panel — node detail (CODE/INFO/HISTORY)
**Depends on:** T3 · **Mode:** goal-based
- `src/routes/node.ts`: `GET /api/node/:id` → entity detail (get_entity) + source code (cyclomatic_complexity or the entity's chunk) + symbol_context (callers, callees, community).
- `src/components/panel/NodePanel.tsx`: right slide-in panel with three tabs.
- `CodeTab.tsx`: shows source code (pre-formatted).
- `InfoTab.tsx`: kind, file, line range, centrality score, community label, callers/callees.
- `HistoryTab.tsx`: temporal events for the entity (temporal_recent or valid_from if available).
- `src/hooks/useNodeDetail.ts`: lazy-fetch on node click.
- **Done when:** clicking a node opens the panel with all three tabs populated.

### T5: Search bar + timeline drawer
**Depends on:** T4 · **Mode:** goal-based
- `src/routes/search.ts`: `GET /api/search?q=...` → `searchCodeJson` BM25 results.
- `src/routes/timeline.ts`: `GET /api/timeline` → repo-wide temporal data (temporal_overview or entities grouped by valid_from).
- `src/components/search/SearchBar.tsx`: top bar with debounced search; results dropdown; click → focus node.
- `src/components/timeline/TimelineDrawer.tsx`: bottom slide-up drawer showing a horizontal timeline of symbol introductions.
- **Done when:** search returns results and clicking one focuses the node; timeline renders.

### T6: Polish — styling, dark mode, responsive, dev workflow
**Depends on:** T5 · **Mode:** visual / manual QA
- Tailwind CSS v4 setup + dark theme (matching the screenshots' dark aesthetic).
- Responsive layout (sidebar collapsible, panel resizable).
- `engram-viz/README.md` with setup + dev instructions.
- Root `package.json` workspace entries if needed.
- **Done when:** the app looks polished and matches the screenshot aesthetic; `pnpm dev` starts both servers.

## Rollout
- **Delivery:** a standalone web app under `engram-viz/`. No flag, no migration. Start with `pnpm dev` in backend + frontend.
- **Deployment sequencing:** T1→T2→T3→T4→T5→T6 strictly (each builds on the prior).

## Risks
- **N-API build:** `@engram/node` must be built (`pnpm run build` in `packages/node`) before the backend can import it. Document this prerequisite.
- **Graph performance:** large repos (1000+ nodes) may lag `react-force-graph-2d`. Mitigation: limit the initial render to top-N central nodes; add a "show all" toggle.
- **Source code retrieval:** the codegraph stores entities/chunks, not full source files. The CODE tab may show the chunk text (function body), not the full file. Document this honestly.

## Changelog
- 2026-07-11: initial plan (engram-viz — single-page graph-centric workspace; 6 tasks bottom-up).
- 2026-07-11: T1-T5 shipped (PR #27). T6 folded into T7 (polish + limitations). Added T7 (polish), T8 (taxonomy), T9 (ontology), T10 (advanced graph features) per user request.

### T7: Polish + Limitations
**Depends on:** T1-T5 (shipped) · **Mode:** visual / manual QA + goal-based
- Write `engram-viz/README.md` with setup, dev workflow, architecture overview.
- Add graph controls to `GraphCanvas.tsx`: zoom-to-fit button, node-count limiter (top-N central nodes + "show all" toggle), community filter checkboxes.
- Add node hover tooltips (name + kind + file).
- Make left sidebar collapsible (toggle button to maximize graph).
- Search warmup: start `indexForSearchJson` in a background task on server boot (non-blocking) so the index is warm by the time the user searches. Update the SearchBar notice to poll readiness.
- **Done when:** graph has controls + tooltips + collapsible sidebar; README exists; search warmup starts on boot.

### T8: Taxonomy view
**Depends on:** T7 · **Mode:** goal-based
- Backend: `src/routes/taxonomy.ts` — `GET /api/taxonomy` calls `listConceptsJson` + `getConceptSchemeJson` → returns `{ schemes: [...], concepts: [...] }` with broader/narrower/related relations. Handle empty gracefully.
- Frontend: `src/components/taxonomy/TaxonomyPanel.tsx` — a sidebar tab (toggle between "Insights" and "Taxonomy" in the left sidebar). Shows concept schemes as a collapsible tree (broader → narrower). Clicking a concept → highlights matching entities in the graph (filter by concept_refs or name match).
- Empty state: "No taxonomy concepts indexed — scan a repo with concept extraction enabled."
- **Done when:** taxonomy panel renders concepts as a tree; clicking highlights graph nodes; empty state is honest.

### T9: Ontology view + entity-kind legend
**Depends on:** T8 · **Mode:** goal-based
- Backend: `src/routes/ontology.ts` — `GET /api/ontology` calls `getOntologyJson` → returns `{ classes, properties, findings }`.
- Frontend: `src/components/ontology/OntologyPanel.tsx` — sidebar tab (Insights / Taxonomy / Ontology). Shows class definitions, property definitions, validation findings.
- Frontend: `src/components/graph/KindLegend.tsx` — floating legend showing EntityKind colors (Function/Struct/Trait/etc.). Clicking a kind filters the graph to only that kind.
- **Done when:** ontology panel renders; kind legend filters the graph.

### T10: Advanced graph features
**Depends on:** T9 · **Mode:** goal-based + manual QA
- Backend: extend `src/routes/node.ts` with `GET /api/node/:id/blast-radius` (calls `blastRadiusJson`) and add `src/routes/path.ts` — `GET /api/path?from=X&to=Y` (calls `dependencyPathJson`).
- Frontend: on node select, fetch blast radius → highlight transitive callers in a distinct color (amber/orange overlay). Add a "Path Finder" mode: click two nodes → render the shortest call path as a highlighted subgraph.
- Frontend: node grouping — a toggle in the graph controls to group nodes by EntityKind or by file-path prefix, with collapsible groups.
- **Done when:** blast radius highlights callers; path finder renders; grouping works.
