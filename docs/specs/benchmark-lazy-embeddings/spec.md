# Spec: benchmark-lazy-embeddings (embed at query time, not index time)

- **Status:** Shipped
- **Shape:** mixed (eval + service)
- **Constrained by:** `engram-eval` (deterministic eval harness); [`fastembed-passage-embeddings`](../fastembed-passage-embeddings/spec.md) (the BGE-small passage embedder this builds on); IT-org ontology as baseline
- **Contract:** none

## Objective

Prove (or disprove) the hypothesis that **lazy embeddings** — embedding text at
Q&A time rather than during indexing — combined with the knowledge graph produce
strong Q&A results, **and** that responses get more efficient as the embedding
cache warms. Two falsifiable claims:

1. **Quality** — hybrid retrieval (lazy embeddings + KG) scores at least as well
   as the KG-only baseline on the [8-question](../../perf/eval-8-question.md) and
   [50-question](../../perf/eval-50-question.md) suites.
2. **Warm-up** — across repeated passes over the same query stream, per-query
   latency falls and cache coverage rises, because embeddings are paid once per
   chunk and then cached. Indexing stays embedding-free (fast, dependency-free);
   the embedding cost amortizes across real queries.

The counterfactual — embeddings computed eagerly for the whole corpus at index
time — is deliberately *not* built; this spec tests whether lazy amortization
recovers the quality without the upfront cost.

## Decision

- **Index time:** unchanged — only the knowledge graph is built. No embeddings.
- **Query time (lazy, embed-on-touch):** for each query, the graph/term walk
  selects a candidate chunk pool; each candidate is embedded idempotently
  (BGE-small passage) and cached by stable chunk id; the query embedding then
  cosine-ranks the cached chunks. This is the literal "combination of embedding
  + tree walk."
- **Fusion point:** evidence-level, not an agentic tool — semantic chunks merge
  into `buildEvidence`'s chunk tiers (entity-ref + semantic, then text-term
  fallback). Deterministic and measurable; no LLM tool-calling variance.
- **Agentic grounding:** the retrieved evidence (now including semantic chunks)
  is fed to the agentic session as a grounding preamble so it shapes the
  synthesized answer, not only the sources list.
- **Cache:** in-memory, cold start per benchmark run (this is what makes the
  warm-up curve reproducible). Persisted/on-disk cache is out of scope.
- **A/B:** `/bench` forces embeddings off (KG-only baseline); `/bench/lazy`
  forces them on and runs `passes` passes, recording per-query latency +
  cache-coverage + per-pass summaries.

## Boundaries

### Always do
- Keep indexing embedding-free; embeddings are generated only at query time.
- Make every lazy-embedding entry point fail closed (return empty / no-op) so
  Q&A degrades to the KG-only path when the native addon lacks the FastEmbed
  feature, the model is unavailable, or any embedding call errors.
- Gate the embedding engine behind the existing `fastembed` Cargo feature.

### Ask first
- Changing the fusion strategy away from evidence-level (e.g. moving semantic
  retrieval into an LLM tool, or replacing `buildEvidence` with `compose_context`).
- Persisting the embedding cache to disk (changes the cold-start assumption the
  benchmark depends on).

### Never do
- Add embedding/vector code to the core domain crates; embeddings stay in the
  sqlite-vec adapter + node binding, orchestration in the demo backend.
- Embed the whole corpus eagerly at index time (that is the eager baseline this
  spec deliberately avoids).
- Make the Q&A path throw on an embedding failure.

## Testing Strategy

- **Unit (Rust, `#[ignore]` — needs cached BGE model):** `indexChunkJson`
  idempotency (second call is a cache hit, no re-embed), `cacheStatsJson`
  counts, `clearJson` empties, `searchJson` returns stored chunk ids. Run with
  `cargo test -p engram-node --features fastembed --lib retrieval:: -- --ignored`.
- **Type/compile:** `pnpm --filter @engram/node typecheck`, `pnpm --filter demo-backend build`.
- **End-to-end (goal-based):** with Microsoft Terminal indexed + LLM creds,
  `POST /bench {suite:8}` returns the KG-only baseline; `POST /bench/lazy
  {suite:8,passes:3}` returns rising coverage and falling latency across passes
  and lazy accuracy ≥ baseline. Numbers recorded in
  [`docs/perf/lazy-embeddings.md`](../../perf/lazy-embeddings.md).

## Acceptance Criteria

- [x] Index time produces no embeddings; the knowledge graph is the only index artifact.
- [x] Lazy, idempotent, cached chunk embedding at query time (`NativeRetrievalEngine.indexChunkJson` + `lazy_embeddings.ts`).
- [x] Semantic chunks fuse into Q&A grounding (`buildEvidence` + agentic preamble).
- [x] `/bench` (KG-only) and `/bench/lazy` (hybrid, multi-pass) routes with per-query coverage + latency + per-pass summaries.
- [x] 8- and 50-question suites supported (`QUESTIONS` / `QUESTIONS_50`).
- [x] `docs/perf/lazy-embeddings.md` records: machine specs, KG-only baseline, lazy quality, and the warm-up table (cold → mid → warm).
- [x] Honest conclusion: does lazy embedding improve quality, and does the warm-up curve hold?

## Assumptions

- The demo native addon is built with the `fastembed` feature and the BGE-small
  model is cached locally (it is, in `demo/backend/.fastembed_cache`).
- The benchmark target repo (microsoft/terminal) is indexed and LLM creds are
  present in `demo/backend/.env`.
- Coverage is measured against the scoped chunk count; it will not approach 100%
  because queries do not touch every chunk — that is expected, not a defect.
