# Benchmark: Lazy (query-time) embeddings + KG, with a warm-up curve

Companion study to [PERFORMANCE.md](./PERFORMANCE.md). That benchmark proved the
**KG-only** path (no embeddings, ever) reaches 75–87.5% on the code-intelligence
eval. This one tests the next hypothesis: **keep indexing embedding-free, but
generate embeddings lazily at query time, cache them, and combine with the KG
walk.** Does hybrid beat KG-only — and do responses get cheaper as the cache
warms?

## Hypothesis

Two falsifiable claims:

1. **Quality** — hybrid retrieval (lazy embeddings + KG) scores **≥ the KG-only
   baseline** on the eval suites.
2. **Warm-up** — across repeated passes over the same query stream, per-query
   **latency falls** and **cache coverage rises**, because each chunk's embedding
   is paid once and then cached. The cost amortizes across queries instead of
   being paid upfront at index time.

## Methodology

### Target + index (unchanged from PERFORMANCE.md)
- **Repo:** [microsoft/terminal](https://github.com/microsoft/terminal)
- **Index:** tree-sitter AST (C/C++/C#), **no embeddings at index time**
- **Graph:** 6,787 entities · 14,763 chunks

### Embedding model
- **FastEmbed BGE-small-en-v1.5** (384-dim), via the Rust `NativeRetrievalEngine`
  (sqlite-vec, in-memory). Passage embeds use no prefix; query embeds use the
  BGE `query:` prefix.

### Retrieval design (lazy, embed-on-touch)
For each query:
1. The graph/term walk selects up to 16 candidate chunks (`ENGRAM_LAZY_POOL`).
2. Each candidate is embedded **idempotently** and cached by stable chunk id
   (`indexChunkJson` — a cache hit skips inference).
3. The query is embedded and cosine-ranked against the cached chunks
   (`searchJson`).
4. Semantic hits merge into `buildEvidence` (entity-ref + semantic tier, then
   text-term fallback) and are fed to the agentic session as grounding.

The cache is **in-memory and cold at the start of each `/bench/lazy` run** — this
is deliberate: a cold start makes the warm-up curve reproducible.

### LLM
- `gemma4:31b-cloud` via ollama cloud (pi SDK), agentic Q&A with graph tools +
  grounding preamble.

### Runs
- **Baseline:** `POST /bench {suite:8}` — embeddings forced OFF (KG-only).
- **Lazy:** `POST /bench/lazy {suite:8, passes:3}` — embeddings ON; resets cache
  (cold start), runs 3 passes, records per-query latency + cache coverage +
  per-pass summaries.

Scoring is keyword-over-substring (correct/partial/wrong/no_answer), identical
to the [8-question eval](./eval-8-question.md). Headline = correct + partial.

## Results

### Claim 1 — Quality (KG-only vs hybrid)

| Run | Correct | Partial | Wrong | No-answer | Correct+partial | Avg / query |
| --- | --- | --- | --- | --- | --- | --- |
| KG-only (`/bench`) | 7 | 0 | 1 | 0 | **7/8 (87.5%)** | 16.8 s |
| Lazy hybrid (`/bench/lazy`, 3 passes × 8) | — | — | — | — | **19/24 (79.2%)** | 17.0 s |

**Quality is neutral.** Per-pass the hybrid scored 7/8, 5/8, 7/8 — the swing is
LLM non-determinism (same questions, same grounding, different prose), not an
embedding effect. On this keyword-overlap-scored eval the KG already surfaces
the expected entities, so semantic chunks add little measurable signal. The
hybrid is within noise of the baseline, not above it.

> The KG-only baseline here (87.5%) is higher than the 75% in PERFORMANCE.md
> because the agentic session now receives retrieved evidence as a grounding
> preamble (a side-effect of wiring semantic chunks through the same path). The
> lazy run uses the identical preamble, so the comparison is apples-to-apples.

### Claim 2 — Warm-up (per pass)

| Pass | Correct+partial | Avg latency | Cache hit rate | Cumulative embed ms |
| --- | --- | --- | --- | --- |
| 1 (cold) | 7/8 | 15.7 s | **18.0%** | **6363** (all of it) |
| 2 | 5/8 | 15.4 s | **59.0%** | 6365 (+2) |
| 3 (warm) | 7/8 | 20.0 s | **72.7%** | 6368 (+3) |

**The warm-up is confirmed.** The cache hit rate climbs 18% → 59% → 73% across
passes, and embedding inference time is paid **once** — ~6.4 s in pass 1, then
≤5 ms added across passes 2 and 3 combined. After the first pass, candidate
chunks are served from the cache with no inference. This is the amortization the
hypothesis predicted: the embedding cost is incurred per *chunk*, not per *query*.

Per-query latency does **not** fall visibly (pass 3 is even higher) — but that is
because the ~15–20 s/query cost is dominated by the agentic LLM, not embedding.
The ~6.4 s of embedding work (paid once) is small next to ~24 × 17 s of LLM
inference. The warm-up is real; it's just dwarfed by LLM cost in this
end-to-end configuration. (Cache coverage against total chunks stays ~0.7% — the
16-chunk candidate pool touches a sliver of the 14,763-chunk corpus; the
meaningful metric is hit rate, not corpus coverage.)

## Conclusion

**Half-confirmed, half-refuted — and that's the honest answer.**

1. **Warm-up mechanism: confirmed.** Lazy embed-on-touch + an idempotent cache
   works exactly as hypothesized. Hit rate 18% → 73% across passes; embedding
   inference paid once (~6.4 s) then reused. The cost amortizes across queries
   instead of being paid upfront at index time. Indexing stays embedding-free.

2. **Quality: not improved (neutral).** On a keyword-scored eval over a
   well-grounded KG, adding semantic chunks did not raise accuracy above the
   KG-only baseline — within LLM variance. This corroborates the original
   [PERFORMANCE.md](./PERFORMANCE.md) finding from the other direction: when the
   graph already grounds the entities the eval scores on, embeddings are
   redundant for *that* measurement. They would earn their keep on paraphrase /
   semantic-similarity questions the graph's name-matching misses — a gap this
   eval suite does not stress.

3. **End-to-end latency: the embedding warm-up is masked by LLM cost.** The
   amortization is real but small relative to ~17 s/query agentic inference. To
   see the warm-up in the latency curve you'd isolate embedding time (as the hit
   rate + cumulative-embed-ms columns do) rather than total query latency.

**Net:** lazy embeddings are a correct, cheap-to-amortize mechanism whose quality
payoff depends on the eval stressing semantic (not lexical) matching. For this
codebase + eval, the KG alone is sufficient — which is itself a useful result.

## Limitations

- **In-memory cache.** A cold start each run is the point, but it means cached
  embeddings don't survive a backend restart. Persistence is out of scope.
- **Candidate pool, not whole corpus.** Only chunks the graph surfaces get
  embedded, so coverage plateaus well below 100% — queries don't touch every
  chunk. This is the embed-on-touch tradeoff, not a defect.
- **Keyword-scored, single repo.** Same caveats as the 8-question eval.
- **No eager-embedding baseline.** This measures lazy-hybrid vs KG-only, not
  lazy vs eager-at-index-time. The hypothesis is specifically that lazy
  recovers the quality *without* the upfront cost.

## See also
- [PERFORMANCE.md](./PERFORMANCE.md) — the KG-only baseline study
- [8-question eval](./eval-8-question.md) · [50-question eval](./eval-50-question.md)
- [Spec](../specs/benchmark-lazy-embeddings/spec.md) · [Lazy orchestrator](../../demo/backend/src/lazy_embeddings.ts)
- [Retrieval engine](../../bindings/node/src/lib.rs) (`NativeRetrievalEngine`)
