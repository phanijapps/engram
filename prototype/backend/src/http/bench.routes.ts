// Benchmark routes — KG-only baseline vs lazy-embeddings multi-pass.

import type { Hono } from "hono";
import { getKnowledgeTransport } from "../adapters/engram.client.js";
import { answerQuestion } from "../services/qa.service.js";
import { QUESTIONS_50, runBenchmark } from "../services/bench.service.js";
import { embeddedCount, resetCache, resetWarmupCounters, warmupSnapshot } from "../adapters/embeddings.client.js";
import { SCAN_SCOPE } from "../data/scan-defaults.js";

export function registerBenchRoutes(app: Hono): void {
  // KG-only baseline: embeddings forced OFF so this measures the pure
  // graph + term-matching path. Single pass; the reference number for the
  // lazy-embeddings comparison (docs/perf/PERFORMANCE.md).
  app.post("/bench", async (c) => {
    const body = await c.req.json().catch(() => ({} as Record<string, unknown>));
    const use50 = body?.suite === 50;
    const { results, summary } = await runBenchmark(
      async (q) => {
        const r = await answerQuestion(q, SCAN_SCOPE, { useLazyEmbeddings: false });
        return { answer: r.answer, sources: r.sources, llm: r.llm };
      },
      { questions: use50 ? QUESTIONS_50 : undefined },
    );
    return c.json({ results, summary, mode: "kg-only" });
  });

  // Lazy-embeddings run: query-time embeddings + KG, multi-pass to expose the
  // warm-up curve. Resets the embedding cache (cold start), records per-query
  // cache coverage + latency across `passes`, and returns per-pass summaries.
  app.post("/bench/lazy", async (c) => {
    const body = await c.req.json().catch(() => ({} as Record<string, unknown>));
    const passes = Math.max(1, Number(body?.passes ?? 1));
    const use50 = body?.suite === 50;
    await resetCache();
    resetWarmupCounters();
    // Coverage denominator: total chunks in scope (computed once).
    const total = ((await getKnowledgeTransport().listChunks(SCAN_SCOPE)) as unknown[]).length;
    const { results, summary, passes: passSummaries } = await runBenchmark(
      async (q) => {
        const r = await answerQuestion(q, SCAN_SCOPE, { useLazyEmbeddings: true });
        return { answer: r.answer, sources: r.sources, llm: r.llm };
      },
      {
        questions: use50 ? QUESTIONS_50 : undefined,
        passes,
        coverage: async () => ({ embedded: await embeddedCount(), total, ...warmupSnapshot() }),
      },
    );
    return c.json({
      results,
      summary,
      passes: passSummaries,
      totalChunks: total,
      suite: use50 ? 50 : 8,
      mode: "lazy-embeddings",
    });
  });
}
