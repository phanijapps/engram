# Spec: llm-relationship-extraction (RFC 0004 Slice 2 / PHASE59)

- **Status:** Shipped
- **Shape:** mixed (service + ui)
- **Constrained by:** RFC-0004 D2/D3 + the Security & data-custody controls (LLM-output validation, call bounds, credential isolation, supply-chain); ADR-0007 (no binding change)
- **Contract:** none (new demo-only routes)

## Objective

An LLM extracts entities + relationships from ingested text to enrich the knowledge
graph, on top of the always-on deterministic `GraphExtractor`. The model is reached
through the **pi SDK** (`@earendil-works/pi-coding-agent`): ollama cloud is registered
as a custom provider/model in a process-local pi `models.json` generated from
`demo/backend/.env` (`base_url`, `api_key`, `model`), and the model is driven headless
via `createAgentSession` + `prompt` with no tools. The deterministic path is the
**zero-credential default**; an "LLM enhance" toggle in the UI turns on LLM extraction
when credentials are present. Extraction output is validated before it writes the graph;
calls are bounded; the API key never reaches the frontend and is never written to disk.

## Decision (aligns with RFC D2)

RFC D2 named the pi SDK as the primary client. This slice implements exactly that:
the pi SDK is the client, ollama cloud is its custom provider (fed by `.env`), and the
deterministic `GraphExtractor` stays the zero-credential baseline. The SDK is imported
lazily inside `extractGraph` so the zero-credential path and the unit tests never load
the agent harness.

## Assumptions

- Technical: ollama cloud (the user's provider) speaks the OpenAI Completions API; `base_url` includes the version segment (e.g. `https://host/v1`). (user-specified; to confirm at runtime against the user's `.env`)
- Technical: pi registers an OpenAI-compatible custom provider via `models.json` with `api: "openai-completions"`; the provider `apiKey` references `$ENGRAM_LLM_API_KEY`, which pi resolves from the environment at request time. (verified — `pi.dev/docs/latest/models`, and headless probe: `modelRegistry.find("ollama-cloud", …)` resolves)
- Technical: `putEntity`/`putRelationship` accept a client-provided `id` and link relationships by it; LLM entities are persisted into the deterministic graph's `graphId`. (verified — round-trip probe + `neighbors()` linkage)
- Technical: the SDK is ESM-only; `demo/backend` is ESM (`"type":"module"`); a lazy dynamic `import()` keeps the harness out of the test process. (verified — 31 unit tests pass without loading pi)
- Technical: `process.loadEnvFile()` reads `.env`. (verified — Node ≥21.7)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Run the deterministic extractor first (zero-cred baseline); LLM is an additive "enhance".
- Validate LLM output against a strict schema (entity kinds allowlist, non-empty names, relationship endpoints resolve to extracted entity names, size bounds) before any graph write.
- Bound every LLM call (~30 s timeout via `session.abort()`, ~100 KB response cap enforced on the streamed text, input truncated).
- Read the API key server-side only; reference it via `$ENGRAM_LLM_API_KEY` in the generated `models.json` (never embed the literal); never send it to the frontend or log it; missing creds → silent deterministic-only path.
- Pin the pi SDK dependency and run `pnpm audit` over the added transitive graph.

**Ask first**
- Streaming/SSE for LLM output beyond the in-process `text_delta` accumulation; switching providers per call.
- Calling the LLM from anywhere except `demo/backend`.

**Never do**
- Write unvalidated LLM output to the graph.
- Put the LLM client in Rust; expose the API key to the browser; embed the literal key in any file.
- Change v1 contracts or Rust; or let the agent run tools during extraction (`noTools: "all"`).

## Testing Strategy

- **TDD (unit):** the pure `parseLLMGraph` / validation logic — valid JSON, malformed JSON, disallowed kinds, oversized output, unresolved relationship endpoints — tested with `vitest` (no network, no pi load).
- **Goal-based (build):** backend + frontend `typecheck` + `build`.
- **Goal-based (plumbing, no creds):** the pi SDK loads headless and resolves the `ollama-cloud` model from the generated `models.json`; the `/llm/extract` + `/ingest/scan` routes return deterministic-only + `llm:"unavailable"` when creds are absent.
- **Manual QA (live, needs creds):** with a real `.env`, ingest a prose snippet with "enhance" on, confirm LLM entities/relationships appear alongside the deterministic graph.

## Acceptance Criteria

- [x] The pi SDK reads `ENGRAM_LLM_BASE_URL` / `ENGRAM_LLM_API_KEY` / `ENGRAM_LLM_MODEL` from `.env` to register `ollama-cloud` as a custom provider; `demo/backend/.env.example` is committed (no secrets); the literal key is never written to disk.
- [x] `parseLLMGraph` validates output (kind allowlist, non-empty names, endpoint resolution, size bounds) and is unit-tested for valid + malformed + adversarial cases.
- [x] `/llm/extract` runs deterministic ingest then (if creds) LLM-extracts via the pi SDK, validates, persists via `putEntity`/`putRelationship`, and returns the merged graph + per-source counts; without creds it returns deterministic-only + `llm:"unavailable"`.
- [x] LLM calls are bounded (~30 s timeout via abort, ~100 KB streamed-response cap, input truncation); the API key is server-side only, referenced (not embedded), and never logged.
- [x] The IngestPanel "LLM enhance" toggle calls `/llm/extract` when on; the ScanPanel has an "enhance" option that LLM-extracts per file when on + creds present (off by default).
- [x] The pi SDK is pinned; backend + frontend typecheck/build pass; unit tests pass without loading the harness.
