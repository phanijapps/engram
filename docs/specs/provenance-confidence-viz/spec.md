# Spec: provenance-confidence-viz (RFC 0004 Slice 4 / PHASE61)

- **Status:** Shipped
- **Shape:** ui
- **Constrained by:** RFC-0004 D4 (provenance + confidence viz); no Rust/contract change — the data already carries `provenance` + `confidence`
- **Contract:** none (frontend-only; consumes already-emitted JSON)

## Objective

The 3D graph makes provenance and confidence visible at a glance. Relationship edges encode confidence (width + opacity) and extraction method (deterministic vs LLM — distinct hue); nodes encode their provenance confidence (opacity); the detail panel shows the full provenance (source, method badge, confidence bar, observed-at) for entities and relationships. A small legend distinguishes deterministic vs LLM edges. Deterministic entities stay the zero-conf baseline; LLM-extracted objects (method `llm_extraction`) are visually marked so the source of every claim stays legible.

## Decision (aligns with RFC D4)

Frontend-only: extend `Graph3D.tsx`'s `RawEntity`/`RawRelationship`/`GraphEntity`/`GraphRelationship` to carry the already-present `provenance` + `confidence`, encode them in the render (edges + nodes), and surface them in the detail panel + legend. No backend, Rust, or contract change.

## Assumptions

- Technical: `/ingest/extract`, `/ingest/scan`, and `/llm/extract` already return entities/relationships carrying `provenance` (`{source, method, confidence, observedAt}`) and relationships carry a top-level `confidence`. (verified — probes + Slice 2 LLM objects use `method:"llm_extraction"`, conf 0.6)
- Technical: drei `<Line>` accepts `transparent` + `opacity` for confidence-based edge opacity. (verified — Line2/LineMaterial)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Consume only fields already in the wire shape (no backend change).
- Keep deterministic entities/edges the visual baseline; mark LLM-derived objects distinctly, never hide them.
- Keep confidence encoding perceptual but accessible (opacity/width, not color-only).

**Ask first**
- Filtering the graph by confidence threshold or method.
- Backend-side provenance enrichment.

**Never do**
- Change Rust, contracts, or the wire shape; introduce a new dependency; make provenance a write path.

## Testing Strategy

- **Goal-based (build):** frontend `typecheck` + `build`.
- **Goal-based (plumbing):** ingest deterministic + LLM-enhanced graphs, confirm edges differ by method/confidence and the detail panel shows provenance.
- **Manual QA:** toggle LLM enhance in IngestPanel/ScanPanel, confirm LLM edges render distinctly + the legend + detail provenance.

## Acceptance Criteria

- [x] Edges encode confidence (width + opacity) and method (deterministic vs LLM — distinct hue).
- [x] Nodes encode provenance confidence (opacity) without losing the degree/size encoding.
- [x] The detail panel shows provenance (source, method badge, confidence bar, observed-at) for entities + relationships.
- [x] A legend distinguishes deterministic vs LLM edges.
- [x] No Rust/contract/backend change; frontend typecheck + build pass.
