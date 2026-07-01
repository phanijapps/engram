// Benchmark eval suite for the lazy-embeddings hypothesis.
// Runs a set of Q&A questions against the indexed knowledge graph.
// No embeddings — the graph (entities + relationships + agentic Q&A) is the
// baseline. The hypothesis: lazy embeddings (at query time) aren't needed when
// the knowledge graph + chunk text provide sufficient grounding.

const QUESTIONS = [
  {
    q: "What does the TerminalHandle class do?",
    expectContains: ["TerminalHandle", "handle"],
    category: "entity_lookup",
  },
  {
    q: "How does text rendering work in the terminal?",
    expectContains: ["render", "text"],
    category: "concept",
  },
  {
    q: "What is the relationship between Terminal and TerminalConnection?",
    expectContains: ["Terminal", "Connection"],
    category: "relationship",
  },
  {
    q: "List the main classes in the renderer module",
    expectContains: ["renderer", "class"],
    category: "structural",
  },
  {
    q: "What does the Settings class manage?",
    expectContains: ["Settings"],
    category: "entity_lookup",
  },
  {
    q: "How are keyboard shortcuts handled?",
    expectContains: ["key", "shortcut"],
    category: "concept",
  },
  {
    q: "What is the call chain for writing text to the screen?",
    expectContains: ["write", "screen", "render"],
    category: "call_graph",
  },
  {
    q: "What are the main components of the terminal architecture?",
    expectContains: ["component", "module"],
    category: "structural",
  },
];

type EvalResult = {
  question: string;
  category: string;
  answer: string;
  sources: number;
  llm: string;
  elapsedMs: number;
  score: "correct" | "partial" | "wrong" | "no_answer";
  matchedTerms: string[];
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

export async function runBenchmark(
  askFn: (q: string) => Promise<{ answer: string; sources: unknown[]; llm: string }>,
): Promise<{
  results: EvalResult[];
  summary: {
    total: number;
    correct: number;
    partial: number;
    wrong: number;
    no_answer: number;
    avgMs: number;
    totalSources: number;
  };
}> {
  const results: EvalResult[] = [];
  for (const { q, expectContains, category } of QUESTIONS) {
    const start = Date.now();
    try {
      const result = await askFn(q);
      const elapsed = Date.now() - start;
      const { score: s, matched } = score(result.answer, expectContains);
      results.push({
        question: q,
        category,
        answer: result.answer.slice(0, 500),
        sources: result.sources.length,
        llm: result.llm,
        elapsedMs: elapsed,
        score: s,
        matchedTerms: matched,
      });
    } catch (e) {
      results.push({
        question: q,
        category,
        answer: String(e),
        sources: 0,
        llm: "error",
        elapsedMs: Date.now() - start,
        score: "wrong",
        matchedTerms: [],
      });
    }
  }
  const summary = {
    total: results.length,
    correct: results.filter((r) => r.score === "correct").length,
    partial: results.filter((r) => r.score === "partial").length,
    wrong: results.filter((r) => r.score === "wrong").length,
    no_answer: results.filter((r) => r.score === "no_answer").length,
    avgMs: Math.round(results.reduce((s, r) => s + r.elapsedMs, 0) / results.length),
    totalSources: results.reduce((s, r) => s + r.sources, 0),
  };
  return { results, summary };
}

export { QUESTIONS };
