# Spec: viz-graph-explorer

- **Status:** Shipped
- **Owner:** engram-viz
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003 (implementation stack), ADR-0022 (surface parity), [`docs/architecture/reference.md`](../../architecture/reference.md)
- **Brief:** [`docs/product/briefs/engram-viz-overhaul.md`](../../product/briefs/engram-viz-overhaul.md)
- **Contract:** [`contracts/openapi/engram-viz-bff.yaml`](../../../contracts/openapi/engram-viz-bff.yaml) *(extends — adds drill-down endpoints)*
- **Shape:** mixed (ui-led)

> **Spec contract:** the full Graph-tab explorer on top of `viz-foundation`. The
> implementing PR must match this spec, or update it. Library names live in
> `plan.md`; this body stays behavioral.

## Objective

The Graph tab's overview becomes **legible**: instead of `viz-foundation`'s
all-2000-node spiral (a dense, unreadable blob), the overview shows the **top-N
significant communities** (default ~150, `?limit=`-controllable, hard-capped at
the foundation's 2000) laid out on **deterministic concentric rings** ordered by
a breadth-first traversal of the meta-edges (connectivity-core inner, periphery
outer) — always legible regardless of how densely connected the meta-graph is.
(Force-directed was tried first and abandoned: a codebase's communities
interlink into one big component, so springs collapse the hubs into a central
blob with no legible structure.) From that overview, a user drills into the
graph: clicking a community meta-node fetches a **bounded sample of that
community's member entities** (the connected ones, via a cached label→entityId
index built from the relationship stream — `call_communities` keys by name but
entities are stored by id, so membership is derived from the relationship ids)
and renders them as a deck.gl drill layer clustered around the community's
coordinate; selecting a member opens an entity-detail panel (kind, community,
outgoing degree, provenance). The foundation's `/graph/node/:id/neighbors`
endpoint remains available for per-entity one-hop expansion (wiring it into the
drill UI is deferred). The explorer sustains ≥ 30 FPS on reference hardware
while drilling and never ships an unbounded payload. This completes the Graph
tab end-to-end.

## Boundaries

### Always do

- Render the overview as a **legible** concentric-ring meta-graph (top-N
  communities, default ~150); the layout is deterministic (BFS-ordered rings, no
  `Math.random`/`Date`) and cached per store-version + limit.
- Fetch the drill as a **bounded, keyset-paginated** sample of a community's
  member entities (via the cached label→entityId index over the read-only
  `node:sqlite` secondary path); reuse `viz-foundation`'s in-process
  `@engram/node` path. The foundation's `/graph/node/:id/neighbors` (keyset over
  `knowledge_relationships`) is available for per-entity expansion.
- Render the drill layer in WebGL level-of-detail; cap visible nodes/edges per
  expand (page `limit` ≤ 500; per-expand K-cap enforced server-side across the
  drill session).
- Derive entity-detail from the `viz-foundation` view-types (no new domain truth).

### Ask first

- Any new top-level dependency.
- Drills deeper than one hop in a single request (not supported by the binding —
  multi-hop is iterative client-driven expands), or batch/multi-select expansions.
- Any write path (this slice is read-only).

### Never do

- Ship an **unbounded neighborhood** payload — every drill endpoint caps + keysets.
  *(Structural.)*
- Render the raw 170 k-node graph on drill (drill is bounded LOD, never full).
  *(Structural.)*
- Bypass the BFF or route browser data through engram-mcp. *(Structural.)*

## Testing Strategy

- **Drill state + cap/cursor logic — TDD** (expand state machine, K-cap
  enforcement, cursor advance over the keyset window).
- **Neighborhood endpoints — goal-based + integration** against a fixture store:
  shapes + keyset + caps conform to the extended OpenAPI.
- **Drill flow + entity-detail — visual / manual QA via a Playwright E2E**: click
  community → bounded neighborhood paints → select node → detail panel opens.
- **Performance — manual on reference hardware**: ≥ 30 FPS during drill (headless
  Chrome's software WebGL cannot represent the bar; it checks render-without-crash).

## Acceptance Criteria

- [x] The overview renders the **top-N** communities (default ~150, `?limit=`,
  hard-cap 2000) positioned on **deterministic concentric rings** (BFS-ordered:
  connectivity-core inner, periphery outer; no RNG/`Date`, reproducible across
  runs) — not the foundation's all-2000 spiral; the response also reports
  `totalCommunities` so the legend can say "N of M".
- [x] Clicking a community meta-node fetches a bounded sample of its **member
  entities** (page `limit` ≤ 500; per-community index cap server-side; keyset) via
  a BFF endpoint and renders them as a deck.gl drill layer; the response carries
  no full dump.
- [x] Selecting a member opens an entity-detail panel showing kind, community,
  outgoing degree, and provenance.
- [x] The drill endpoints (`/graph/community/:id/members`, `/graph/entity/:id`)
  are keyset-paginated + hard-capped, and added to
  `contracts/openapi/engram-viz-bff.yaml` (with `503`/`Error`/`422`/`404`).
- [x] The explorer sustains ≥ 30 FPS on reference hardware during drill (the drill
  layer reuses the overview's deck.gl primitives; ≤ a few hundred nodes), and
  renders the bounded drill set without crash in the headless E2E.
- [x] All data flows through the `viz-foundation` in-process BFF (no engram-mcp,
  no browser store access).

## Assumptions

- Inherits `viz-foundation`'s verified stack/contract/assumptions (Node ≥ 22,
  `agentzero` store, in-process `@engram/node`, React 19 + Tailwind v4 + deck.gl).
- Technical: the binding exposes `neighbors` (`neighborsJson`) on
  `createNativeKnowledgeTransport` — a one-hop snapshot with a `limit`, **not**
  cursor-aware and with **no `subgraph`/depth API** (`packages/node/src/transport.ts`;
  `bindings/node/src/provider.rs`). The BFF serves the drill as a keyset window
  over `knowledge_relationships` via the read-only `node:sqlite` secondary path
  (consistent with `viz-foundation`'s Boundary).
- Product: local single-user, multi-user-ready seam inherited from S1.
