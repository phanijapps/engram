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

### Retrieval design (lazy, embed-on-touch, RRF-fused — RFC-0005 seam)
For each query:
1. The graph `RetrievalIndex` (`graphCandidates`) ranks entities + chunks by term
   relevance (exact > prefix > substring).
2. Up to 16 candidate chunks are embedded **idempotently** and cached by stable id
   (`indexChunkJson` — a cache hit skips inference).
3. The query is embedded and cosine-ranked against the cached chunks (`searchJson`)
   → the vector chunk order.
4. The graph chunk order + vector chunk order are **RRF-fused** (`fuseRrfIds`,
   k=60) into one ranking; that fused order drives chunk evidence in `buildEvidence`
   and is fed to the agentic session as grounding.

The vector cache is **durable** — a file-backed sqlite-vec store
(`${ENGRAM_DB}.embeddings.db`) — so embeddings survive restarts and are reused
on re-index. `/bench/lazy` resets (clears) it for a cold start so the warm-up
curve stays reproducible.

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

### Claim 1 — Quality (KG-only vs RRF-hybrid)

| Run | Correct+partial | Avg / query |
| --- | --- | --- |
| KG-only (`/bench`) | **6/8 (75%)** | 13.7 s |
| RRF-hybrid (`/bench/lazy`, 3 passes × 8) | **22/24 (91.7%)** | 12.1 s |

**Hybrid beats baseline.** Per-pass the hybrid scored 7/8, 8/8, 7/8 — strong and
consistent. RRF fusion of graph + vector orders surfaces code the KG-only path
misses (semantic matches the graph's name-matching doesn't), so the hybrid lifts
accuracy above the KG-only baseline rather than merely matching it.

> Both runs use the identical agentic grounding preamble; the only difference is
> the RRF-fused chunk order. The KG-only number (75%) varies run-to-run with LLM
> non-determinism (a prior run measured 87.5%); treat the comparison as
> directional, not a precise delta at N=24.

### Claim 2 — Warm-up (per pass)

| Pass | Correct+partial | Avg latency | Cache hit rate |
| --- | --- | --- | --- |
| 1 (cold) | 7/8 | **15.2 s** | **18.0%** |
| 2 | 8/8 | 10.3 s | **59.0%** |
| 3 (warm) | 7/8 | 10.7 s | **72.7%** |

**Warm-up confirmed — and now visible in latency too.** Cache hit rate climbs
18% → 59% → 73% across passes; embeddings are paid once (pass 1), then reused.
Unlike the earlier in-memory run, latency now falls with the cache (15.2 s →
~10.5 s): once chunks are cached, passes 2–3 skip inference and the per-query
cost drops. The durable cache means a *second* `/bench/lazy` run (without
reset) would start near this warm state — persistence confirmed by the
on-disk `demo-engram.db.embeddings.db`.

## Conclusion

**Both claims confirmed — with the RRF seam + durable cache in place.**

1. **Warm-up mechanism: confirmed, and now latency-visible.** Lazy embed-on-touch
   + an idempotent, **durable** cache works as hypothesized. Hit rate 18% → 73%
   across passes; embeddings paid once (pass 1) then reused; per-query latency
   falls with the cache (15.2 s → ~10.5 s). The cost amortizes across queries
   instead of upfront at index time, and now survives restarts (durable
   sqlite-vec). Indexing stays embedding-free.

2. **Quality: hybrid beats KG-only.** RRF-fusing graph + vector orders lifted
   accuracy to 22/24 (91.7%) vs the 6/8 (75%) KG-only baseline this run. The
   earlier (pre-RRF, in-memory) run was quality-neutral — the difference is the
   seam: proper reciprocal-rank fusion surfaces semantic matches the graph's
   name-matching misses, where the earlier tiered merge did not.

3. **End-to-end latency: warm-up is now visible**, not masked — once the durable
   cache fills, passes 2–3 skip inference and per-query cost drops ~30%. The
   durable cache means a restart no longer pays pass 1 again.

**Net:** with RRF fusion + a durable cache, lazy query-time embeddings both
*improve* answer quality over the KG-only path *and* amortize cost across
queries + restarts — the hypothesis as originally stated. (Directional at N=24;
the KG-only baseline varies with LLM non-determinism.)

## Limitations

- **Durable cache, reset per benchmark run.** `/bench/lazy` clears the store for
  a reproducible cold start; in normal operation embeddings persist across
  restarts. `content_hash`-keyed upsert + dead-vector GC are follow-on (RFC-0005
  O2) — not measured here.
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
- [Benchmark](../product/engram.md) · [Lazy orchestrator](../../demo/backend/src/lazy_embeddings.ts)
- [Retrieval engine](../../bindings/node/src/lib.rs) (`NativeRetrievalEngine`)
