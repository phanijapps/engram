# engram-viz — session context (2026-07-13)

> Working context for the engram-viz web workspace and the codegraph layer it
> sits on. Captures the work done in the 2026-07-13 session so a fresh reader
> (or a future session) can pick up without re-deriving it.

## What engram-viz is

A single-page, graph-centric code visualization workspace:
- **Backend** (`engram-viz/backend`) — Hono REST server on `:3001`, a thin
  transport over the `@engram/node` native codegraph engine
  (`NativeKnowledgeEngine`), which wraps the Rust `SqlKnowledgeStore` +
  `LexicalIndex`. Data lives in `~/.engram/codegraph-mem-alpha.db`.
- **Frontend** (`engram-viz/frontend`) — React + Vite on `:5173`,
  `react-force-graph-2d`, Tailwind dark theme, zustand store.
- The hero view is the **call graph**: nodes = entities on `calls` edges,
  Louvain-community-colored, degree-sized. Overlay panels: left sidebar
  (repo selector + insights/taxonomy/ontology), node detail, timeline, search.

Run: `cd engram-viz/backend && npm run dev` and `cd engram-viz/frontend && npm run dev`.

## Key data model fact (load-bearing)

Entities are partitioned by `knowledge_graphs.stable_source_key`
(e.g. `github.com/phanijapps/engram`) — **not** by `KnowledgeSource.id`
(`source-…`, which is metadata about one ingestion event). The graph
`?source=` filter and `entitiesBySource()` filter by the **stable_source_key**.
`/api/sources` returns `engine.repos()` (distinct stable keys + counts), so
the repo dropdown's option values are stable keys that flow correctly into the
filter. Getting this wrong produces a repo dropdown that shows 0 entities.

## Session work (2026-07-13)

### 1. Repo-switching dropdown
- **Problem:** the sidebar showed static text (`stats.sources[0].name`); no way
  to switch repos; and `/api/sources` returned the source *id*, which doesn't
  match the graph filter → `entityCount: 0`.
- **Fix:** `engine.repos()` (direct SQLite, groups `knowledge_graphs` by
  `stable_source_key` with entity/relationship counts); `/api/sources` returns
  it; frontend `<select>` with "All repositories" + per-repo options; store
  `sourceFilter` + refetch graph on change (`useGraphData`).
- **Files:** `engram-viz/backend/src/lib/engine.ts` (`repos()`,
  `repoDisplayName`), `backend/src/index.ts` (`/api/sources`),
  `frontend/src/components/sidebar/LeftSidebar.tsx`, `frontend/src/store/graphStore.ts`,
  `frontend/src/hooks/useGraphData.ts`, `frontend/src/lib/{api,types}.ts`.

### 2. Ingesting a second repo (agentzero)
- Scanned `~/projects/agentzero` via `POST /api/scan` → 14,178 entities /
  19,156 relationships (repo identity `github.com/phanijapps/zbot`).
- **How agentzero uses engram:** a dedicated adapter crate
  `stores/zbot-engram-adapter` depends on engram core crates by path
  (`engram-domain/-memory/-knowledge/-belief/-hierarchy/-integration/-conformance`)
  and wires `EngramProvider` + `bootstrap_provider` + the repository traits
  behind zbot store interfaces (stores / mapping / governance / migration /
  capabilities). `.codex/config.toml` also points Codex at
  `engram-codegraph-mcp`. Textbook host-SDK pattern.

### 3. Graph snappiness — Strategy 1 (data diet)
With 2 repos (~18k entities) `/api/graph` shipped a 4.5 MB payload and the UI
blocked on parse. Spec: `docs/specs/engram-viz-graph-perf/spec.md`.

| Lever | Change |
|---|---|
| Degree cap | `?maxNodes=` (default **2000**): top-N nodes by degree; pruned links dropped; response carries `capped` + `originalNodeCount` |
| gzip | `hono/compress` middleware |
| Communities cache | `engine.communityCache` (Louvain computed once, invalidated on scan); `DEFAULT_MAX_PASSES` constant |
| Slim payload | graph node → `{id,name,kind,file,community,degree}` (dropped line/endLine/complexity/conceptRefs — served by `/api/node/:id`) |
| Visible degree | post-cap `degree` recomputed from surviving edges so canvas sizing matches what's drawn |
| Startup freeze | removed `engine.prewarmLexical()` startup call (it blocked the event loop) |
| UI notice | "N nodes of M (capped)" in the top bar when `capped` |

**Measured (2 repos):** wire 4.57 MB → **358 KB** (gzip); steady-state
`/api/graph` 520 ms → **~50–90 ms**; zbot switch 770 ms → ~330 ms.
- **Files:** `backend/src/routes/graph.ts`, `backend/src/lib/engine.ts`,
  `backend/src/index.ts`, `frontend/src/{App.tsx,lib/types.ts,store/graphStore.ts,hooks/useGraphData.ts}`.

### 4. Lexical-index 218 s freeze (root-cause bug)
- **Symptom:** first `/api/search` (and everything queued behind it, including
  `/api/graph` and `/api/search/ready`) froze the whole server — observed at
  **218 s** server-side.
- **Root cause:** `LexicalIndex::upsert` called `writer.commit()` once per
  document; `index_for_search_json` looped over all entities → **~18k Tantivy
  commits** (each finalizes a segment + reloads the reader under
  `OnCommitWithDelay`) → O(n²) build.
- **Fix:** added `LexicalIndex::upsert_batch(entries)` (one commit for the
  whole corpus) and switched the binding to it. Kept `upsert` for incremental
  single-doc updates. Two new unit tests; all 12 lexical tests green.
- **Result:** full-corpus build **~218 s → 810 ms** (first), 1.6 ms warm. No
  `worker_threads` offload needed (sub-second synchronous build is fine).
- **Files:** `adapters/retrieval/tantivy-lexical/src/index.rs`,
  `bindings/node/src/knowledge.rs`. Native binding rebuilt
  (`cargo build --release -p engram-node`) and copied to
  `packages/node/engram_node.node` via `scripts/build-native.mjs`.

### 5. React StrictMode double API calls
- **Symptom:** on load, every endpoint (`/api/graph`, `/api/stats`,
  `/api/sources`, `/api/insights`) fired **twice** (9 requests).
- **Root cause:** `<React.StrictMode>` double-invokes mount effects in dev.
- **Fix:** kept StrictMode (valuable dev guard; production doesn't double-call)
  and added **concurrent-request dedupe** at the API layer (`getJson` in
  `frontend/src/lib/api.ts`) — identical concurrent GETs share one in-flight
  promise, cleared on settle. Verified in Chrome DevTools: 9 → 5 requests.
- **Files:** `engram-viz/frontend/src/lib/api.ts`.

## Verification gates used
- Rust: `cargo test -p engram-store-lexical` (12 pass), `cargo check --release`.
- TS: `npm run typecheck` (backend + frontend, both clean).
- Live: `curl` timings for `/api/graph`, `/api/search`, `/api/search/ready`,
  `/api/sources`; Chrome DevTools network for the double-call fix.
- Native binding rebuilt and copied after the lexical change.

## Deferred (see `docs/backlog.md` → `engram-viz-graph-perf`)
- **C7 — focus-node-pruned-by-cap:** clicking an insight whose node was pruned
  by the degree cap silently no-ops recenter. Fix is a design call (refetch the
  node's neighborhood with `?maxNodes` disabled, or an expand affordance).
- **Strategy 2 — overview-first supergraph:** aggregate Louvain communities
  into super-nodes (~30–60 bubbles) with drill-down, so 18k entities read at a
  glance. Designed but not implemented this session.
- Lexical worker-thread offload is **moot** after the batch-upsert fix (noted
  for the record only; revisit if the corpus grows ~100×).

## REPL tips
- DB: `sqlite3 ~/.engram/codegraph-mem-alpha.db`. Entity file lives in
  `sourceRefs[].location.path` (not a top-level `file`) for engram-viz-scanned
  repos; the MCP-scanned engram repo has a top-level `file`.
- Relationships: `predicate` and `subject/object` are nested in
  `knowledge_relationships.record_json` (no `predicate` / `object_id` columns).
- Backend dev = `tsx watch` (auto-reloads); restart cleanly by killing all
  `engram-viz/backend` tsx procs + freeing `:3001` (stale node workers can
  linger and contended-bind, masquerading as a crash).
