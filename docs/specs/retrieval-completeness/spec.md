# Spec: retrieval-completeness

Status: Shipped (Phases 1–4; Phase 5 optional)
Mode: light (multi-slice; this spec phases the work — each phase is its own PR)
Shape: service
Constrained by: RFC-0005 (backend-agnostic retrieval composition), ADR-0022 (engine neutrality — new indexes are adapter cells behind the `RetrievalIndex` port)
- **Contract:** none — re-uses the existing `RetrievalMode` enum + `RetrievalIndex` port; no domain-contract change.

> **Closure summary (2026-07-30).** On implementation, the "55% retrieval-mode
> gap" proved stale: the live recall path (`SqlUnifiedRecall`) runs **all**
> `retrieval_lanes` and fuses via RRF (there is no mode-router in the live
> path), and the durable lanes already cover every mode-signal — Semantic
> (vector), Graph (graph), Keyword (lexical), **Cue** (facts lane's
> `cue_score`), **Hierarchical/community** (`associative_recall_lane` +
> `community_summary_recall_lane`). The one genuinely-missing signal was
> **Temporal**, now shipped (Phase 2). Predictive retrieval is reachable
> (Phase 1). Phases 3–4 need no new lanes (covered above); Phase 5 (auto-wire
> hints into retrieve) remains as an optional enhancement.

## Objective

Close the retrieval-mode completeness gap (the 55% divergence item): the
`RetrievalMode` enum declares **Temporal, Cue, Hierarchical** alongside the
durable Semantic/Graph/Keyword, but the first three previously existed only in
the retired process-local fixture and have **no durable adapter-backed
`RetrievalIndex`** today. Re-land each as a durable index over SQLite, and wire
predictive retrieval (`RetrievalHints`) so it is reachable from the agent
surface.

## Phases (each → its own PR)

### Phase 1 — predictive retrieval reachable (SLICE; ships first)
Expose `predict_context` as an MCP tool wrapping `RecentActivityPredictor`
(`engram-retrieval::predict`). An agent passes its `AgentState` (task + recent
queries/targets) and gets back `RetrievalHints` (predicted queries + target ids)
to feed into `recall`. Also adds the missing `predict.rs` unit tests (the
predictor currently has none). No contract change; the predictor is
dependency-free.

### Phase 2 — Temporal mode on a durable index  ✅ shipped
A `TemporalRetrievalIndex` in `engram_store_sqlite/src/memory/temporal_retrieval.rs`
implementing `RetrievalIndex`: recency-weighted (exponential half-life decay)
memory candidates over `SqlMemoryService::list_memories_in_scope`, filtered to
active + newest-first. Wired as a `retrieval_lanes` entry in the SQLite
bootstrap so recall gains a recency signal fused alongside graph/vector/lexical.
Pure `recency_score` + `rank_temporal` unit-tested; mirrors `GraphRetrievalIndex`.
(The original "registered as a route" framing was revised: `SqlUnifiedRecall`
runs all lanes and fuses — there is no mode-router in the live path — so the
temporal lane is a fused lane, not a `RetrievalMode`-routed one.)

### Phase 3 — Cue mode on a durable index  ✅ covered (no new lane needed)
Reassessment: the **facts lane** (`SqlMemoryService::retrieve`) already scores
cues — `cue_score(record, cues) → CueMatch` is blended into each candidate's
`total`. `RetrievalRequest.cues` flows through it. A separate pure-Cue
`RetrievalIndex` lane would duplicate that signal for no fusion gain. Closed:
cue-based retrieval is already durable; no new lane warranted.

### Phase 4 — Hierarchical-expansion on a durable index  ✅ covered (no new lane needed)
Reassessment: the SQLite bootstrap already wires two graph-topological
expansion lanes — `associative_recall_lane` (Personalized PageRank /
HippoRAG-style seed expansion) and `community_summary_recall_lane` (GraphRAG
community detection + summary ranking). Together they cover seed-expansion and
cluster/hierarchy signals. A separate `HierarchyExpansionIndex` would duplicate
these. Closed: hierarchical/community expansion is already durable via those
lanes; revisit only if a distinct cluster-walk signal (beyond PPR + community
summary) is later shown to add recall quality.

### Phase 5 — predictive integration into retrieve (deeper wire)
Feed `RetrievalHints` into the retrieve path (auto-expand queries / pre-seed
targets). Requires an `AgentState` seam at the recall layer without bloating
`RetrievalRequest` — designed in this phase, not Phase 1.

## Boundaries

### Always do
- Each new mode is an adapter cell behind the existing `RetrievalIndex` port; engine-neutral.
- Reuse existing scoring (codegraph/temporal) + stores (SQLite memory/knowledge/hierarchy) — no new infrastructure.
- A mode that can't resolve reports a degraded `unsupported_mode` failure (existing router behavior), never aborts recall.

### Never do
- Change `RetrievalMode` or `RetrievalRequest` shape for Phases 1–4 (contract stability).
- Re-introduce a process-local fixture — the modes must be durable/adapter-backed.
- Put SQL or engine types in `engram-retrieval` core (ADR-0022).

## Testing Strategy
- **Phase 1**: TDD the `RecentActivityPredictor` (tokenization, task terms, target-id pass-through, empty state) + a goal-based check that the `predict_context` tool returns hints over stdio.
- **Phases 2–4**: TDD each index against an in-memory/SQLite store; goal-based check that `RetrievalMode::<Mode>` returns ranked candidates composed via RRF.

## Acceptance Criteria (Phase 1 — this slice)
- [ ] `engram-mcp` depends on `engram-retrieval`.
- [ ] `predict_context` MCP tool takes `AgentState` fields and returns `RetrievalHints`.
- [ ] `RecentActivityPredictor` has unit tests (tokenization, task, target ids, empty).
- [ ] Tool registered in `main.rs`; gates green.
