# Plan: qa-over-knowledge (RFC 0004 Slice 6 / PHASE63)

Q&A over knowledge + memory, reusing the Slice 2 pi SDK path. Single commit.

## Tasks

### T1 — Extract shared `runLLM` helper from `llm.ts`
- **Tests:** goal-based — existing `llm.test.ts` still passes (parseLLMGraph/getLLMConfig unaffected).
- **Depends on:** none
- **Approach:** In `llm.ts`, pull the pi session logic out of `extractGraph` into `export async function runLLM(systemPrompt, userText, config): Promise<string>` — `ensureModelsJson` + lazy `import("@earendil-works/pi-coding-agent")` + `createAgentSession({noTools:"all"})` + subscribe `text_delta` + ~30s abort + ~100KB cap + redacted-error; returns the collected text (throws on no content). `extractGraph` becomes: `runLLM(extractionPrompt(kind), truncated, config)` → `parseLLMGraph(JSON.parse(extractJsonObject(...)))`.

### T2 — `qa.ts` grounding + synthesis (+ tests)
- **Tests (TDD):** `qa.test.ts` — `buildEvidence(question, memories, beliefs)` pure: returns `{context, sources}`; query-term filter on belief subject/content; deterministic fallback answer string.
- **Depends on:** T1
- **Approach:** `demo/backend/src/qa.ts`. `buildEvidence(question, memories, beliefs)` pure: filter beliefs by query-term overlap (subject.key + content), take memory records, build a `context` string + `sources: [{kind:"memory"|"belief", id, text, source}]`. `answerQuestion(question, scope, opts)`: retrieve memory (`getTransport().retrieve`) + list beliefs (`getBeliefTransport().listBeliefs`) → `buildEvidence` → if `getLLMConfig()`: `runLLM(qaSystemPrompt, context, config)` → answer = trimmed text; else answer = deterministic evidence summary. Return `{answer, sources, llm}`.

### T3 — `/qa/ask` route
- **Tests:** goal-based — typecheck; curl without creds → evidence + `unavailable`.
- **Depends on:** T2
- **Approach:** `app.ts`: `POST /qa/ask { question, scope }` → `answerQuestion` → `c.json`.

### T4 — `QAPanel`
- **Tests:** goal-based — frontend typecheck/build.
- **Depends on:** T3
- **Approach:** `demo/frontend/src/QAPanel.tsx`: question input + Ask → POST `/qa/ask` → render `answer` (prose) + `sources` (kind badge, id, text, source link) + LLM status hint (`unavailable` → "set .env"). Wire into App.tsx.

### T5 — Validate + lighter adversarial pass
- **Tests:** backend + frontend typecheck/build; `vitest`; single-pass review focused on grounding correctness, no-LLM-invented-sources, credential isolation, + the `runLLM` refactor not regressing extraction.
- **Depends on:** T4

## Out of scope (logged)
- LLM-grounded knowledge-graph keyword search (no entity-search port); streaming answers; multi-turn.
