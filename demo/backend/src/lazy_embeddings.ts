// Lazy (query-time) embeddings for Q&A grounding.
//
// Index time builds only the knowledge graph — no embeddings. At query time we
// lazily embed (BGE-small, via the Rust FastEmbed engine) the chunks the graph
// surfaces, cache them by stable id, and semantic-rerank. The cache is cold at
// process start and fills as queries touch chunks; later queries reuse cached
// vectors (cheap) instead of re-running inference. This is the "combination of
// embedding + tree walk": the graph selects candidates, embeddings re-rank them.
//
// Every entry point fails closed — on any error (or when the native addon lacks
// the FastEmbed feature) it returns empty, so Q&A degrades to the KG-only path.

import { getRetrievalTransport } from "./engram.js";
import type { QaChunk } from "./qa.js";

/** Per-query candidate pool: how many chunks the graph surfaces for embedding. */
const POOL = Number(process.env.ENGRAM_LAZY_POOL ?? "16");

const STOP = new Set([
  "the", "a", "an", "is", "are", "was", "were", "of", "to", "in", "on", "for",
  "and", "or", "how", "what", "why", "who", "when", "do", "does", "did", "with",
  "this", "that", "it", "find", "show", "get", "give", "tell",
]);

function terms(question: string): string[] {
  return question
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((t) => t.length > 2 && !STOP.has(t));
}

let cachedEnabled: boolean | null = null;
let cachedAvailable: boolean | null = null;

// Cumulative warm-up counters across a benchmark run (reset with the cache).
// Cache HITS (embedded:false) vs MISSES (embedded:true) + time in inference —
// the deterministic warm-up signal, unaffected by LLM variance.
let warmup = { calls: 0, hits: 0, misses: 0, embedMs: 0 };

export type WarmupSnapshot = {
  calls: number;
  hits: number;
  misses: number;
  embedMs: number;
  hitRate: number; // %
};

export function resetWarmupCounters(): void {
  warmup = { calls: 0, hits: 0, misses: 0, embedMs: 0 };
}

export function warmupSnapshot(): WarmupSnapshot {
  const { calls, hits, misses, embedMs } = warmup;
  return {
    calls,
    hits,
    misses,
    embedMs: Math.round(embedMs),
    hitRate: calls > 0 ? Math.round((hits / calls) * 1000) / 10 : 0,
  };
}

/** Whether lazy embeddings are opted in (env `ENGRAM_LAZY_EMBEDDINGS`, default on). */
export function lazyEmbeddingsEnabled(): boolean {
  if (cachedEnabled !== null) return cachedEnabled;
  const raw = (process.env.ENGRAM_LAZY_EMBEDDINGS ?? "1").toLowerCase();
  cachedEnabled = raw !== "0" && raw !== "false" && raw !== "off";
  return cachedEnabled;
}

/** Whether the native addon exposes the FastEmbed retrieval engine. Memoized. */
export function lazyEmbeddingsAvailable(): boolean {
  if (cachedAvailable !== null) return cachedAvailable;
  try {
    // Constructing the engine loads the BGE model; this is the feature probe.
    getRetrievalTransport();
    cachedAvailable = true;
  } catch {
    cachedAvailable = false;
  }
  return cachedAvailable;
}

/** Pick up to `pool` chunks whose text matches query terms (the tree-walk proxy). */
function selectCandidates(question: string, chunks: QaChunk[], pool: number): QaChunk[] {
  const ts = terms(question);
  if (ts.length === 0) return chunks.slice(0, pool);
  return chunks
    .map((c) => ({
      c,
      score: ts.reduce((n, t) => n + (c.text.toLowerCase().includes(t) ? 1 : 0), 0),
    }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, pool)
    .map((x) => x.c);
}

/**
 * Returns up to `topK` chunks semantic-ranked against the question, embedding
 * candidates on demand. Empty when embeddings are off/unavailable, on error,
 * or while the cache is still cold for this query's neighborhood.
 *
 * `enabled` overrides the env default so callers (the benchmark) can force
 * lazy embeddings on or off per run for a clean A/B.
 */
export async function semanticChunksFor(
  question: string,
  allChunks: QaChunk[],
  topK: number,
  enabled: boolean = lazyEmbeddingsEnabled(),
): Promise<QaChunk[]> {
  if (!enabled || !lazyEmbeddingsAvailable() || allChunks.length === 0) {
    return [];
  }
  const transport = getRetrievalTransport();
  // Embed-on-touch: idempotent per chunk. Per-chunk failures are swallowed so
  // one bad chunk can't poison the whole query. Track hits/misses + inference
  // time for the warm-up curve (a cache hit reuses a vector, no inference).
  for (const candidate of selectCandidates(question, allChunks, POOL)) {
    if (!candidate.text) continue;
    const t0 = performance.now();
    try {
      const res = await transport.indexChunk(candidate.id, candidate.text.slice(0, 2000));
      warmup.calls++;
      if (res.embedded) warmup.misses++;
      else warmup.hits++;
    } catch {
      warmup.calls++;
      warmup.misses++;
    }
    warmup.embedMs += performance.now() - t0;
  }
  // Query the index — only already-embedded chunks can match, so coverage grows
  // across the query stream (the warm-up effect).
  let hits: { id: string }[];
  try {
    hits = await transport.search(question, topK);
  } catch {
    return [];
  }
  const byId = new Map(allChunks.map((c) => [c.id, c]));
  const ranked: QaChunk[] = [];
  for (const hit of hits) {
    const chunk = byId.get(hit.id);
    if (chunk) ranked.push(chunk);
  }
  return ranked;
}

/** Number of chunks currently embedded (cache-coverage numerator). */
export async function embeddedCount(): Promise<number> {
  if (!lazyEmbeddingsAvailable()) return 0;
  try {
    return (await getRetrievalTransport().cacheStats()).embedded;
  } catch {
    return 0;
  }
}

/** Coverage % of the embedded cache against a total chunk count. */
export function coveragePercent(embedded: number, total: number): number {
  if (total <= 0) return 0;
  return Math.round((embedded / total) * 1000) / 10;
}

/** Clears the embedded-chunk cache + vector index (cold start for the benchmark). */
export async function resetCache(): Promise<void> {
  if (!lazyEmbeddingsAvailable()) return;
  try {
    await getRetrievalTransport().clear();
  } catch {
    // ignore
  }
}
