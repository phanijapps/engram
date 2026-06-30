# Spec: enterprise-3d-graph (RFC 0004 Slice 0 / PHASE57)

- **Status:** Shipped
- **Shape:** ui
- **Constrained by:** RFC-0004 (D5), ADR-0007 (no change — frontend only)
- **Contract:** none (consumes the existing `/ingest/extract` response; no contract change)

## Objective

The demo frontend renders its knowledge graph in a **minimalist, enterprise-ready
3D view with navigable links**, replacing the current 2D Cytoscape panel. The view
uses `react-three-fiber` + `@react-three/drei` with a **deterministic Fibonacci-sphere
layout** (borrowing agentzero's technique), a restrained neutral/dark palette with a
single accent, and full navigability: click a node → a detail panel (entity fields +
source links); click an edge → relationship detail; hover to highlight; orbit/zoom.

The view consumes the **existing** `/ingest/extract` response shape
(`{entities:[{id,kind,name,sourceRefs,provenance}], relationships:[{subject.id,predicate,object.id,confidence}], chunkCount}`)
unchanged. No backend, contract, or Rust change.

## Assumptions

- Technical: frontend is React 18.3 + Vite 5.4 + TypeScript 5.7 (`demo/frontend/package.json`); the graph today is Cytoscape in `IngestPanel.tsx`. (verified)
- Technical: `KnowledgeEntity`/`KnowledgeRelationship` serialize camelCase; `EntityKind` is snake_case; there is **no native `mention_count`** — node weighting derives from relationship degree / `confidence`. (verified against `core/domain/src/knowledge.rs`)
- Technical: `@react-three/fiber` + `@react-three/drei` + `three` are compatible with React 18 + Vite 5 (standard, well-established stack). (verified — peer deps match React 18)
- Product: the aesthetic is *minimalist but stunning, enterprise-ready* — restraint over effects; the agentzero cinematic glassmorphism is explicitly **not** the target. (user confirmation 2026-06-30)
- Process: lighter adversarial review (single pass) per user standing preference. (user confirmation)

## Boundaries

**Always do**
- Render the graph with `react-three-fiber`/`drei` on a deterministic Fibonacci-sphere layout.
- Keep the aesthetic minimalist/enterprise (neutral dark, single accent, clean type, minimal chrome).
- Make every node and edge a navigable link (select → detail; hover → highlight).
- Consume the existing `/ingest/extract` shape; derive `mention_count`-like weighting client-side.

**Ask first**
- Adding new backend endpoints or changing the ingest response shape.
- Introducing a state-management library or routing.

**Never do**
- Change any v1 contract or generated type.
- Add LLM, retrieval, or extraction logic (those are later slices).
- Touch Rust or the N-API binding.
- Reintroduce Cytoscape.
- Build hierarchy aggregation/clustering (deferred to TBD).

## Testing Strategy

- **Goal-based (type + build):** `pnpm --filter demo-frontend typecheck` and `pnpm --filter demo-frontend build` must pass — catches the r3f/drei/React-18 type and JSX integration that is the riskiest part of this slice.
- **Manual QA (visual + interaction):** run the demo, ingest the default code snippet, confirm the 3D graph renders with deterministic positions, node-click opens the detail panel, edge-hover highlights, orbit/zoom works, and the aesthetic reads minimalist/enterprise.

## Acceptance Criteria

- [x] Cytoscape (`cytoscape`, `@types/cytoscape`) removed from `demo/frontend`; `three`, `@react-three/fiber`, `@react-three/drei` added.
- [x] A `Graph3D` view renders `entities` + `relationships` from an ingest result on a deterministic Fibonacci-sphere layout (stable positions across renders for the same input).
- [x] Selecting a node opens a detail panel showing the entity's name, kind, and source references as links; selecting an edge shows its predicate + endpoints + confidence.
- [x] Hover highlights a node and its incident edges; orbit/zoom/pan work via OrbitControls.
- [x] Minimalist/enterprise aesthetic: neutral dark background, single accent color, clean typography, minimal chrome (no gratuitous glow/particles).
- [x] `IngestPanel` uses `Graph3D` (Cytoscape code path removed); the rest of the app (memory, taxonomy, search panels) is unaffected.
- [x] `pnpm --filter demo-frontend typecheck` and `pnpm --filter demo-frontend build` pass.
