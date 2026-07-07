# Spec: graph-explorer (whole-knowledge-graph explorer)

- **Status:** Shipped
- **Shape:** mixed (service + ui)
- **Constrained by:** demo-ui-shell (lives in the shell as a route); backend adds endpoints only (no contract/Rust-domain change); consumes the existing knowledge-graph data
- **Contract:** none (new demo-only endpoints; transport stays `unknown`-typed)

## Objective

A dedicated `/explorer` route visualizes the **entire** knowledge graph at a high level and lets the user drill in. By default it clusters nodes **by source/repo** (each ingested source = one cluster, sized by entity count) and draws **cross-repo edges** between clusters that share an entity name (e.g. `auth-service` in repo A and repo B) — so "how different repos connect" is visible at a glance. **Double-clicking a cluster** expands it into its member entities + their internal relationships; clicking a node opens a detail panel. A 2D force-directed layout (readable labels, zoom, drill-down) renders it.

The backend adds three scoped list methods + one overview route so the explorer can read across every graph: `list_graphs`, `list_entities`, `list_relationships` (all scope-filtered) and `POST /knowledge/overview`. Cross-repo linking + clustering are computed client-side from the entity/relationship lists.

This phase runs on **existing** ingested data (repos + code/concept entities + their relationships). Enriching extraction to emit value-stream / requirement / API-endpoint entities + cross-doc links (so `valuestream → requirement → code` chains appear) is explicitly deferred — recorded in `## Out of scope`.

## Decision

2D force-directed layout (via `react-force-graph-2d`) for label readability + drill-down, in a new `/explorer` route; the 3D `Graph3D` stays on `/ingest` and `/index`. Group by source/repo (the entity's `graphId` → the parent `KnowledgeGraph.name`). Cross-repo edges = entity-name matches across distinct graphs (computed client-side). Backend additions mirror the existing scope-filtered list pattern (`list_concepts` is the template); no new domain types, no contract change.

## Assumptions

- Technical: entities already carry `graphId` (deterministic + LLM extraction both set it), and `KnowledgeGraph.name` is the source/repo label (e.g. `scan:myrepo`). (verified — Slice 2 probes + `SqlKnowledgeStore`)
- Technical: no list-graphs/list-entities/list-relationships endpoint exists today; `list_concepts` is the scope-filtered template. (verified)
- Technical: `react-force-graph-2d` renders a canvas force graph with node/edge click + zoom handlers (no three.js dep in the -2d variant). (community-standard library)
- Product: 2D / group-by-source / explorer-now-enrich-later confirmed by user. (user confirmation)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Cluster by source/repo at the high level; cross-link repos by shared entity name (client-side).
- Scope-filter all backend list methods (`scope_allows`), mirroring `list_concepts`.
- Keep the explorer on existing data; do not invent new extraction in this slice.
- Reuse the shell (sidebar item + command-palette entry); no second styling system.

**Ask first**
- Server-side clustering / pagination; cross-repo semantic (LLM) linking.

**Never do**
- Change Rust domain types or contracts; add value-stream/requirement extraction here; duplicate the Graph3D component (the explorer is its own 2D viz).

## Testing Strategy

- **TDD (unit, Rust):** `engram-store-knowledge-sqlite` — `list_graphs`/`list_entities`/`list_relationships` return only scope-visible rows (cross-tenant hidden).
- **Goal-based (build):** backend typecheck/test; rebuild native binding; frontend typecheck/build.
- **Goal-based (plumbing):** `/knowledge/overview` returns the aggregated graph via curl (scope-filtered).
- **Manual QA:** ingest 2+ repos/sources that share an entity name → `/explorer` shows two clusters + a cross-repo edge; double-click a cluster → its entities + internal edges; click a node → detail.

## Acceptance Criteria

- [x] `SqlKnowledgeStore` exposes scope-filtered `list_graphs`/`list_entities`/`list_relationships` (+ Rust unit tests for scope isolation).
- [x] `NativeKnowledgeEngine` exposes them over N-API; `NativeKnowledgeTransport` wraps them; `POST /knowledge/overview` returns `{graphs, entities, relationships}` scope-filtered.
- [x] A `/explorer` route renders a 2D force-directed graph clustered by source/repo, with cross-repo edges on shared entity names.
- [x] Double-clicking a cluster expands it to its member entities + internal relationships; clicking a node opens a detail panel.
- [x] Sidebar + command palette include the explorer; backend untouched otherwise; typecheck/build/test green.
