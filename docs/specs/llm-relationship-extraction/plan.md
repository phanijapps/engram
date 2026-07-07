# Plan: llm-relationship-extraction (RFC 0004 Slice 2 / PHASE59)

pi SDK (`@earendil-works/pi-coding-agent`) + `.env` creds — ollama cloud as a custom
provider in a process-local `models.json`, driven headless via `createAgentSession` +
`prompt` (`noTools: "all"`). Aligns with RFC D2.

## Tasks

### T1 — `.env.example` + load env
- **Tests:** goal-based — backend boots with/without `.env`.
- **Depends on:** none
- **Approach:** Commit `demo/backend/.env.example` (`ENGRAM_LLM_BASE_URL` / `ENGRAM_LLM_API_KEY` / `ENGRAM_LLM_MODEL`). In `main.ts`, call `process.loadEnvFile()` (Node ≥21.7) inside a try/catch so a missing `.env` is fine.

### T2 — `llm.ts` (pi SDK) + pure parser/validator (+ tests)
- **Tests (TDD):** `llm.test.ts` — `parseLLMGraph` on valid JSON, malformed JSON, disallowed kinds (dropped→unknown), dedupe, unresolved endpoints (dropped); `getLLMConfig` absent / placeholder / present.
- **Depends on:** T1
- **Approach:** `getLLMConfig(): {baseUrl,apiKey,model}|null`. `ensureModelsJson(config)` writes a process-local pi `models.json` (`ollama-cloud` provider, `api:"openai-completions"`, `apiKey:"$ENGRAM_LLM_API_KEY"`, model id from env; no literal secret). `extractGraph(text, kind, config)` lazily `import("@earendil-works/pi-coding-agent")`, `AuthStorage.inMemory()` + `ModelRegistry.create(auth, file)` + `find("ollama-cloud", model)`, `createAgentSession({ model, noTools:"all", sessionManager: SessionManager.inMemory(), cwd: os.tmpdir() })`, subscribe to `message_update`/`text_delta`, `prompt(prompt)`, ~30 s timeout via `session.abort()`, ~100 KB cap on streamed text, `dispose()`. Parse via `extractJsonObject` → `parseLLMGraph`. `parseLLMGraph` pure: kind allowlist, non-empty trimmed names, dedupe, endpoint resolution, counts. The lazy import keeps the zero-cred path + tests from loading the harness.

### T3 — `/llm/extract` route + `enhance.ts`
- **Tests:** goal-based — typecheck + build; goal-based plumbing without creds (route returns deterministic + `llm:"unavailable"`); manual live QA with creds.
- **Depends on:** T2
- **Approach:** `enhance.ts` owns `enhanceWithLLM({ text, kind, graphId, scope, source, actor })` → `getLLMConfig()` (null → `unavailable`), else `extractGraph` → build v1 entities/relationships (stable client ids `entity-llm-<sha12>`, LLM provenance `method:"llm_extraction"`, confidence 0.6), persist via `getKnowledgeTransport().putEntity`/`putRelationship` into the deterministic `graphId`, never throw (`error` on failure). New route in `app.ts`: body `{ text, documentKind, scope?, policy?, sourceName?, actor? }`; run `ingestExtract` baseline; `enhanceWithLLM` into its `graphId`; return merged `{ entities, relationships, chunkCount, llm: {entities,relationships} | "unavailable" | "error" }`.

### T4 — `enhance` flag on `/ingest/scan`
- **Tests:** goal-based — typecheck; goal-based without creds (stream carries `llm:"unavailable"`); manual.
- **Depends on:** T3
- **Approach:** Add `enhance?: boolean` to the scan body. When true, after each file's deterministic ingest, call `enhanceWithLLM` into that file's `graphId`; emit merged `entities`/`relationships` + an `llm` status in the progress event and `llmEntities`/`llmRelationships` in the done summary. Off by default (scan stays fast/deterministic).

### T5 — UI toggles
- **Tests:** goal-based — frontend typecheck + build.
- **Depends on:** T3, T4
- **Approach:** `IngestPanel` gains an "LLM enhance" checkbox; when on, POST to `/llm/extract` instead of `/ingest/extract` and render the merged graph + an LLM status hint. `ScanPanel` gains an "enhance" checkbox passed in the scan body; shows an "LLM unavailable (set .env)" hint when the stream reports `llm:"unavailable"` and LLM counts in the summary.

### T6 — Validate + supply-chain + lighter adversarial pass
- **Tests:** backend + frontend typecheck/build; `vitest`; `pnpm audit --prod` (confirm no known vulnerabilities in the `@earendil-works/pi-coding-agent` transitive graph); single-pass review focused on output-validation, credential isolation, and supply-chain.
- **Depends on:** T5

## Out of scope (logged)
- LLM enhance on the whole repo by default (opt-in only here); Q&A (Slice 6); provenance/confidence viz (Slice 4).
