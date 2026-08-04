# Plan: viz-graph-explorer

- **Spec:** [`spec.md`](spec.md)
- **Status:** Draft

> **Plan contract:** implementation strategy for the Graph-tab drill-down. Builds
> on `viz-foundation` (T1–T9 there are prerequisites).

## Approach

Extend the `viz-foundation` BFF with bounded **one-hop** neighborhood endpoints,
then add a drill layer + entity-detail panel to the deck.gl Graph view. The
binding's `neighbors` is a one-hop snapshot with a `limit` (not cursor-aware; no
subgraph API), so the BFF serves the drill as a keyset window over
`knowledge_relationships` via the read-only `node:sqlite` secondary path, enforces
a hard node/edge K-cap (page `limit` ≤ 500; per-expand K-cap server-side), and the
client caps visible elements; multi-hop is iterative one-hop expands. Order:
(1) backend drill endpoints + contract delta; (2) drill state in the zustand store;
(3) deck.gl drill layer + entity-detail panel; (4) Playwright E2E + manual FPS.

## Constraints

- ADR-0003, ADR-0022, `reference.md` (in-process transport; fail-closed reads;
  typed→HTTP errors).
- `docs/CONVENTIONS.md` §4 (contract header), §6 (full work-loop — public-interface
  change).

## Construction tests

- **Unit (TDD):** drill expand state machine; per-expand K-cap on nodes/edges;
  cursor advance over the keyset window returns disjoint pages.
- **Integration:** `/api/graph/community/{id}/members` and
  `/api/graph/node/{id}/neighbors` conform to the extended OpenAPI; keyset + caps;
  503 + `Error` + `422 BadCursor`.
- **E2E (Playwright):** click community → bounded neighborhood paints → select
  node → detail panel; no full dump in the network log; renders without crash/GPU
  overflow.
- **Manual FPS:** ≥ 30 FPS on reference hardware during drill.

## Design (LLD)

### Design decisions

- Drill via **bounded one-hop neighborhood endpoints** (binding `neighbors`,
  served as a keyset window over `knowledge_relationships`), K-capped server-side —
  never client-side full-neighborhood; multi-hop is iterative. *(drill ACs;
  extends `contracts/openapi/engram-viz-bff.yaml`.)*
- `limit` bounds page size (≤ 500, the shared contract cap); the per-expand K-cap
  is enforced server-side across the drill session (separate from `limit`).
- Reuse `viz-foundation` deck.gl + view-types + legend/zoom (S1 T8); add a drill
  `ScatterplotLayer` + `ArcLayer` overlaid on the overview + a focus-on-drill
  control, plus a right-side entity-detail panel.

### Interfaces & contracts

Extends `contracts/openapi/engram-viz-bff.yaml`:

- `GET /api/graph/community/{id}/members?cursor=&limit=` → keyset-paged entities in
  a community (drill-in preview).
- `GET /api/graph/node/{id}/neighbors?cursor=&limit=` → keyset-paged, K-capped
  one-hop neighborhood over `knowledge_relationships` (no `depth` param; multi-hop
  is iterative).

Both keyset + capped, both declare `503` + `Error` + `422 BadCursor`.

### Component / module decomposition

- **backend:** `src/routes/graph.ts` (drill handlers); `src/db/reader.ts` (keyset
  window over `knowledge_relationships` by endpoint node); `src/engram/knowledge.ts`
  (neighbor entity hydration).
- **frontend:** `src/features/graph/DrillLayer.tsx` (deck.gl drill layer),
  `src/features/graph/EntityDetail.tsx` (panel), `src/store/graph.ts` (drill state).

### Behavior & rules

Read-only. One-hop per expand; per-expand node/edge caps enforced server-side;
multi-hop is client-driven iterative expands. Entity detail reuses `GraphEntityView`
+ provenance.

### Failure, edge cases & resilience

Node with >cap neighbors → capped + next-cursor ("expand more"). Degraded store →
503 + `Error`; bad cursor → 422.

### Quality attributes (NFRs)

- Performance: ≥ 30 FPS during drill (manual, reference hardware); bounded payloads.
- Operability: degraded-mode surfaced via 503.

## Tasks

### T1: Neighborhood drill endpoints + contract delta

**Depends on:** viz-foundation T5

**Tests:**
- Integration: `/api/graph/community/{id}/members` + `/api/graph/node/{id}/neighbors`
  conform to the extended OpenAPI; keyset window over `knowledge_relationships`;
  page `limit` ≤ 500 + per-expand K-cap server-side; 503+Error+422. *(drill-endpoint
  AC)*

**Approach:**
- `src/routes/graph.ts` drill handlers; serve `neighbors` as a keyset window over
  `knowledge_relationships` (read-only `node:sqlite`); hydrate neighbor entities
  via the binding; hard node/edge caps; extend
  `contracts/openapi/engram-viz-bff.yaml` (authored directly, no `api-contract`
  skill) with `503`/`Error`/`422`.

**Done when:** integration tests green; contract extended + linked.

### T2: Drill state in the store

**Depends on:** T1

**Tests:**
- Unit (TDD): expand/collapse state machine; per-expand cap; cursor advance.
  *(drill AC)*

**Approach:**
- `src/store/graph.ts`: drill state (expanded communities, loaded neighborhoods,
  selected node); api client methods.

**Done when:** unit tests green.

### T3: deck.gl drill layer + entity-detail panel

**Depends on:** T1, T2, viz-foundation T7

**Tests:**
- Visual/manual QA: drill layer paints bounded neighborhood; entity-detail panel
  opens on select; focus-on-drill control works.

**Approach:**
- `src/features/graph/DrillLayer.tsx` (deck.gl overlay); `EntityDetail.tsx` (panel
  from view-types + provenance); reuse S1 legend/zoom; add focus-on-drill.

**Done when:** drill + detail render against real data.

### T4: E2E (Playwright) + manual FPS gate

**Depends on:** T3

**Tests:**
- E2E (Playwright): community click → bounded neighborhood paints → node select →
  detail panel; no full dump; renders without crash/GPU overflow.
- Manual FPS: ≥ 30 FPS on reference hardware during drill. *(all ACs)*

**Approach:** Playwright suite; separate manual FPS check; confirm repo gates.

**Done when:** E2E + manual FPS green.

## Rollout

- **Delivery:** extends the shipped Graph tab; reversible via git. No data
  migration (read-only).
- **Deployment sequencing:** after `viz-foundation`; backend T1 before frontend
  T2–T3.

## Risks

- The binding's `neighbors` is a non-cursor-aware snapshot → the BFF wraps it as a
  keyset window over `knowledge_relationships` (T1) so drill is paginated.
- Drill layer overwhelming the overview canvas → per-expand K-cap + LOD fade for
  non-focused communities.

## Changelog

- 2026-08-03: initial plan.
- 2026-08-03: review pass — corrected the binding surface: `graph_subgraph`/depth-2
  do not exist; drill is bounded one-hop `neighbors` served as a keyset window over
  `knowledge_relationships`; reconciled page `limit` (≤500) vs per-expand K-cap;
  noted S1 legend/zoom covers the brief's legend/controls item.
