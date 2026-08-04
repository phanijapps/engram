# Plan: viz-foundation

- **Spec:** [`spec.md`](spec.md)
- **Status:** Draft

> **Plan contract:** this is the implementation strategy. It may change as we
> learn; substantial changes are recorded in the changelog at the bottom.

## Approach

Greenfield rewrite of `engram-viz/{frontend,backend}` in-place. Two tracks that
meet in a headless-browser E2E gate plus a manual FPS check: a **backend** track
wires `@engram/node` in-process, defines view-types, centralizes scope behind a
multi-user seam, adds keyset/aggregation helpers via read-only `node:sqlite` (the
binding exposes neither pagination nor bulk counts), projects the flat
`call_communities` label-map into **bounded** community meta-nodes + **bounded**
inter-community meta-edges, and exposes a keyset/capped REST surface; a
**frontend** track upgrades to React 19 + Router 7 + Tailwind v4, ports zbot's
styling, builds the 3-tab shell, and renders the community-overview Graph view in
deck.gl. Riskiest parts: (a) the firehose-pagination gap — solved by read-only
`node:sqlite` keyset/aggregation *alongside* the binding, never replacing it; (b)
the community projection pipeline — `call_communities` returns a flat
`HashMap<symbol, label>`, so meta-nodes/meta-edges are derived (group-by-label →
rank → truncate nodes to 2000 → inter-cluster edge map → rank → truncate edges to
4000 → cache); (c) bounding the edge array (worst case ~2 M at 2000 communities)
to the 4000-edge budget.

## Constraints

- ADR-0003 (implementation stack), ADR-0008 (durable ontology/taxonomy repos),
  ADR-0022 (surface parity — MCP stays the agent surface; the BFF is a second
  transport over the same provider), `reference.md` (Hono + Vite React + N-API
  transport; fail-closed reads; typed→HTTP errors).
- `docs/CONVENTIONS.md` §4 (contract header + `contracts/<type>/`), §6 (full
  work-loop — new deps, multi-feature, public-interface).

## Construction tests

- **Integration:** open the provider via `configJson` against the `agentzero`
  store → `capabilities()` non-empty; `/api/graph/communities` returns bounded
  meta-nodes (≤ 2000) + bounded meta-edges (≤ 4000).
- **Integration (keyset):** a list endpoint returns a page + next-cursor;
  following the cursor yields disjoint rows; the row cap is enforced; the keyset
  column (`rowid`) is monotonic + indexed.
- **Unit (TDD):** view-type projections from raw `record_json` → typed view, with
  fixture-based parity vs the named `engram-domain` Rust struct.
- **Unit (TDD):** the community projection pipeline (label-map → bounded meta-
  nodes/meta-edges) on a fixture graph.
- **Unit (TDD):** scope-seam — a test asserting no route handler references
  `tenant`/`workspace` literals (scope comes from the central module).
- **E2E (Playwright, headless):** open the app → styled shell → Graph overview
  paints meta-nodes from real data; the network carries no unbounded
  entity/relationship dump; the bounded set renders without crash/GPU overflow.
  (Headless Chrome uses software WebGL — this run does NOT gate FPS.)
- **Manual verification (FPS gate):** ≥ 30 FPS on reference hardware with the
  bounded meta-node/edge set, measured manually (the headless run cannot represent
  this bar).

## Design (LLD)

### Design decisions

- **Stack** (the spec body stays behavioral; the library pick lives here): Hono
  4.6 BFF; React 19 + react-router-dom 7 + Vite 5 + zustand frontend; Tailwind v4
  (`@tailwindcss/vite` + `@theme inline`); **deck.gl** WebGL for the Graph
  overview; `@engram/node` in-process for structured reads. *(Traces to: shell +
  overview ACs · `contracts/openapi/engram-viz-bff.yaml`.)*
- **In-process `@engram/node`** (not engram-mcp) for the browser data path —
  structured JSON, lower latency, reaches more surfaces. *(wiring AC; ADR-0022.)*
- **Read-only `node:sqlite` secondary path** (built-in, Node ≥ 22) for `COUNT`/
  `GROUP BY`/keyset the binding lacks; a future backlog item may nativize these
  into `@engram/node` (not yet filed). *(keyset AC.)*
- **LOD:** render community meta-nodes/meta-edges, never raw nodes. *(overview
  AC.)*
- **Community projection pipeline (both arrays bounded):** `call_communities` →
  flat label-map → group-by-label → memberCount → rank communities by memberCount
  → **truncate nodes to 2000** → inter-cluster edge map over the surviving
  communities → rank edges by weight → **truncate edges to 4000** → precompute
  `x,y` layout (deterministic seed) → cache. *(overview AC.)*
- **Local view-types as projections** (fills the `@engram/contracts` gap).
- **Multi-user seam:** scope resolved in one module, injected into handlers.

### Data & schema

The `agentzero` store is read-only from the viz: `knowledge_entities` /
`_relationships` / `_chunks`, `memories`, `beliefs`, `hierarchy_nodes` /
`_relations`, `concept_schemes` / `concepts`, `ontologies` / `ontology_classes`.
No migrations. View-types project from `engram-domain` structs: `Entity` →
`GraphEntityView`, `KnowledgeRelationship` → `GraphRelationshipView`,
`BeliefStatement` → `BeliefView`, `HierarchyNode` → `HierarchyNodeView`,
`Ontology` → `OntologyView`, `Concept` → `TaxonomyConceptView`. The community
overview carries only `CommunityMetaNode { id, name, memberCount, x?, y? }` and
`CommunityMetaEdge { source, target, weight }` (no entity rows in S1; previews
land in S2 drill-down).

### Interfaces & contracts

BFF REST — `contracts/openapi/engram-viz-bff.yaml`:

- `GET /api/health` → `{ status: "ok", scope?, capabilities }` (`503` + `Error`
  when degraded).
- `GET /api/capabilities` → capability report.
- `GET /api/graph/communities` → bounded meta-nodes (`maxItems` 2000) + bounded
  inter-community meta-edges (`maxItems` 4000), aggregated server-side;
  `built:false` when there are too few relationships to cluster.
- `GET /api/graph/stats` → counts.
- `GET /api/entities?cursor=&limit=` → keyset-paged entities.
- `GET /api/graph/node/:id/neighbors?cursor=&limit=` → keyset-paged neighborhood.

All list endpoints are keyset + capped; every read endpoint declares a degraded
`503` (+ `Error`). (SSE streaming is deferred to a later slice; not scaffolded
here.)

### Component / module decomposition

- **backend** (split by domain — no god-module): `src/index.ts` (Hono app),
  `src/engram/{provider,knowledge,belief}.ts` (in-process transport wiring),
  `src/scope.ts` (the multi-user seam — single source of scope/config),
  `src/views/*.ts` (view-type projections), `src/routes/{health,graph}.ts`,
  `src/db/reader.ts` (read-only `node:sqlite` for counts/keyset),
  `src/aggregation/communities.ts` (cached community projection).
- **frontend:** `src/App.tsx` (shell + Router 7 nav), `src/styles/*` (ported zbot
  CSS + Tailwind v4), `src/store/` (zustand), `src/lib/api.ts` (BFF client +
  keyset), `src/features/graph/CommunityOverview.tsx` (deck.gl).

### State & control flow

Boot: fetch `/api/health` + `/api/capabilities` + `/api/graph/communities` →
render shell + Graph overview. Memory/Observatory routes exist and render styled
"coming in S3/S4" placeholders (so the shell is real, not stub-routed).

### Behavior & rules

Read-only in this slice. Reads fail closed. Keyset cursors are opaque tokens
over `rowid`. Community aggregation is cached, keyed by store version/mtime. The
overview empty-state triggers on **too few relationships to cluster** (or Louvain
divergence), not on hierarchy-table state — `call_communities` reads
`knowledge_relationships`, not `hierarchy_*`.

### Failure, edge cases & resilience

Store unreachable → `/api/health` = 503 degraded; read endpoints return 503 +
`Error`; UI shows an error state. Too few relationships to cluster → overview
shows an empty-state. `list*` beyond cap → capped + next-cursor.

### Quality attributes (NFRs)

- Performance: overview ≤ 2000 meta-nodes + ≤ 4000 meta-edges at ≥ 30 FPS (manual,
  reference hardware); no endpoint exceeds its cap.
- Operability: health + capabilities; degraded-mode surfaced.
- Extensibility: multi-user seam (scope centralized).

## Tasks

### T1: Backend skeleton + in-process provider wiring

**Depends on:** none

**Tests:**
- Integration: `createNativeProviderTransport({ configJson })` opens against the
  `agentzero` store; `capabilities()` is non-empty; `listEntities(scope)`
  returns rows. *(wiring AC)*

**Approach:**
- `src/engram/{provider,knowledge,belief}.ts`; `configJson` builder
  (`storage_path`, scope `default/agentzero`, embedding `none`/384, `SingleFile`
  `engram_data.db`).
- Read-only `node:sqlite` reader (`mode=ro`).
- `GET /api/health` (503 when the store is unreachable), `GET /api/capabilities`.

**Done when:** the wiring integration test is green and `/api/health` returns
scope + capabilities (and 503 when degraded).

### T2: View-types + projections (TDD)

**Depends on:** T1

**Tests:**
- Unit: project one real `record_json` fixture per struct → its view-type
  (`Entity`→`GraphEntityView`, `KnowledgeRelationship`→`GraphRelationshipView`,
  `BeliefStatement`→`BeliefView`, `HierarchyNode`→`HierarchyNodeView`,
  `Ontology`→`OntologyView`, `Concept`→`TaxonomyConceptView`); assert each field
  against the Rust struct's published field set. *(view-types AC)*

**Approach:**
- `src/views/*.ts` + types; fixtures captured from the `agentzero` store;
  field sets read from `engram-domain`.

**Done when:** unit tests green; `tsc --noEmit` clean.

### T3: Keyset pagination + read-only aggregation helpers

**Depends on:** T1

**Tests:**
- Integration: keyset over `knowledge_entities` on `rowid` returns disjoint pages
  + next-cursor; cap enforced; `COUNT`/`GROUP BY` returns community sizes.
  *(keyset AC)*

**Approach:**
- `src/db/reader.ts`: keyset on `rowid` (monotonic, indexed); opaque cursor
  encode/decode; `COUNT`/`GROUP BY` helpers.

**Done when:** integration tests green.

### T4: Multi-user scope seam

**Depends on:** T1

**Tests:**
- Unit: `src/scope.ts` is the single resolver of `{tenant, workspace,
  environment}` from env/config; a test asserts no file under `src/routes/`
  contains the literals `"tenant"` or `"workspace"` as hardcoded values (scope is
  injected). *(multi-user-seam AC)*

**Approach:**
- `src/scope.ts` (env/config → `Scope`); inject into handlers; the lint/test
  guards the seam.

**Done when:** the seam test is green (no hardcoded scope literals in routes).

### T5: BFF REST endpoints + OpenAPI contract

**Depends on:** T2, T3

**Tests:**
- Goal-based + integration (fetch-only network-shape): `/api/graph/communities`,
  `/api/graph/stats`, `/api/entities`, `/api/graph/node/:id/neighbors` conform to
  the OpenAPI; keyset + caps hold; node/edge `maxItems` enforced; each read
  endpoint returns 503 + `Error` when degraded. *(contract + endpoints AC)*

**Approach:**
- `src/routes/graph.ts`; author `contracts/openapi/engram-viz-bff.yaml` directly
  (no `api-contract` skill installed — note "authored without rule-enforcement");
  `x-spec` back-link to this spec.

**Done when:** endpoint tests green; the contract links back to the spec.

### T6: Community projection pipeline (cached, both arrays bounded)

**Depends on:** T2, T3

**Tests:**
- Unit (TDD): on a fixture label-map, the pipeline yields `CommunityMetaNode[]`
  (≤ 2000) + `CommunityMetaEdge[]` (≤ 4000, top by weight); cache hit returns the
  same instance; empty-state when relationship count is below threshold.
  *(overview aggregation AC)*

**Approach:**
- `src/aggregation/communities.ts`: `call_communities` label-map → group-by-label
  → memberCount → rank communities by memberCount → **truncate to 2000 nodes** →
  inter-cluster edge map over the surviving communities (from
  `knowledge_relationships` via the read-only reader) → rank edges by weight →
  **truncate to 4000 edges** → precompute `x,y` layout (deterministic seed) →
  cache keyed by store version. Both arrays are provably bounded.

**Done when:** unit tests green; pipeline returns bounded meta-nodes + meta-edges.

### T7: Frontend shell + zbot styling port (React 19 + Tailwind v4)

**Depends on:** none (frontend track)

**Tests:**
- Visual/manual QA + goal-based (`tsc` + `vite build`). *(shell AC)*

**Approach:**
- Upgrade React 18 → 19, add react-router-dom 7; port `theme/components/effects
  .css` + fonts; migrate to `@tailwindcss/vite` + `@theme inline`; `App.tsx`
  3-tab nav with styled Memory/Observatory placeholders.

**Done when:** shell renders zbot-styled; build clean.

### T8: BFF client + Graph community-overview view (deck.gl)

**Depends on:** T5, T6, T7

**Tests:**
- E2E prerequisite (network): `/api/graph/communities` shape; no raw-node rows;
  node/edge arrays within caps.
- *(rendering + FPS verified in T9)*

**Approach:**
- `src/lib/api.ts` (fetch + keyset helper);
  `src/features/graph/CommunityOverview.tsx` (deck.gl `ScatterplotLayer`
  meta-nodes + `ArcLayer` inter-community meta-edges); legend + zoom.

**Done when:** the view consumes the endpoint and renders meta-nodes/meta-edges.

### T9: E2E (Playwright) + manual FPS gate + README

**Depends on:** T8

**Tests:**
- E2E (Playwright, headless): shell renders zbot-styled; meta-nodes paint from
  real data; no raw-node dump in the network log; renders the bounded set without
  crash/GPU overflow. *(Headless Chrome uses software WebGL — not an FPS gate.)*
- Manual FPS: ≥ 30 FPS on reference hardware (manual measurement). *(all ACs)*

**Approach:**
- Playwright suite (no-crash/no-overflow render check + network-shape); FPS is a
  separate manual check on reference hardware; add the `docs/specs/README.md`
  active entry (also added when the spec lands — see Changelog); confirm repo
  gates.

**Done when:** E2E + manual FPS gate green; README lists `viz-foundation`.

## Rollout

- **Delivery:** in-place rebuild; the old engram-viz behavior is replaced.
  Reversible via git (greenfield rewrite). No data migration (read-only).
- **Infrastructure:** none new — local Hono + Vite over the existing `agentzero`
  store.
- **Deployment sequencing:** backend T1–T6 land first (API-testable); frontend
  T7–T8; E2E + manual FPS T9. S2–S4 build on T1–T8.

## Risks

- The binding's `list*` firehose over 170 k entities → mitigated by read-only
  `node:sqlite` keyset + aggregation (T3) before any browser endpoint; a future
  backlog item may nativize these into `@engram/node` (not yet filed).
- Community projection edge blow-up (worst case ~2 M at 2000 communities) →
  mitigated by computing the edge map *after* node truncation + a 4000-edge cap
  (T6).
- React 18 → 19 + Tailwind v3 → v4 migration regressions → visual QA (T7).
- deck.gl bundle size / learning curve → acceptable (research-confirmed).
- **Event-loop stall on first `/api/graph/communities` (cache miss):** Louvain
  (≈2.6 s) + the 227 k-row edge stream run synchronously in the route, blocking
  every other request for that window. Cached thereafter (WAL-aware mtime key).
  Worker-thread offload / boot-time precompute is a follow-up (deferred:
  `viz-community-event-loop-stall`).

## Changelog

- 2026-08-03: initial plan.
- 2026-08-03: adversarial-review pass 1 — added scope-seam task (T4); rewrote
  community task as a projection pipeline (T6) since `call_communities` returns a
  flat label-map; named `node:sqlite` (built-in) as the read-only secondary path
  + a backlog note to nativize; struck SSE scaffolding; mandated Playwright for
  the rendering E2E; named view-type Rust sources + fixtures; upgraded to React 19
  + Router 7 to match zbot.
- 2026-08-03: adversarial-review pass 2 — bounded the inter-community **edges**
  array (≤ 4000, top by weight) in T6 + the OpenAPI (the pass-1 blocker); split
  FPS verification (manual on reference hardware = the gate; headless E2E =
  render-without-crash only, since headless Chrome uses software WebGL);
  reconciled SSE as deferred in the brief; trimmed spec Assumptions to
  preconditions; softened backlog references to "future / not yet filed".
- 2026-08-04: implementation review (single pass) — applied: WAL-aware community
  cache key (main + `-wal` mtime); name-or-id edge fallback (`entity_key`); the
  OpenAPI `Health.capabilities` is an object (not a string list); fixture
  `/api/health` HTTP test (200 + 503); `child_process`-absent Boundary
  assertion; deferred the four non-graph view-types (AC5 → backlog
  `viz-non-graph-view-types`); recorded the event-loop-stall risk; tightened
  AC4 (knowledge engine + node:sqlite, not the firehose list transports).
