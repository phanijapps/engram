# Plan: kg-redesign

## Tasks

### T1 — WebGL force-graph replaces the explorer as `/`
- **Tests:** goal-based — frontend typecheck/build; renders 12K nodes.
- **Approach:** `pnpm add react-force-graph-3d three-forcegraph` (WebGL, handles 10K+). New `src/routes/graph.tsx` as the default route. Fetches `/knowledge/overview` once, clusters by graph (source/repo), renders nodes colored by kind, edges colored by predicate. `three-forcegraph` handles node culling + LOD.

### T2 — Node detail panel
- **Tests:** goal-based — build; click node → panel.
- **Approach:** shadcn `Sheet` (right side) showing: name, kind, source file (from source_refs), provenance (source repo + git metadata), relationships (calls/mentions/defined_in list), confidence, last updated. Links to source files.

### T3 — Search + filter bar
- **Tests:** goal-based — build; filter by kind.
- **Approach:** Top-bar with: text search (entity name), kind filter (function/class/concept/requirement/value_stream/api), predicate filter (calls/mentions/defined_in), source filter (repo name).

### T4 — Performance virtualization
- **Tests:** goal-based — 12K nodes at 30+ FPS.
- **Approach:** `three-forcegraph` handles culling (nodes outside camera frustum are not rendered). Cap initial render to top 500 by degree; load more on zoom. Pre-compute degree on the backend.

### T5 — Index popup modal
- **Depends on:** T1
- **Approach:** Replace `/index` route with a shadcn `Dialog` triggered from the sidebar. Path input + force checkbox → POST /ingest/jobs → poll → toast on done. No live graph during indexing.

### T6 — Dashboard as side panel
- **Depends on:** T1
- **Approach:** Collapsible left panel (or `/dashboard` route) showing: tenant, indexed repos (name, git remote, branch, SHA, last updated, entity count), indexed document count, chunk count.
