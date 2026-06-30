# Plan: graph-explorer (whole-knowledge-graph explorer)

2D force-directed whole-graph explorer on existing data; cluster by source/repo,
cross-link repos by shared entity name, double-click drill-down. Backend list
methods + overview route; new `/explorer` route. Two commits: **A** backend,
**B** explorer UI.

## Tasks

### T1 — Backend list methods + `/knowledge/overview` [Commit A]
- **Tests (TDD):** `engram-store-knowledge-sqlite` — `list_graphs`/`list_entities`/`list_relationships` scope isolation (tenant-a visible, tenant-b hidden).
- **Depends on:** none
- **Approach:** `adapters/knowledge/sqlite/src/service.rs`: add `pub async fn list_graphs/list_entities/list_relationships(&self, scope)` on `SqlKnowledgeStore` (store-specific, like `list_beliefs` in the belief adapter) — `SELECT record_json … ORDER BY id`, deserialize, `scope_allows` filter. Binding `bindings/node/src/lib.rs`: `listGraphsJson`/`listEntitiesJson`/`listRelationshipsJson` (`{scope}` → encode). `packages/node/src/{binding,transport,index}.ts`: add to interfaces + impls + re-exports. `demo/backend/src/app.ts`: `POST /knowledge/overview` → `{ graphs, entities, relationships }` (three list calls, scope from body). Rebuild native.

### T2 — `/explorer` route (2D force-directed, cluster + drill-down) [Commit B]
- **Tests:** goal-based — frontend typecheck/build; manual QA.
- **Depends on:** T1
- **Approach:** `pnpm --filter demo-frontend add react-force-graph-2d`. `src/routes/explorer.tsx`: fetch `/knowledge/overview` once. **High-level view:** nodes = clusters (one per graph/source; `id=graph.id`, `name=graph.name`, `count=entities in graph`, `kind="cluster"`), sized by count, colored per-repo; edges = cross-cluster links computed by grouping entities by lowercased `name` and linking the distinct graphs that share a name. **Double-click a cluster** → swap to that cluster's member entities as nodes (their `kind`/`name`) + their internal relationships (by `graphId`) as edges; a Back control collapses to the high-level view. **Click a node** → a shadcn side sheet/panel with detail (name, kind, source/graph, degree). Wire into `router.tsx` + sidebar `NAV_ITEMS` + command palette (automatic via NAV_ITEMS).

### T3 — Validate + lighter adversarial pass
- **Tests:** backend typecheck/test; rebuild native; frontend typecheck/build; `/knowledge/overview` curl smoke; single-pass review focused on scope isolation, cross-name-link correctness, + drill-down state.
- **Depends on:** T2

## Out of scope (logged)
- Value-stream / requirement / API-endpoint extraction + cross-doc semantic linking (phase 2 — needs extractor + ontology work).
- Server-side clustering / pagination; community detection; saving explorer layouts.
