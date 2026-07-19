# Plan: provenance-confidence-viz (RFC 0004 Slice 4 / PHASE61)

Frontend-only slice. Single commit.

## Tasks

### T1 — Extend graph types + buildGraphData
- **Tests:** goal-based — frontend typecheck.
- **Depends on:** none
- **Approach:** In `Graph3D.tsx`: add `GraphProvenance` (`{source?, method?, confidence?, observedAt?}`); extend `GraphEntity`/`GraphRelationship` with `provenance?` + `method?`/`confidence?`; extend `RawEntity`/`RawRelationship` with `provenance?`. `buildGraphData` passes provenance through; entity confidence = `provenance.confidence`; relationship `method` = `provenance.method`, confidence = top-level `confidence ?? provenance.confidence`.

### T2 — Encode confidence + method in edges + nodes
- **Tests:** goal-based — frontend build; manual.
- **Depends on:** T1
- **Approach:** `GraphEdge`: `isLLM = method === "llm_extraction"`; confidence drives `lineWidth` (`0.5 + confidence*1.2`, bumped when active) + `opacity` (`transparent; 0.4 + 0.6*confidence`, 0.2 when dimmed); color = active ? accent : isLLM ? amber (`#8a6a2a`/`#3a2f1a` dim) : blue-gray (`#3a4768`/`#26304a` dim). `GraphNode`: opacity when not dimmed = `0.55 + 0.45*confidence`.

### T3 — Provenance in detail panel + legend
- **Tests:** goal-based — frontend build; manual.
- **Depends on:** T2
- **Approach:** `DetailOverlay`: add a provenance block (source, method badge — `LLM` when `llm_extraction` else `deterministic`, confidence bar, observed-at) for entities + relationships. Small `.graph3d__legend` (deterministic vs LLM swatches). CSS for the confidence bar + legend.

### T4 — Validate + lighter adversarial pass
- **Tests:** frontend typecheck + build; single-pass review focused on encoding correctness + accessibility + no-scope-creep.
- **Depends on:** T3

## Out of scope (logged)
- Confidence/method filtering; backend provenance enrichment; belief/contradiction viz (Slice 5).
