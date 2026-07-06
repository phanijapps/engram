// Benchmark eval suite for the lazy-embeddings hypothesis.
//
// Two studies share this harness:
//   /bench       — KG-only baseline (no embeddings), single pass.
//   /bench/lazy  — lazy (query-time) embeddings + KG, multi-pass to expose the
//                  warm-up curve: latency falls and cache coverage rises as the
//                  embedding cache fills across passes.
//
// Indexing never embeds. At query time (lazy path) the chunks the graph surfaces
// are embedded on demand and cached; later queries reuse the cached vectors.

export type BenchQuestion = {
  q: string;
  expectContains: string[];
  category: string;
};

// 8-question pilot — the fast smoke suite (docs/perf/eval-8-question.md).
export const QUESTIONS: BenchQuestion[] = [
  { q: "What does the TerminalHandle class do?", expectContains: ["TerminalHandle", "handle"], category: "entity_lookup" },
  { q: "How does text rendering work in the terminal?", expectContains: ["render", "text"], category: "concept" },
  { q: "What is the relationship between Terminal and TerminalConnection?", expectContains: ["Terminal", "Connection"], category: "relationship" },
  { q: "List the main classes in the renderer module", expectContains: ["renderer", "class"], category: "structural" },
  { q: "What does the Settings class manage?", expectContains: ["Settings"], category: "entity_lookup" },
  { q: "How are keyboard shortcuts handled?", expectContains: ["key", "shortcut"], category: "concept" },
  { q: "What is the call chain for writing text to the screen?", expectContains: ["write", "screen", "render"], category: "call_graph" },
  { q: "What are the main components of the terminal architecture?", expectContains: ["component", "module"], category: "structural" },
];

// 50-question trend suite (docs/perf/eval-50-question.md). A strict superset of
// the 8-question pilot (questions 1–8 mirror it verbatim).
export const QUESTIONS_50: BenchQuestion[] = [
  { q: "What does the TerminalHandle class do?", expectContains: ["TerminalHandle", "handle"], category: "entity_lookup" },
  { q: "What does the Settings class manage?", expectContains: ["Settings"], category: "entity_lookup" },
  { q: "What is the role of the TextBuffer?", expectContains: ["TextBuffer", "text"], category: "entity_lookup" },
  { q: "What does the KeyChord class represent?", expectContains: ["KeyChord", "key"], category: "entity_lookup" },
  { q: "What does the Renderer do?", expectContains: ["Renderer", "render"], category: "entity_lookup" },
  { q: "What is the CommandHistory responsible for?", expectContains: ["CommandHistory", "history"], category: "entity_lookup" },
  { q: "What does the TerminalConnection interface define?", expectContains: ["TerminalConnection", "Connection"], category: "entity_lookup" },
  { q: "What does the FontInfo class hold?", expectContains: ["FontInfo", "font"], category: "entity_lookup" },
  { q: "What is the purpose of the GlyphAtlas?", expectContains: ["GlyphAtlas", "glyph"], category: "entity_lookup" },
  { q: "What does the TextAttribute class represent?", expectContains: ["TextAttribute", "attribute"], category: "entity_lookup" },
  { q: "How does text rendering work in the terminal?", expectContains: ["render", "text"], category: "concept" },
  { q: "How are keyboard shortcuts handled?", expectContains: ["key", "shortcut"], category: "concept" },
  { q: "How does the terminal handle VT sequences?", expectContains: ["VT", "sequence"], category: "concept" },
  { q: "How does selection work?", expectContains: ["selection"], category: "concept" },
  { q: "How is the color table applied to text?", expectContains: ["color"], category: "concept" },
  { q: "How does mouse input get processed?", expectContains: ["mouse", "input"], category: "concept" },
  { q: "How does the terminal manage scrolling?", expectContains: ["scroll"], category: "concept" },
  { q: "How does search work in the buffer?", expectContains: ["search"], category: "concept" },
  { q: "What is the relationship between Terminal and TerminalConnection?", expectContains: ["Terminal", "Connection"], category: "relationship" },
  { q: "How do Tab and Pane relate?", expectContains: ["Tab", "Pane"], category: "relationship" },
  { q: "What is the relationship between Renderer and DxRenderer?", expectContains: ["Renderer", "Dx"], category: "relationship" },
  { q: "How do KeyChord and ActionAndArgs relate?", expectContains: ["KeyChord", "Action"], category: "relationship" },
  { q: "What is the relationship between Profile and Settings?", expectContains: ["Profile", "Settings"], category: "relationship" },
  { q: "How do TextBuffer and ROW relate?", expectContains: ["TextBuffer", "ROW"], category: "relationship" },
  { q: "What is the relationship between TerminalPage and TermControl?", expectContains: ["TerminalPage", "control"], category: "relationship" },
  { q: "How do CommandHistory and the shell relate?", expectContains: ["CommandHistory", "shell"], category: "relationship" },
  { q: "List the main classes in the renderer module.", expectContains: ["renderer", "class"], category: "structural" },
  { q: "What are the main components of the terminal architecture?", expectContains: ["component", "module"], category: "structural" },
  { q: "List the main types in the settings model.", expectContains: ["Settings", "Profile"], category: "structural" },
  { q: "What are the main classes involved in text buffering?", expectContains: ["TextBuffer", "ROW"], category: "structural" },
  { q: "List the renderer backends.", expectContains: ["Renderer", "Dx"], category: "structural" },
  { q: "What are the main parts of the input pipeline?", expectContains: ["input", "key"], category: "structural" },
  { q: "List the main components of the connection layer.", expectContains: ["Connection", "terminal"], category: "structural" },
  { q: "What is the call chain for writing text to the screen?", expectContains: ["write", "screen", "render"], category: "call_graph" },
  { q: "Trace how a key press reaches an action handler.", expectContains: ["key", "action"], category: "call_graph" },
  { q: "What is the call chain from VT sequence to text buffer?", expectContains: ["VT", "TextBuffer"], category: "call_graph" },
  { q: "How does a render request propagate to the GPU?", expectContains: ["render", "Dx"], category: "call_graph" },
  { q: "Trace the path from user input to character output.", expectContains: ["input", "output"], category: "call_graph" },
  { q: "What functions are called when the terminal scrolls?", expectContains: ["scroll", "function"], category: "call_graph" },
  { q: "Where is the Terminal class defined?", expectContains: ["Terminal"], category: "navigation" },
  { q: "Who calls the Renderer's Paint method?", expectContains: ["Paint", "Renderer"], category: "navigation" },
  { q: "What file contains the TextBuffer?", expectContains: ["TextBuffer"], category: "navigation" },
  { q: "Which classes use the GlyphAtlas?", expectContains: ["GlyphAtlas"], category: "navigation" },
  { q: "Where is the Settings class implemented?", expectContains: ["Settings"], category: "navigation" },
  { q: "What calls into the TerminalConnection?", expectContains: ["TerminalConnection"], category: "navigation" },
  { q: "How does the renderer interact with the text buffer across files?", expectContains: ["Renderer", "TextBuffer"], category: "cross_file" },
  { q: "How do the settings flow from JSON to runtime objects?", expectContains: ["Settings", "json"], category: "cross_file" },
  { q: "How does the control layer connect to the connection layer?", expectContains: ["control", "Connection"], category: "cross_file" },
  { q: "How many renderer backends are there?", expectContains: ["renderer", "backend"], category: "aggregation" },
  { q: "List all the classes in the terminal core module.", expectContains: ["class", "terminal"], category: "aggregation" },
];

export type EvalResult = {
  question: string;
  category: string;
  answer: string;
  sources: number;
  llm: string;
  elapsedMs: number;
  score: "correct" | "partial" | "wrong" | "no_answer";
  matchedTerms: string[];
  // Warm-up metadata (populated by the lazy multi-pass run).
  pass?: number;
  embeddedChunks?: number;
  totalChunks?: number;
  cacheCoverage?: number; // % of total chunks embedded
  cacheHits?: number; // cumulative cache hits (no inference)
  cacheMisses?: number; // cumulative cache misses (ran inference)
  embedMs?: number; // cumulative ms in embedding inference
  hitRate?: number; // cumulative cache hit rate %
};

export type Summary = {
  total: number;
  correct: number;
  partial: number;
  wrong: number;
  no_answer: number;
  avgMs: number;
  avgCoverage?: number;
};

export type PassSummary = Summary & {
  pass: number;
  embedMs?: number; // cumulative embedding inference ms at end of pass
  hitRate?: number; // cumulative cache hit rate % at end of pass
};

export type BenchOptions = {
  questions?: BenchQuestion[];
  passes?: number;
  /** Coverage snapshot at the moment of each query. */
  coverage?: () => Promise<{
    embedded: number;
    total: number;
    hits?: number;
    misses?: number;
    embedMs?: number;
    hitRate?: number;
  }>;
  /** Stream hook — called after each query with its result. */
  onQuery?: (result: EvalResult) => void;
};

function score(answer: string, expected: string[]): { score: EvalResult["score"]; matched: string[] } {
  const lower = answer.toLowerCase();
  const matched = expected.filter((e) => lower.includes(e.toLowerCase()));
  if (answer.includes("don't know") || answer.includes("no matching") || answer.includes("insufficient")) {
    return { score: "no_answer", matched };
  }
  if (matched.length >= expected.length * 0.5) return { score: "correct", matched };
  if (matched.length > 0) return { score: "partial", matched };
  return { score: "wrong", matched };
}

function summarize(pass: number, results: EvalResult[]): PassSummary {
  const avgMs = results.length ? Math.round(results.reduce((s, r) => s + r.elapsedMs, 0) / results.length) : 0;
  const cov = results.filter((r) => typeof r.cacheCoverage === "number");
  const avgCoverage = cov.length ? Math.round((cov.reduce((s, r) => s + (r.cacheCoverage ?? 0), 0) / cov.length) * 10) / 10 : undefined;
  // Cumulative warm-up state at the end of the pass (from the last query).
  const last = results[results.length - 1];
  return {
    pass,
    total: results.length,
    correct: results.filter((r) => r.score === "correct").length,
    partial: results.filter((r) => r.score === "partial").length,
    wrong: results.filter((r) => r.score === "wrong").length,
    no_answer: results.filter((r) => r.score === "no_answer").length,
    avgMs,
    avgCoverage,
    embedMs: last?.embedMs,
    hitRate: last?.hitRate,
  };
}

export async function runBenchmark(
  askFn: (q: string) => Promise<{ answer: string; sources: unknown[]; llm: string }>,
  options: BenchOptions = {},
): Promise<{ results: EvalResult[]; summary: PassSummary; passes: PassSummary[] }> {
  const questions = options.questions ?? QUESTIONS;
  const passes = Math.max(1, options.passes ?? 1);
  const all: EvalResult[] = [];
  const passSummaries: PassSummary[] = [];

  for (let pass = 1; pass <= passes; pass++) {
    const passResults: EvalResult[] = [];
    for (const { q, expectContains, category } of questions) {
      const start = Date.now();
      let result: EvalResult;
      try {
        const r = await askFn(q);
        const elapsed = Date.now() - start;
        const { score: s, matched } = score(r.answer, expectContains);
        let embedded = 0;
        let total = 0;
        let hits: number | undefined;
        let misses: number | undefined;
        let embedMs: number | undefined;
        let hitRate: number | undefined;
        if (options.coverage) {
          try {
            const cov = await options.coverage();
            embedded = cov.embedded;
            total = cov.total;
            hits = cov.hits;
            misses = cov.misses;
            embedMs = cov.embedMs;
            hitRate = cov.hitRate;
          } catch {
            // coverage is best-effort
          }
        }
        result = {
          question: q,
          category,
          answer: r.answer.slice(0, 500),
          sources: r.sources.length,
          llm: r.llm,
          elapsedMs: elapsed,
          score: s,
          matchedTerms: matched,
          pass,
          embeddedChunks: embedded,
          totalChunks: total,
          cacheCoverage: total > 0 ? Math.round((embedded / total) * 1000) / 10 : 0,
          cacheHits: hits,
          cacheMisses: misses,
          embedMs,
          hitRate,
        };
      } catch (e) {
        result = {
          question: q,
          category,
          answer: String(e),
          sources: 0,
          llm: "error",
          elapsedMs: Date.now() - start,
          score: "wrong",
          matchedTerms: [],
          pass,
        };
      }
      passResults.push(result);
      all.push(result);
      options.onQuery?.(result);
    }
    passSummaries.push(summarize(pass, passResults));
  }

  const summary = summarize(0, all);
  return { results: all, summary, passes: passSummaries };
}
