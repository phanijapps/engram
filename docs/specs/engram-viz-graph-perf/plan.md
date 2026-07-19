# Plan: engram-viz graph snappiness

- **Spec:** [`spec.md`](spec.md)
- **Status:** In Progress

## Strategy 1 — data diet

### T1: Cache `communities()` in the engine
**Mode:** goal-based · **Depends on:** none
Add a `communityCache: Map<string, number> | null` field; `communities()`
populates and returns it; `invalidateCache()` clears it (alongside
`relCache`/`entityCache`). Done when: typecheck clean; second
`/api/graph` call is markedly faster than the first (Louvain skipped).

### T2: Slim graph node payload
**Mode:** goal-based · **Depends on:** none
In `routes/graph.ts`, drop `line`, `endLine`, `complexity`, `conceptRefs`
from each node object. Keep `id, name, kind, file, community, degree`.
Done when: typecheck clean; a node object in the response has exactly
those keys.

### T3: Server-side `maxNodes` degree cap
**Mode:** goal-based · **Depends on:** T2
In `routes/graph.ts`, after computing nodes+degree, accept `?maxNodes=`
(default 2500): sort nodes by degree desc, take top N, drop links whose
endpoints aren't in the kept set. Done when: typecheck clean; payload for
all-repos ≤ ~1.3 MB before gzip.

### T4: gzip compress middleware
**Mode:** goal-based · **Depends on:** none
`app.use("*", compress())` from `hono/compress` in `index.ts`, placed
early (after logger). Done when: `curl --compressed -w size_download` for
`/api/graph` ≤ 400 KB.

### T5: Verify Strategy 1
**Mode:** manual QA · **Depends on:** T1–T4
Curl before/after table; load both repos in the UI, switch repos, confirm
snappy + node-detail panel intact.

## Strategy 2 — overview-first supergraph

### T6: Backend `/api/supergraph`
**Mode:** goal-based · **Depends on:** T1 (uses communities cache)
New route returning `{ communities: [{id, memberCount, dominantKind,
dominantRepo}], links: [{source, target, weight}] }`. Aggregate over
entities grouped by community (from `communities()`), with repo derived
from each entity's source. Done when: typecheck clean; curl returns
super-nodes/links.

### T7: Frontend Overview/Detailed mode + drill-down
**Mode:** manual QA · **Depends on:** T6
`viewMode: "overview" | "detailed"` + `focusedCommunity` in the store;
a toggle control; Overview renders super-nodes (sized by memberCount,
colored by dominantRepo) from `/api/supergraph`; clicking a super-node
sets a community focus and switches to Detailed showing only that
community's nodes. Done when: typecheck clean; toggling + drill-down
work in the running app.

## Changelog
- 2026-07-12: initial plan (two strategies, in sequence; light mode).
