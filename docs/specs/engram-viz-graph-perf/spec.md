# Spec: engram-viz graph snappiness (2 repos)

- **Status:** In Progress
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Shape:** performance

> **Spec contract:** this document defines what "done" means.

## Objective

With two repos indexed (~18k entities), `/api/graph` ships a 4.5 MB payload
and the UI blocks while it is parsed; the Louvain `communities()` recompute
runs on every request. Make the workspace snappy via two strategies, in
sequence:

1. **Data diet (Strategy 1)** — send less, compute once: degree cap,
   gzip, cache communities, slim node payload.
2. **Overview-first supergraph (Strategy 2)** — aggregate Louvain
   communities into super-nodes so 18k entities read as ~30–60 bubbles,
   with drill-down.

Mode: light (no risk trigger fired — additive, reversible perf work on a
local viz tool; no security/destructive/structural-boundary change).

## Acceptance Criteria

### Strategy 1 (data diet)
- [x] `/api/graph` (all repos) uncompressed payload ≤ 1.5 MB (was 4.5 MB);
      gzipped wire bytes ≤ 400 KB. *(measured: 1.1 MB uncompressed, 358 KB
      gzip with maxNodes=2000)*
- [x] `communities()` is cached in the engine and invalidated on scan
      (`invalidateCache`), so repeat `/api/graph` calls skip the Louvain
      recompute.
- [x] Graph node payload is slimmed to `{id, name, kind, file, community,
      degree}` — `line`, `endLine`, `complexity`, `conceptRefs` no longer
      sent (unused in the render path; node detail still served by
      `/api/node/:id`).
- [x] A server-side `maxNodes` cap (default 2000, top-N by degree) prunes
      leaf noise; links whose endpoints are pruned are dropped; the UI shows
      a "Showing N of M (capped)" notice.
- [x] No regression: filtered (`?source=`) graph still works; node-detail
      panel still shows file + callers/callees (`complexity`/`line` were
      already null pre-change — native cyclomaticComplexity returns null for
      these entities); community/kind coloring unchanged.

### Strategy 2 (supergraph)
- [ ] `GET /api/supergraph` returns community super-nodes (memberCount,
      dominantKind, dominantRepo) + aggregated super-links (weight).
- [ ] Frontend has an Overview/Detailed toggle; Overview renders
      super-nodes sized by memberCount and colored by repo.
- [ ] Clicking a super-node drills into that community's member nodes in
      Detailed view.
- [ ] No regression in Detailed view.

## Testing Strategy
- **Strategy 1 — goal-based.** `npm run typecheck` (backend + frontend);
  curl `/api/graph` before/after for size + timing (`--compressed` for wire
  bytes); confirm repeat-call time drops after communities cache warms.
- **Strategy 2 — visual QA.** Toggle Overview/Detailed in the running app,
  confirm super-nodes render and drill-down works.

## Assumptions
- `hono/compress` (v4.12.27) is available — confirmed.
- Graph-node `line`/`endLine`/`complexity`/`conceptRefs` are unused in the
  render path (only on `NodeDetail`) — confirmed by grep.
- `communities()` is global over `relationships()` (cached), so its cache
  is valid until `invalidateCache()` — confirmed by read.
