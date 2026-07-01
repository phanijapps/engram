# Plan: benchmark-lazy-embeddings

## Changelog
- **2026-07-01:** Implemented. Narrowed from the original research-flavored plan
  to a concrete, two-claim benchmark (quality + warm-up) and the minimal wiring
  to test it. Original tasks (T1–T5 below) are retained as history; the
  implemented tasks (I1–I5) are what shipped.

## Implemented tasks

### I1 — Lazy primitives on the retrieval engine (Rust)
- **Depends on:** none.
- **Tests:** `#[ignore]` Rust unit tests (need cached BGE model) — idempotency,
  cache stats, clear, search-by-stored-id.
- **Approach:** add `indexChunkJson` (idempotent embed-on-touch), `cacheStatsJson`,
  `clearJson` to `NativeRetrievalEngine` (`bindings/node/src/lib.rs`), over the
  existing `SqliteVectorIndex`. Expose via the `fastembed` feature. No domain/
  contract change.

### I2 — Lazy-embedding orchestrator + transport types (TS)
- **Depends on:** I1.
- **Tests:** typecheck.
- **Approach:** extend `NativeRetrievalTransport` (`packages/node/src/transport.ts`)
  with `indexChunk` / `cacheStats` / `clear` + response types; new
  `demo/backend/src/lazy_embeddings.ts` orchestrator (available/enabled guards,
  `semanticChunksFor`, `embeddedCount`, `coveragePercent`, `resetCache`).

### I3 — Wire semantic chunks into Q&A
- **Depends on:** I2.
- **Tests:** typecheck; end-to-end `/qa/ask` still works with embeddings off.
- **Approach:** `answerQuestion` accepts `useLazyEmbeddings`; `semanticChunksFor`
  feeds a `semanticChunks` tier into `buildEvidence` (merged with entity-ref,
  before text-term fallback); retrieved evidence fed as a grounding preamble to
  the agentic session. Fails closed to KG-only.

### I4 — Benchmark harness + `/bench/lazy`
- **Depends on:** I3.
- **Tests:** `/bench` returns KG-only baseline; `/bench/lazy` returns warm-up
  series with rising coverage.
- **Approach:** `runBenchmark(askFn, {passes, coverage, onQuery})` with per-query
  `cacheCoverage`/`embeddedChunks`/`totalChunks` + per-pass summaries; `QUESTIONS_50`.
  `/bench` forces embeddings off; `/bench/lazy` resets cache, computes
  `totalChunks` once, runs N passes, returns the series.

### I5 — Docs + run
- **Depends on:** I4.
- **Approach:** finalize this spec + plan; record results in
  `docs/perf/lazy-embeddings.md`; cross-link from `docs/perf/PERFORMANCE.md`.

## Original tasks (history)

### T1 — Research hypothesis draft
- N/A (research). Drafted the lazy-embeddings hypothesis: KG provides structural
  retrieval, embeddings add semantic, latency amortizes across queries.

### T2 — Index Microsoft Terminal
- Goal-based — successful index. Done via `/ingest/jobs` with force; 6,787
  entities / 14,763 chunks, no embeddings during indexing.

### T3 — Build eval suite
- TDD — eval fixtures. Shipped as `QUESTIONS` (8) + `QUESTIONS_50` (50) in
  `demo/backend/src/bench.ts`, mirrored in `docs/perf/eval-{8,50}-question.md`.

### T4 — Run evals (KG-only vs lazy)
- Depends on T2, T3. `/bench` (KG-only) vs `/bench/lazy` (hybrid, multi-pass).

### T5 — Write perf doc
- Depends on T4. `docs/perf/lazy-embeddings.md` (replaces the original
  PERFORMANCE.md-only plan; PERFORMANCE.md now cross-links it).
