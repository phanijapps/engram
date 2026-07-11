# engram-viz

A web-based, single-page **code-graph visualization workspace** that turns an indexed repository into an interactive knowledge graph. The graph is the hero — always visible, full-viewport — with community-colored nodes sized by centrality, calls-edges, and overlay panels for insights, taxonomy, ontology, node detail, search, and timeline.

## Architecture

```
Frontend (React + Vite SPA)        Backend (Hono REST server)
        │                                  │
        │  HTTP /api/*                      │  N-API calls
        ▼                                  ▼
  Vite dev proxy :5173 ──►  Hono on :3001  ──►  @engram/node  ──►  Rust  ──►  SQLite
                                                                          (~/.engram/codegraph-*.db)
```

- **Backend** (`backend/`): A thin Hono REST server on `:3001`. Each route proxies one codegraph operation via the `@engram/node` N-API binding. No business logic — pure transport. For taxonomy and ontology data, which the N-API surface exposes only by-ID, the backend uses Node's built-in `node:sqlite` for ID discovery.
- **Frontend** (`frontend/`): A React + Vite single-page app. Talks only to the REST API — no direct N-API or SQLite access. Uses `react-force-graph-2d` for the graph, `zustand` for state, and Tailwind CSS for the dark IDE aesthetic.

## Prerequisites

1. **Node.js >= 22** (for `node:sqlite` support in the backend).
2. **@engram/node built**: the native N-API binding must be compiled before the backend can import it.
   ```bash
   # From the repository root:
   pnpm run build:native
   ```
3. **An indexed codegraph database** at `~/.engram/codegraph-mem-alpha.db` (or set `ENGRAM_DB`). You can index a repo through the UI (type a path and click Scan) or via the codegraph MCP server / ingest tools.

## Dev workflow

Two terminals:

```bash
# Terminal 1 — backend (Hono on :3001)
cd engram-viz/backend
pnpm install
pnpm dev

# Terminal 2 — frontend (Vite on :5173, proxies /api to :3001)
cd engram-viz/frontend
pnpm install
pnpm dev
```

Open `http://localhost:5173`. If the database is empty, type a repository path and click **Scan**. The graph appears once entities and `calls` relationships are indexed.

### Environment variables

| Variable    | Default                                          | Purpose                        |
| ----------- | ------------------------------------------------ | ------------------------------ |
| `PORT`      | `3001`                                           | Backend listen port.           |
| `ENGRAM_DB` | `~/.engram/codegraph-mem-alpha.db`               | Path to the codegraph SQLite db. |

## Features

### Phase 1 (T1–T5)
- Full-viewport force-directed call graph (nodes = symbols, edges = `calls` relationships).
- Community-colored nodes (Louvain), sized by degree centrality.
- Left sidebar: repo selector, node/edge counts, clickable insights (dead code, central symbols, bridge symbols).
- Right node-detail panel: CODE (source text), INFO (kind, file, community, callers/callees), HISTORY (temporal provenance).
- Top search bar with BM25 keyword search.
- Bottom timeline drawer with symbol-introduction histogram.

### Phase 2 (T7–T10)
- **Graph controls**: zoom-to-fit, node-count limiter (top-N by degree), community visibility filter, path-finder mode, group-by-kind toggle.
- **Node hover tooltips**: name, kind, file.
- **Collapsible left sidebar**: maximize graph space.
- **Search warmup**: the BM25 index is pre-warmed on backend boot in a background task. The search bar polls `/api/search/ready` and shows a warming indicator until ready.
- **Taxonomy view** (`/api/taxonomy`): concept schemes and concepts rendered as a broader→narrower tree. Clicking a concept highlights matching graph nodes.
- **Ontology view** (`/api/ontology`): class definitions, property definitions, and validation axioms.
- **Entity-kind legend**: floating legend (bottom-right). Clicking a kind filters the graph to only that kind.
- **Blast radius**: when a node is selected, the INFO tab has a "Blast Radius" button that highlights all transitive callers in amber.
- **Dependency path**: path-finder mode — click two nodes to trace the shortest call path between them.
- **Node grouping**: group-by-kind toggle color-codes nodes by EntityKind.

## Known limitations

- **Graph performance**: repos with 1000+ call-graph nodes may lag `react-force-graph-2d`. The node-count limiter (default: top 200 by degree) mitigates this.
- **Source text**: the CODE tab shows the chunk text (function body) the codegraph indexed, not the full source file. Some declarations (traits, type aliases) have no extractable chunk.
- **Taxonomy and ontology**: these views are empty unless concept schemes or ontologies have been explicitly indexed into the codegraph database. Code indexing does not produce taxonomy/ontology data.
- **Timeline sparseness**: a single-pass index produces one introduction spike. Rich timelines require incremental re-scans over time.
- **Search first-query latency**: although the index is pre-warmed on boot, if the backend was just started, the first search may still wait for the warm-up to complete. The search bar shows a warming indicator.

## Project structure

```
engram-viz/
  backend/
    src/
      index.ts              Hono app, route registration, search warmup on boot
      lib/engine.ts         Singleton N-API wrapper + SQLite taxonomy/ontology discovery
      lib/ingest.ts         Background scan job wrapper
      routes/
        scan.ts             POST /api/scan
        stats.ts            GET /api/stats
        graph.ts            GET /api/graph
        insights.ts         GET /api/insights
        node.ts             GET /api/node/:id, GET /api/node/:id/blast-radius
        search.ts           GET /api/search
        timeline.ts         GET /api/timeline
        taxonomy.ts         GET /api/taxonomy
        ontology.ts         GET /api/ontology
        path.ts             GET /api/path?from=X&to=Y
  frontend/
    src/
      App.tsx               Layout shell
      store/graphStore.ts   Zustand state
      lib/api.ts            Typed REST client
      lib/types.ts          Response type definitions
      lib/colors.ts         Community + kind color helpers
      hooks/
        useGraphData.ts     Fetches graph + stats on mount
        useNodeDetail.ts    Lazy-fetches node detail on click
      components/
        graph/
          GraphCanvas.tsx   Force-directed graph + tooltips + highlight overlay
          GraphControls.tsx Zoom-to-fit, node limit, community filter, path mode, group toggle
          KindLegend.tsx     Floating entity-kind legend (click to filter)
          PathStatus.tsx     Path-finder mode status bar
        sidebar/
          LeftSidebar.tsx   Repo selector, counts, tabbed panels
          InsightCard.tsx   Reusable insight list
        taxonomy/
          TaxonomyPanel.tsx Concept tree (broader→narrower)
        ontology/
          OntologyPanel.tsx Class/property/axiom definitions
        panel/
          NodePanel.tsx     Right slide-in detail panel
          CodeTab.tsx       Source code view
          InfoTab.tsx       Facts + callers/callees + blast radius
          HistoryTab.tsx    Temporal provenance
        search/
          SearchBar.tsx     Top search with readiness polling
        timeline/
          TimelineDrawer.tsx Bottom timeline drawer
```
