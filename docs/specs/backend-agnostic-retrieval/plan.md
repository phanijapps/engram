# Plan: backend-agnostic-retrieval

Implements [RFC-0005](../../rfcs/0005-backend-agnostic-retrieval-composition.md) /
[ADR-0009](../../adr/0009-retrieval-composition-seam.md). SQLite-only. Each task
is one PR; `Tests:` before `Approach:`; `Depends on:` explicit.

### T1 — Configurable RRF strength (D7)
- **Tests:** TDD — weighted RRF (per-source weight scales `weight/(k+rank)`); default config (k=60, equal weights) behaves as pure RRF; `Default` impl; existing 8 RRF tests still pass.
- **Approach:** add `ReciprocalFusionConfig { k, default_source_weight, source_weights }` to `core/retrieval/src/config.rs` (mirror `WeightedFusionConfig`); `ReciprocalRankFusion` holds the config; `fuse` applies `config.source_weight(src)/(k+rank)`. Export from `lib.rs`.
- **Depends on:** none (builds on the committed RRF spike).

### T2 — Graph RetrievalIndex (knowledge adapter, behind the port)
- **Tests:** TDD — `retrieve_candidates` returns ranked `RetrievalResult`s of `Entity`/`Chunk` target type for a query + scope; ranking by term relevance; scope filtering respected.
- **Approach:** new `GraphRetrievalIndex` in `adapters/knowledge/sqlite` implementing `RetrievalIndex` over the existing SQLite store; ranks entities (name/kind term match) + chunks (entity-ref / term match) into `RetrievalResult` with `fusion_trace.source = "graph"`. No new storage.
- **Depends on:** none.

### T3 — Composition orchestrator + backend-selection config
- **Tests:** TDD + integration — orchestrator runs [stub graph index, stub vector index] → RRF → `compose_context`; cross-source consensus ranks first; config selects adapters per plane; absent config ⇒ defaults.
- **Approach:** in `core/orchestration`, add the orchestrator (collect candidates from wired `RetrievalIndex`es → `RetrievalFusion::fuse` → `ContextComposer::compose`) + a per-plane backend-selection config (env-overridable; manifest shape reserved — RFC O1). Wire SQLite graph + sqlite-vec by default.
- **Depends on:** T1, T2.

### T4 — Durable sqlite-vec engine wiring + content_hash upsert + lazy GC
- **Tests:** TDD — `open(path)` reopen-survival (already green); `content_hash`-keyed upsert (`ON CONFLICT`) reuses vectors for unchanged chunks; lazy GC skips search hits mapping to no live chunk.
- **Approach:** thread a path option through `NativeRetrievalEngine` (mirror the `dbPath` pattern) to `SqliteVectorIndex::open`; add upsert on insert; skip dead hits at search. Binding + demo construct with a durable path (e.g. `*.embeddings.db`).
- **Depends on:** none (spike ships `open(path)`).

### T5 — Demo Q&A through the seam (RRF-fused hybrid)
- **Tests:** goal-based — `/qa/ask` grounds in orchestrator-produced RRF-fused context; bespoke `buildEvidence` chunk-tiering retired; Q&A still works with embeddings disabled (fail-closed to KG).
- **Approach:** in `demo/backend`, replace the bespoke chunk tiering with an orchestrator call (graph + vector indexes → RRF → context); feed fused context to the agentic grounding preamble. Keep entity/relationship graph evidence as complementary.
- **Depends on:** T1, T2, T3, T4.

### T6 — Benchmark re-run + perf doc
- **Tests:** goal-based — `/bench` (KG-only) vs `/bench/lazy` (RRF-hybrid): hybrid correct+partial ≥ baseline; warm-up hit-rate climbs; second run reuses persisted embeddings.
- **Approach:** re-run both on the indexed terminal repo; record quality + warm-up numbers in `docs/perf/lazy-embeddings.md`; update the spec acceptance checkboxes.
- **Depends on:** T5.

## Sequencing
T1, T2, T4 are independent (parallelizable). T3 waits on T1+T2. T5 waits on T1–T4. T6 waits on T5. Single-pass adversarial review per slice (per project preference).
