# Eval: 8-Question Code-Intelligence Suite

A small, fast smoke suite for the **lazy-embeddings hypothesis** — does the
knowledge graph alone (entities + relationships + chunk text + agentic Q&A,
no vector embeddings) ground code-intelligence answers? Eight questions is a
pilot: enough to see a signal, small enough to run in under two minutes.

- **Target repo:** [microsoft/terminal](https://github.com/microsoft/terminal)
  (e58bd4bdab, main)
- **Index:** tree-sitter AST (C/C++/C#), no embeddings
- **LLM:** `gemma4:31b-cloud` via ollama cloud (pi SDK), agentic Q&A with
  `search_entities` / `get_neighbors` / `get_code` tools
- **Cost:** ~8 LLM turns, ~70 s wall-clock

This is the exact question set `demo/backend/src/bench.ts` runs.

## Categories

| Category | Count | Posture |
| --- | --- | --- |
| entity_lookup | 2 | Find/explain a named entity |
| concept | 2 | Explain how a mechanism works |
| relationship | 2 | How two entities relate |
| structural | 2 | List main components of an area |
| call_graph | 1 | Trace a call chain |

## Scoring rubric

Keyword-over-substring scoring (matches `bench.ts:score`):

1. If the answer contains a refusal marker (`don't know`, `no matching`,
   `insufficient`) → **no_answer**.
2. Else, count how many `expectContains` keywords appear (case-insensitive
   substring) in the answer.
3. `matched >= 50% of expected` → **correct**.
4. `0 < matched < 50%` → **partial**.
5. `matched == 0` → **wrong**.

Keyword scoring is intentionally lenient — it gates the graph grounding
returning the right neighborhood, not prose quality. A factual answer that
happens to omit the expected noun undercounts; rerun with a larger model or
read the per-question answer for the real signal.

## Question set

| # | Category | Question | Expected keywords |
| --- | --- | --- | --- |
| 1 | entity_lookup | What does the TerminalHandle class do? | `TerminalHandle`, `handle` |
| 2 | concept | How does text rendering work in the terminal? | `render`, `text` |
| 3 | relationship | What is the relationship between Terminal and TerminalConnection? | `Terminal`, `Connection` |
| 4 | structural | List the main classes in the renderer module | `renderer`, `class` |
| 5 | entity_lookup | What does the Settings class manage? | `Settings` |
| 6 | concept | How are keyboard shortcuts handled? | `key`, `shortcut` |
| 7 | call_graph | What is the call chain for writing text to the screen? | `write`, `screen`, `render` |
| 8 | structural | What are the main components of the terminal architecture? | `component`, `module` |

## How to run

```bash
# 1. Index the target repo (clean DB, no manifest cache)
#    via the dashboard "Index" dialog, force re-index, or:
#    POST /ingest/jobs { path: "<terminal checkout>", force: true }

# 2. Run the suite (calls /qa/ask for each question)
pnpm --filter demo-backend exec tsx src/bench.ts
```

`runBenchmark(askFn)` returns per-question `{ score, matchedTerms, sources,
elapsedMs }` plus a summary. The HTTP `/bench` route exposes the same over the
API.

## Last known results

Run on the terminal index above (full numbers in
[PERFORMANCE.md](./PERFORMANCE.md)):

| Metric | Value |
| --- | --- |
| Correct | 5 (62.5%) |
| Partial | 1 (12.5%) |
| Wrong | 1 (12.5%) |
| No answer | 1 (12.5%) |
| Avg time | 9.2 s |

Correct + partial = **75%**. The one `wrong` was a formatting glitch (raw
JSON tool-call leaking into prose), not a factual error — see the limitations
section in PERFORMANCE.md.

## Limitations

- **Small N.** Eight questions is a smoke test, not a statistically stable
  measurement. For trend tracking use the
  [50-question suite](./eval-50-question.md).
- **Keyword-scored.** A well-grounded answer phrased without the expected
  noun scores as partial/wrong. Read the answer text before declaring
  regression.
- **Single repo.** All questions target one C++ codebase. Cross-language and
  cross-repo behavior isn't exercised here.

## See also

- [50-question suite](./eval-50-question.md) — the larger, trend-tracking eval
- [Performance benchmark](./PERFORMANCE.md) — methodology + results
- [Benchmark source](../../demo/backend/src/bench.ts) — the runnable suite
- [Q&A logic](../../demo/backend/src/qa.ts)
