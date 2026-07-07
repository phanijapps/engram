# Spec: qa-over-knowledge (RFC 0004 Slice 6 / PHASE63)

- **Status:** Shipped
- **Shape:** mixed (service + ui)
- **Constrained by:** RFC-0004 D6/D7 + the Security & data-custody controls (LLM-output validation, call bounds, credential isolation); reuses the Slice 2 pi SDK path
- **Contract:** none (new demo-only route; transport stays `unknown`-typed)

## Objective

Answer natural-language questions over the demo's knowledge + memory, with grounded citations. `/qa/ask { question, scope }` retrieves memory (keyword retrieve) + beliefs (query-term filter), builds a grounded context, and — when `.env` LLM creds are present — drives ollama cloud (gemma4:31b-cloud) via the pi SDK to synthesize an answer that cites the retrieved records. Without creds, it returns the retrieved evidence as a structured summary (deterministic fallback). The answer + sources (kind, id, text, source) are surfaced in a QAPanel. Retrieval stays deterministic; only synthesis is LLM-backed.

## Decision (aligns with RFC D6/D7)

Reuse the Slice 2 pi SDK integration. Extract a shared `runLLM(systemPrompt, userText, config)` helper from `llm.ts` (the ensureModelsJson + createAgentSession + subscribe + prompt + capped/timeout logic) so both extraction and Q&A share one bounded, output-validated LLM path. `qa.ts` owns grounding + prompt construction + answer shaping; it never trusts LLM text as data (the answer is rendered as text; sources come from the deterministic retrieval, not the LLM). Deterministic fallback when creds are absent.

## Assumptions

- Technical: `/memory/retrieve` returns memory records (content + provenance) for `{ query, scope, modes:["keyword"], limit, budget }`. (verified — App.tsx)
- Technical: `/belief/list` returns beliefs (subject + content + provenance). (verified — Slice 5)
- Technical: the pi SDK drives ollama cloud via the Slice 2 models.json path; `runLLM` reuses it. (verified — Slice 2)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Ground only in deterministic retrieval (memory + beliefs); the LLM synthesizes prose, never invents cited records.
- Bound the LLM call (reuse the ~30s timeout, ~100KB cap, input truncation from `runLLM`).
- Read the API key server-side only; reference via `$ENGRAM_LLM_API_KEY`; never send to the frontend or log it.
- Missing creds → deterministic evidence summary, never an error traceback.

**Ask first**
- LLM-grounded knowledge-graph keyword search (no entity-search port today); streaming answers.

**Never do**
- Trust LLM output as cited sources; expose the key to the browser; duplicate the pi session plumbing; change Rust/contracts.

## Testing Strategy

- **TDD (unit):** `qa.ts` grounding — given memory records + beliefs, the deterministic fallback builds the expected evidence summary + sources; query-term filtering drops non-matching beliefs (no network, no pi).
- **Goal-based (build):** backend + frontend typecheck/build; `vitest`.
- **Goal-based (plumbing):** `/qa/ask` without creds returns evidence + `llm:"unavailable"`; with creds, manual live QA.
- **Manual QA:** write a memory + belief, ask a question, confirm the answer cites them.

## Acceptance Criteria

- [x] `/qa/ask` grounds in memory + beliefs, synthesizes via the pi SDK when creds are present, and returns `{ answer, sources, llm }`; without creds it returns a deterministic evidence summary + `llm:"unavailable"`.
- [x] `runLLM` is shared by extraction + Q&A (no duplicated pi plumbing); calls are bounded; the key stays server-side.
- [x] Sources come from deterministic retrieval, never from LLM-invented text.
- [x] A QAPanel asks a question and renders the answer + cited sources.
- [x] Backend + frontend typecheck/build + unit tests pass; no Rust/contract change.
