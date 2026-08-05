# Spec: viz-foundation

- **Status:** Shipped
- **Owner:** engram-viz
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003 (implementation stack), ADR-0008 (durable ontology/taxonomy repositories), ADR-0022 (surface parity / engine neutrality), [`docs/architecture/reference.md`](../../architecture/reference.md) (golden-path stack)
- **Brief:** [`docs/product/briefs/engram-viz-overhaul.md`](../../product/briefs/engram-viz-overhaul.md)
- **Contract:** [`contracts/openapi/engram-viz-bff.yaml`](../../../contracts/openapi/engram-viz-bff.yaml)
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means for the foundation
> slice of the engram-viz overhaul. The implementing PR must match it, or update
> it. Verification must be derivable from it. Library names (Hono, deck.gl,
> Tailwind v4, React 19) live in `plan.md` Design decisions; this body stays
> behavioral.

## Objective

engram-viz is a local single-user browser app (with a multi-user-ready seam) that
opens engram's `agentzero` store and presents, in zbot's exact visual style, a
three-tab shell — Memory / Observatory / Graph — whose first vertical is a
**Graph overview** that renders engram's full knowledge graph via WebGL
level-of-detail over server-pre-aggregated community meta-nodes, sustaining
≥ 30 FPS on reference hardware (defined below). This slice delivers the
foundation the later tabs build on: a greenfield app shell with ported zbot
styling; a Backend-for-Frontend that reads engram in-process via the native
binding (never engram-mcp for the browser); TypeScript view-types for the
graph/belief/hierarchy/ontology/taxonomy records the published TS contracts lack;
and a community-overview Graph view fed by server-pre-aggregated, keyset-paginated
endpoints. Success: opening the app shows a styled, working community overview of
the live graph at ≥ 30 FPS, with no raw-node dump crossing the wire.

**Reference hardware** (for the FPS acceptance criterion): a 2022-era laptop
(x86-64 or Apple Silicon, integrated GPU), Chrome ≥ 120, 1080p viewport. A
dedicated benchmark rig is out of scope for this slice. The FPS bar is measured
**manually** on this hardware — headless Chrome uses software WebGL and cannot
represent it (see Testing Strategy).

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.

### Always do

- Read structured engram data **in-process via `@engram/node`** transports
  (provider + knowledge + belief). Use **read-only `node:sqlite`** (built into
  Node ≥ 22 — not a new dependency) **solely as a secondary read path for
  aggregation counts and keyset windowing the binding does not expose** — never
  for writes, never to re-implement domain semantics. A future backlog item may
  nativize pagination/aggregation into `@engram/node` so this secondary path can
  retire (not yet filed).
- Render the graph overview as **server-pre-aggregated community meta-nodes +
  bounded inter-community meta-edges** via WebGL; every browser-facing list
  endpoint is keyset-paginated and/or aggregated, and the overview payload
  itself is bounded (nodes ≤ 2000, edges ≤ 4000).
- Port zbot's styling from source (`theme.css` / `components.css` /
  `effects.css` + Tailwind v4 `@theme inline` + Fraunces/IBM Plex/JetBrains Mono).
- Resolve scope/tenant/config at the **BFF boundary** (one module, env/config
  driven), keeping a clean seam so a future auth + per-user-scope layer slots in
  without endpoint rewrites. Route handlers must not hardcode `tenant`/`workspace`
  literals.
- Fail reads closed (degrade to a safe baseline); translate typed errors to HTTP
  at the BFF (`reference.md`).

### Ask first

- Adding any new top-level dependency beyond the three declared for this slice —
  **deck.gl**, **`@tailwindcss/vite`**, **`react-router-dom`** (the route shell
  zbot uses).
- Any **write** path to the engram store from the viz (scan, `write_memory`,
  `belief_put`) — this slice is read-only.
- Changing engram's Rust core or `@engram/contracts` — this slice defines local
  view-types; proposing additions to `@engram/contracts` is a separate decision.

### Never do

- Route browser data through **engram-mcp** (stdio + Debug-text + LLM-agent
  semantics). *(Structural: no MCP client in the BFF data path.)*
- Introduce a second backend or a **god-module BFF** — split by domain
  (graph / memory / observatory / aggregation). *(Structural: no monolithic
  service module.)*
- Ship an **unbounded payload** to the browser — every endpoint
  caps/windowed/aggregates, including the overview's node and edge arrays.
  *(Structural.)*
- Redefine engram domain truth in TS — view-types are **projections** validated
  against `engram-domain` Rust, not a parallel model.

## Testing Strategy

- **View-type projections — TDD.** Each view-type projects from a named
  `engram-domain` Rust struct (`Entity` → `GraphEntityView`,
  `KnowledgeRelationship` → `GraphRelationshipView`, `BeliefStatement` →
  `BeliefView`, `HierarchyNode` → `HierarchyNodeView`, `Ontology` →
  `OntologyView`, `Concept` → `TaxonomyConceptView`). Parity is verified by a
  fixture-based snapshot test: a real `record_json` row per struct → the typed
  view, asserted field-by-field against the Rust struct's published fields.
- **Aggregation + keyset logic — TDD** (group-by-label, top-K, rank/truncate for
  both nodes and edges, cursor encode/decode): compressible invariants.
- **BFF endpoints — goal-based + integration** against a fixture store: assert
  response shapes, keyset cursors, and row caps conform to the OpenAPI contract;
  fetch-only is sufficient for these network-shape tests.
- **In-process binding wiring — integration.** Open the provider via `configJson`
  against the `agentzero` store; assert capabilities are non-empty and the
  community projection yields bounded meta-nodes + bounded meta-edges.
- **App shell + styling port + community-overview view — visual / manual QA via
  a headless-browser E2E test** (Playwright): shell renders zbot-styled,
  meta-nodes paint from real data, the network carries no raw-node dump, and the
  bounded set renders without crash or GPU overflow. (Headless Chrome uses
  software WebGL, so this run does **not** gate FPS — it gates correctness +
  render stability.)
- **Performance (LOD) — manual on reference hardware.** The ≥ 30 FPS bar is
  measured manually on the reference machine; the headless E2E cannot represent
  it.

## Acceptance Criteria

- [ ] Opening engram-viz shows the ported zbot app shell (topbar + 3-tab nav
  Memory / Observatory / Graph) visually consistent with zbot's theme tokens,
  fonts, and component classes. *(manual QA / E2E)*
- [ ] The Graph tab renders an overview of the live `agentzero` graph as
  community meta-nodes + inter-community meta-edges via WebGL, never raw nodes;
  the overview network response contains **only** meta-nodes/meta-edges — no
  entity or relationship rows.
- [ ] Every BFF list endpoint accepts a keyset cursor and returns a bounded page
  + a next-cursor; aggregation endpoints return pre-computed community/summary
  shapes. The community overview is bounded (nodes `maxItems` 2000, edges
  `maxItems` 4000 in the contract).
- [ ] The BFF reads engram in-process via `@engram/node` — the provider transport
  (capabilities) + the knowledge engine (community analytics via the raw
  binding); the knowledge/belief list transports are wired + exercised
  (knowledge via the live smoke; belief for S3). Paginated reads + counts use
  the read-only `node:sqlite` secondary path (the binding's list methods are
  unpaged firehoses). No engram-mcp subprocess is spawned and no Debug-text
  parsing occurs (asserted: no `child_process` import under `src/`).
- [ ] TypeScript view-types exist for graph (entity / relationship) + derived
  community meta-types (`CommunityMetaNode` / `CommunityMetaEdge`), each with a
  fixture-based projection test. (Belief / hierarchy / ontology / taxonomy
  view-types are deferred: `viz-non-graph-view-types` — they render in S3 / S4.)
- [ ] A multi-user seam is present and verified: scope/tenant/config resolved in
  one BFF module; a test asserts no route handler hardcodes `tenant`/`workspace`
  literals, so a future auth/per-user-scope layer slots in without rewrites.
- [ ] The BFF REST contract is authored at `contracts/openapi/engram-viz-bff.yaml`
  and linked bidirectionally (spec `Contract:` header + contract `x-spec`); every
  read endpoint declares a degraded `503` (and the `Error` shape).
- [ ] Overview renders at ≥ 30 FPS on reference hardware (measured manually) with
  the bounded meta-node/edge set; typecheck + build + the repo gate set pass.

## Assumptions

- Technical: Node ≥ 22 is required (built-in `node:sqlite` is the read-only
  secondary path) (source: `engram-viz/backend/package.json` `engines`).
- Technical: the `agentzero` store is at `~/.engram/agentzero/engram_data.db`
  (2.0 GiB SQLite WAL + `lexical/` + sqlite-vec 53 k×384-dim), scope
  `{tenant:"default", workspace:"agentzero"}` (source: read-only sqlite probe +
  running `engram-mcp --project agentzero`).
- Technical: the in-process `@engram/node` surface (provider + knowledge + belief
  transports; `call_communities` returns a flat `HashMap<symbol, label>`) is the
  structured read path (source: `packages/node/src/{binding,transport}.ts`;
  `codegraph/queries/src/queries.rs`; `bindings/node/src/codegraph.rs`).
- Technical: engram-mcp is agent-only — stdio + Debug-text + LLM-agent semantics
  (source: `mcp/engram-mcp/src/server.rs`; modelcontextprotocol.io).
- Process: Status = Draft/Implementing/Shipped/Deferred; full work-loop applies
  (source: `docs/CONVENTIONS.md` §4, §6).
- Product: local single-user now, with a multi-user-ready seam
  (source: user confirmation 2026-08-03).
- Product/scope: rebuild in-place at `engram-viz/` (source: user confirmation
  2026-08-03).

*(Stack selection — Hono, React 19, Tailwind v4, deck.gl — lives in `plan.md`
Design decisions, per the preamble.)*
