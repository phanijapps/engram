# Plan: enterprise-3d-graph (RFC 0004 Slice 0 / PHASE57)

Follows RFC-0004 D5. Frontend-only; consumes the existing `/ingest/extract` shape.

## Tasks

### T1 — Swap graph dependencies
- **Tests:** goal-based — `pnpm --filter demo-frontend typecheck` passes after the swap (no Cytoscape imports remain).
- **Depends on:** none
- **Verification mode:** goal-based check
- **Approach:** In `demo/frontend/package.json`, remove `cytoscape` + `@types/cytoscape`; add `three`, `@react-three/fiber`, `@react-three/drei`, and `@types/three` (dev). Run `pnpm install` (workspace-aware). Remove the Cytoscape import/usage from `IngestPanel.tsx` is T4; T1 only fixes deps + confirms the install + a clean typecheck of the *remaining* (non-graph) app by temporarily stubbing the panel — simpler: do T1 + T4 together so the app never references a missing dep.

### T2 — `Graph3D` component (layout + render + aesthetic)
- **Tests:** goal-based — typecheck + `vite build` pass; the component is typed against a `GraphData` interface derived from the ingest response.
- **Depends on:** T1
- **Verification mode:** goal-based check
- **Approach:** New `demo/frontend/src/Graph3D.tsx`. Define `GraphData { entities: GraphEntity[]; relationships: GraphRelationship[] }` where `GraphEntity = { id; name; kind; degree; sourceRefs }`. Compute a **deterministic Fibonacci-sphere position** per entity (golden-angle lattice: `y=1-(2i)/(n-1)`, `radius=sqrt(1-y^2)`, `theta=i*goldenAngle`) so the same input always yields the same layout. Render nodes as instanced/small meshes sized by `degree`, colored by `kind` from a restrained palette (one accent family). Edges via drei `<Line>` (or a single BufferGeometry) at low opacity. `<Canvas>` with `<OrbitControls>` (damping, slow optional auto-rotate), neutral-dark background, ambient + one directional light.

### T3 — Navigable links + detail panel
- **Tests:** goal-based — typecheck + build pass; selection state lifts to the panel.
- **Depends on:** T2
- **Verification mode:** goal-based + manual QA
- **Approach:** `Graph3D` accepts `onSelect(entityId?)` / `onHover` and highlights the selected node + incident edges (emissive bump, dim others). A sibling `GraphDetailPanel` renders the selected entity (name, kind, source refs as `<a>` links to their document path/chunk) or the selected relationship (predicate, endpoints, confidence). Source-ref link target derives from `EvidenceRef` fields available on the entity; where no path exists, render the ref id as a non-breaking label.

### T4 — Wire into `IngestPanel`; remove Cytoscape; styles
- **Tests:** goal-based — `typecheck` + `build` pass with zero Cytoscape references.
- **Depends on:** T3
- **Verification mode:** goal-based check + manual QA
- **Approach:** Replace the Cytoscape `useEffect`/`cyRef` block in `IngestPanel.tsx` with `<Graph3D data={…}>` + `<GraphDetailPanel>`; derive `degree` per entity from `result.relationships`. Update `styles.css` for the minimalist/enterprise aesthetic (panel chrome, detail panel, canvas sizing). Delete the `cytoscape` import.

### T5 — Validate
- **Tests:** `pnpm --filter demo-frontend typecheck` and `pnpm --filter demo-frontend build` green; manual QA checklist (render, deterministic layout, node/edge interaction, aesthetic).
- **Depends on:** T4
- **Verification mode:** goal-based + manual QA
- **Approach:** Run the gates; fix type/build errors; run the demo and walk the AC checklist.

## Out of scope (logged, not done here)
- Provenance/confidence *encoding depth* (Slice 4), hierarchy (TBD), LLM extraction (Slice 2), scale ingestion (Slice 1).
