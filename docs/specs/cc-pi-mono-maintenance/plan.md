# Plan: pi-mono LLM Maintenance

Spec: [`spec.md`](./spec.md) · Status: **Ready to implement** · Milestone 2 of the batch.

## Approach

Wire the pi-mono SDK (`@earendil-works/pi-ai`) into the shipped `engram-maintain` module to
drive **LLM belief synthesis** + **LLM-augmented contradiction detection**. The LLM lives
entirely in **TypeScript** (per RFC-0017 "TS = LLM"); **Rust stays LLM-free** — the in-tree
`ReflectionSynthesizer` (deterministic) and rule-based `detect_contradictions` stay as-is, and
the new LLM paths run as **parallel, opt-in TS ops** over the facade that write `reflection-llm`
beliefs + contradictions. They do **not** replace the Rust trait impls.

Decisions (finalized in the spec): default **Anthropic Claude** (`claude-haiku-4-5`),
switchable to **Ollama** via env (`PI_PROVIDER=ollama` + `OLLAMA_BASE_URL`); **extend the
provider surface** (facade → N-API → TS) for belief-list + contradiction ops so maintenance
runs over the agentzero store via the facade (not the standalone `NativeBeliefEngine`).

The milestone is bigger than M1 (it touches Rust). Two foundational tasks (T1 LLM wrapper, T2
surface-parity) are independent and parallelizable; T3/T4 depend on them.

## Task breakdown

### T1 — LLM provider wrapper (TS) — *no deps*
**Files**: `packages/runtime/src/maintenance/llm.ts` (new); add `@earendil-works/pi-ai` to
`packages/runtime/package.json` deps; rebuild dist.
- A thin `LlmProvider` interface: `complete(systemPrompt, userText, tools) → { toolCalls }`
  wrapping pi-mono's `builtinModels()` → `getModel(provider, modelId)` → `complete(model,
  context, {apiKey})`.
- Config from env: `PI_PROVIDER` (`anthropic`|`ollama`|`openai`, default `anthropic`),
  `PI_MODEL` (default `claude-haiku-4-5`), provider key (`ANTHROPIC_API_KEY` / `OPENAI_API_KEY`)
  / `OLLAMA_BASE_URL`. Auth resolves via pi-mono's provider env or explicit key.
- **`PI_DRY_RUN=1`** mode: returns canned fixture responses (deterministic, token-free tests).
- **Acc**: unit test (`maintenance/llm.test.ts`) exercises `PI_DRY_RUN` end-to-end (prompt in →
  toolCalls out); real-provider path is smoke-only (manual, needs a key). `pnpm --filter
  @engram/runtime typecheck + test` green.

### T2 — Surface parity: belief-list + contradiction on the provider — *no deps (Rust)*
**Files**: `core/belief/src/lib.rs` (if `list_beliefs` is missing on `BeliefRepository`),
`core/integration/src/provider.rs` (accessor/wiring), `bindings/node/src/provider.rs`
(`NativeBeliefsApi`), `packages/node/src/{binding,provider}.ts`.
- Expose on the **provider** surface (mirrors `listMemoriesPaged` / `communityOverview`):
  `listBeliefsJson(scope, …)`, `detectContradictionsJson(scope/beliefs)`,
  `putContradictionJson`, `getContradictionJson`, `resolveContradictionJson`, `listContradictionsJson`.
- These exist on the **standalone** `NativeBeliefEngine` today; the provider's
  `NativeBeliefsApi` only has get/upsert/retract/listStale. Verify whether the provider's held
  `BeliefRepository` already has `list_beliefs` (active) — if not, add the port method
  (additive, no contract break).
- TS: matching methods on `NativeProviderTransport` + `NativeProviderBinding` types.
- **Acc**: `cargo check --workspace` + `cargo test -p engram-integration -p bindings/node`
  green; **engine-neutrality lint** clean (no engine type in core/integration/binding);
  **surface-parity lint** clean (facade ↔ binding). Add a TS test asserting the new methods
  exist + round-trip over a fixture.

### T3 — LLM reflection op (engram-maintain) — *depends on T1*
**Files**: `packages/runtime/src/maintenance/cli.ts` + `reflect.ts` (new).
- New op (e.g. `engram-maintain --reflect-llm` or a `reflect-llm` subcommand): reads active
  memories for scope via `listMemoriesPaged`, builds a pi-mono `Context` (systemPrompt =
  belief-synthesis instructions + a `record_belief` TypeBox tool:
  `{ subject, predicate, object, confidence, sourceMemoryIds }`), `complete()`, collects
  `toolCall` blocks → `Belief` records, writes each via `beliefPut` with
  `provenance.method = "reflection-llm"`, `provenance.source = "pi-mono"`.
- Runs standalone (the CLI/MCP); does NOT alter the Rust `consolidate()` path
  (deterministic-by-default). Composes as an opt-in step.
- **Acc**: `PI_DRY_RUN` test — given fixture memories, the op emits N `reflection-llm` beliefs
  via the (stubbed) transport's `beliefPut`; provenance is correct. `pnpm test` green.

### T4 — LLM contradiction op — *depends on T1 + T2*
**Files**: `packages/runtime/src/maintenance/cli.ts` + `contradict.ts` (new).
- Op: reads active beliefs via `listBeliefs` (T2), batches → pi-mono `find_contradiction` tool
  (`{ subject, beliefIds[], kind, severity, explanation }`), writes via `putContradiction` (T2).
- The rule-based detector stays as a fast pre-filter; LLM judges semantic conflicts it misses.
- **Acc**: `PI_DRY_RUN` test — given fixture beliefs with a same-`subject.key` dup AND a
  different-key semantic conflict, the op records ≥1 contradiction the rule-based pass would
  miss. Eval-fixture style (per `engram-eval` skill).

### T5 — MCP maintenance tools — *depends on T2/T3/T4*
**Files**: `packages/runtime/src/mcp/tools.ts`.
- Add `maintenance_run` (reflect-llm + optional contradict), `contradiction_detect`,
  `contradiction_list`, `belief_list` — all routing through `NativeProviderTransport` (ADR-0022:
  the agent surface).
- **Acc**: `mcp.test.ts` — `tools/list` includes the new tools; a `maintenance_run` call (dry-run
  transport) returns a result shape.

### T6 — Capability flip + docs — *depends on T2*
**Files**: `core/integration/src/sqlite/bootstrap.rs` (call `.contradiction(...)` /
`.maintenance(...)` setters + a provider handle), `core/integration/src/capability.rs` (update
the pinned regression test that asserts these stay `FeatureDisabled`).
- Flip `contradiction` + `maintenance` from `FeatureDisabled` → `Supported` once T2 wires them.
- **Acc**: `/api/health` capability report shows both `Supported`; the updated regression test
  passes; `cargo test` green.

## Gate sequence (per work-loop)

1. **Rust** (after T2/T6): `cargo fmt --all && cargo check --workspace && cargo test -p
   engram-belief -p engram-integration -p bindings/node`; `.codex/hooks/check-engine-neutrality.sh`
   + `check-surface-parity.sh` + `check-contracts.sh` + `check-docs.sh`.
2. **TS**: `pnpm --filter @engram/runtime run build` → `typecheck` → `test`
   (incl. `PI_DRY_RUN` reflection + contradiction tests) → `mcp.test.ts`.
3. **End-to-end** (manual): `engram-maintain --reflect-llm` over the agentzero store produces
   `reflection-llm` beliefs; `contradiction_detect` finds a semantic conflict; MCP `tools/list`
   shows the new tools; `/api/health` flips `contradiction` + `maintenance` to `Supported`.
4. **Review**: full adversarial pass (this milestone touches Rust + the binding + a capability
   flip + a new dependency — risk triggers fire: security boundary [LLM/secret key handling,
   prompt-injection from stored memory text], surface parity, engine neutrality).

## Risks / caveats

- **Surface-parity Rust work is the riskiest task** (touches `core/belief`, `engram-integration`,
  `bindings/node`) — gated by neutrality + parity lints + cargo tests. Additive only (no
  contract break); run `contracts:generate` if any generated type moves.
- **`list_beliefs` may not exist** on the provider's `BeliefRepository` (only `list_stale` +
  `list_contradictions` confirmed) → T2 may add a port method. Flagged, additive.
- **Secret handling**: the LLM API key is read from env in the TS module (never logged, never
  sent to the browser). Memory text is sent to the LLM as prompt content — note the
  prompt-injection surface (a malicious memory could try to steer the model); mitigate via
  structured tool-output (don't let the model execute, only emit toolCalls we parse).
- **pi-mono is a new dependency** — pin a version; confirm it loads under vitest (it's pure TS,
  unlike the native binding, so low risk).
- **Rust stays LLM-free** — no LLM type/config in `engram-domain`/`engram-integration`/
  `bindings/node`; `check-engine-neutrality.sh` is the gate. The `BeliefSynthesizer` Rust trait
  is NOT swapped (the LLM path is TS-side + writes via `beliefPut`).
- **Token cost** — `PI_DRY_RUN` for tests; real runs is manual/opt-in. Default model is the cheap
  `claude-haiku-4-5`.
