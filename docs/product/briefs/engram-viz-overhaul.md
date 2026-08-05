# Brief: engram-viz overhaul — zbot-style Memory/Observatory/Graph over engram data

- **Slug:** `engram-viz-overhaul`
- **Received:** 2026-08-03
- **Owner:** engram-viz
- **Source:** session request — "engram-viz overhaul; mirror zbot's Memory/Observatory/Graph tabs exactly; backed by engram-mcp data (2 GB) vs zbot's ~100 MB; use the mcp well; build from scratch on `packages/`; plan the backend"
- **Shape:** A — spec-granular; one row per tab/layer.
- **Status:** Ready

> Rebuild engram-viz from scratch as a 3-tab browser app that mirrors zbot
> (`agentzero-autonomy-ledger/apps/ui`) Memory / Observatory / Graph surfaces in
> exact styling and interaction, but backed by engram's `agentzero` store
> (~2 GB single-file SQLite + vector index). The reference UI is React 19 +
> Tailwind v4 + ~8 k-line hand-written CSS ("futuristic dark void + cyan
> accent"), whose three tabs are **routes**; engram-viz today is a single-surface
> code-call-graph workspace (react-force-graph-2d, Tailwind v3 GitHub-dark) that
> **bypasses engram-mcp entirely** — it uses `@engram/node` in-process against a
> *different* store. Research confirms the "better path": a thin Hono HTTP/SSE
> Backend-for-Frontend loading `@engram/node` in-process (**not** engram-mcp for
> the browser — MCP is an LLM-agent transport that returns Debug-text blobs over
> stdio), with **deck.gl** WebGL level-of-detail rendering over server-pre-
> aggregated Leiden/Louvain communities. Note engram's data is mostly **indexed
> code** (170 k entities / 227 k relationships / 423 k chunks); the
> memory/belief/hierarchy/procedure tables are **empty**, so Memory &
> Observatory are sparse until populated.

## Outcome

A user opens engram-viz and sees, in zbot's exact visual style, three tabs over
engram's live `agentzero` data: a **Graph** tab rendering engram's knowledge
graph performantly at full 2 GB scale via WebGL level-of-detail (community
meta-nodes → drill-to-neighborhood); a **Memory** tab over engram's
memory/belief/procedure surfaces; and an **Observatory** tab over
graph/belief/hierarchy health — with smooth interaction (no full-graph dumps,
no freezes) backed by a Node BFF that mirrors zbot's REST/SSE contracts and
reads structured data in-process from `@engram/node`.

## Success metrics

> Proposed defaults — correct me:

- Graph tab renders an overview of engram's full graph (170 k+ nodes) at a
  stable 30–60 FPS by rendering community meta-nodes, **never raw nodes**;
  drilling a community fetches a bounded neighborhood (cap K ≈ 50–100) with
  sub-second latency.
- No browser request ever receives an unbounded `list*` payload — every list
  endpoint is keyset-paginated and/or aggregated server-side.
- Memory/Observatory render real data when their engram tables are populated,
  and a correct empty-state when not.
- The UI is visually indistinguishable in styling/layout from zbot's three
  tabs (same theme tokens, fonts, component classes, route shell).
- The backend reads engram via `@engram/node` in-process (structured JSON) and
  never parses engram-mcp Debug strings for browser data.

## Scope / Non-goals

**In scope:**

- Greenfield rebuild of engram-viz (frontend + backend), building on `packages/`
  (`@engram/node` transports; `@engram/contracts` types).
- Port zbot's styling system (`theme.css` / `components.css` / `effects.css`,
  Tailwind v4, Fraunces/IBM Plex/JetBrains Mono) and the 3-route tab shell.
- **Graph tab**: deck.gl WebGL, LOD over pre-computed communities, on-demand
  neighborhoods, entity detail, legend/controls.
- **Memory tab**: facts/beliefs/contradictions/procedures over engram
  memory + belief surfaces; ward/content deck; hybrid search; empty-states.
- **Observatory tab**: graph/belief/hierarchy health + stats; learning-health
  bar; slideovers.
- A Hono BFF mirroring zbot's REST contracts, reading `@engram/node` in-process,
  with server-side aggregation + keyset windowing. (SSE streaming is deferred to
  S2–S4; S1 ships REST + keyset/aggregation only.)
- TS view-type definitions for the graph/belief/hierarchy/ontology/taxonomy
  records that `@engram/contracts` currently lacks.

**Non-goals (proposed — confirm):**

- Do **not** route browser data through engram-mcp; it remains the LLM-agent
  surface only.
- Do **not** modify engram's Rust core/contracts beyond optional typed
> view-additions; no second backend.
- Do **not** replicate zbot's non-viz tabs (chat/research) or its gateway
  runtime — only the Memory/Observatory/Graph UI surfaces and their data
  contracts.
- Populating engram's empty belief/hierarchy/procedure tables is out of scope
  for the *viz* (tracked separately); the viz must handle their empty states
  gracefully. Optional: run `hierarchy_build` to give Observatory real data.

## Appetite

Multi-week, quality-first (confirmed). Greenfield rebuild + 3 tabs + performant
LOD + backend BFF. Staged delivery is natural — Graph tab first (where the
2 GB lives), then Memory/Observatory.

## Current state (gap analysis, 2026-08-03)

Grounded in codebase + data + research. Full detail in
`docs/research/large-graph-viz-transport-survey.md`.

| # | Area | State | Evidence / gap |
|---|---|---|---|
| 1 | Reference UI (zbot) | ✅ exists | `agentzero-autonomy-ledger/apps/ui`: React 19 + Router 7 + Tailwind v4 + ~8 k-line CSS; 3 route-tabs (`/memory`, `/observatory` 2D-D3, `/observatory-v2` "Graph" 3D-r3f); REST + WS transport to a gateway |
| 2 | Target app (engram-viz) | 🟡 wrong-shape | `mem-alpha/engram-viz`: single-surface code-call-graph (react-force-graph-2d, Tailwind v3 GitHub-dark); no top-level tabs; bypasses engram-mcp |
| 3 | Data source | ✅ exists | `~/.engram/agentzero/engram_data.db` — 2.0 GiB single-file SQLite + `lexical/` + 53 k × 384-dim vectors |
| 4 | Data-kind mismatch | ⚠️ key risk | 2 GB is indexed **code**: 423 k chunks / 227 k relationships / 170 k entities (hermes/zbot/pi/engram repos). memory/belief/hierarchy/procedures tables **empty** (24 memories). Graph tab is where the data lives |
| 5 | engram-mcp transport | ❌ wrong for UI | stdio JSON-RPC only; returns Rust Debug text blobs; no HTTP; MCP spec = LLM-agent ("human in the loop") transport. Browser must **not** speak MCP |
| 6 | `@engram/node` in-process | ✅ exists | provider + knowledge + belief transports, structured JSON, reaches more surfaces than the TS MCP. **Gaps:** no pagination on `list*` (firehose), no bulk counts/aggregations, community detection only on the knowledge engine |
| 7 | `packages/contracts` | 🟡 partial | has memory/retrieval/chunk/event types only; **no** graph/belief/hierarchy/ontology/taxonomy record types → viz must define view types |
| 8 | Server analytics | ✅ exists | `engram-graph-analytics` (PageRank/betweenness/communities) + `community-summary` adapter + knowledge-engine analytics (dead-code/central/bridge/blast-radius) — LOD primitives exist |
| 9 | Styling port | ❌ gap | must port `theme/components/effects.css` + migrate Tailwind v3 → v4; current palette is GitHub-dark, target is futuristic-cyan-void |
| 10 | Perf at scale | ❌ gap | current viz ships whole graph (`maxNodes` 2000 cap). Must move to LOD: server communities + deck.gl meta-nodes + on-demand neighborhoods |

## Proposed approach (the "better path" — researched, awaiting confirmation)

1. **Backend (Hono BFF).** Load `@engram/node` in-process —
   `createNativeProviderTransport({ configJson })` for recall/write/graph/beliefs,
   `createNativeKnowledgeTransport({ dbPath })` for list-all + analytics,
   `createNativeBeliefTransport({ dbPath })` for beliefs/contradictions — all
   pointed at `~/.engram/agentzero`. Expose zbot-shaped REST
   (`/api/graph/*`, `/api/memory/*`, `/api/hierarchy/stats`,
   `/api/belief-network/*`, …). Add server-side aggregation
   + keyset windowed queries (direct SQLite for `COUNT`/`GROUP BY`, since the
   binding lacks them). Never speak MCP to the browser.
2. **Frontend (greenfield).** Port zbot's `theme/components/effects` CSS +
   Tailwind v4 + fonts + 3-route shell + Memory/Observatory/Graph components.
   Replace the Graph renderer with **deck.gl** (Scatterplot + Arc layers) doing
   LOD: render Leiden/Louvain community meta-nodes at overview; drill to a
   neighborhood (`graph_neighbors` / `graph_subgraph`, capped) on click.
3. **Aggregation layer.** Pre-compute communities (`engram-graph-analytics` /
   `hierarchy_build`) + centrality/bridges; cache; ship small summaries. Run
   `hierarchy_build` if Observatory needs real data.
4. **View-types.** Define TS view interfaces for graph/belief/hierarchy/
   ontology/taxonomy records (cross-check vs `engram-domain` Rust); optionally
   propose adding them to `@engram/contracts`.

## Confirmed cut (4 shippability-first slices; decomposition confirmed 2026-08-03)

Cut by shippability (end-to-end verticals), not by layer. Perf — community
aggregation, keyset pagination, LOD caps — is baked into S1/S2 where scale bites,
not a separate horizontal slice.

| Spec slug | Ships (end-to-end) | Depends on |
|---|---|---|
| **S1** `viz-foundation` | Greenfield shell + zbot styling port (React 19 + Tailwind v4) + Hono BFF reading engram in-process via `@engram/node` (never engram-mcp) + view-types + community-overview Graph view (deck.gl LOD) + keyset/aggregation | — |
| **S2** `viz-graph-explorer` | Full Graph tab: LOD drill-down (community → bounded neighborhood), entity-detail panel, legend/controls, deck.gl perf at 170 k | S1 |
| **S3** `viz-memory` | Memory tab: facts/beliefs/contradictions/procedures over engram memory+belief surfaces; search; empty-states | S1 |
| **S4** `viz-observatory` | Observatory tab: graph/belief/hierarchy health + stats; learning-health bar; slideovers | S1 |

## Spec map

<!-- Shape A: one row per derived spec. Status is auto-derived by
scripts/lint-brief-coverage.py from each spec's own Status field — do not
hand-edit the Status column. Rows are added as slices are scaffolded. -->

| Spec | Status |
| --- | --- |
| [`viz-foundation`](../../specs/viz-foundation/spec.md) | Draft |
| [`viz-graph-explorer`](../../specs/viz-graph-explorer/spec.md) | Draft |
| [`viz-memory`](../../specs/viz-memory/spec.md) | Draft |
| [`viz-observatory`](../../specs/viz-observatory/spec.md) | Draft |

## References

- Research: `docs/research/large-graph-viz-transport-survey.md`
- Reference UI: `~/projects/agentzero-autonomy-ledger/apps/ui` (Memory/Observatory/Graph)
- Target: `mem-alpha/engram-viz/` (current — to be rebuilt)
- Data: `~/.engram/agentzero/engram_data.db` (2.0 GiB)
- TS artifacts: `mem-alpha/packages/` (node / client / contracts / adapters / runtime)
