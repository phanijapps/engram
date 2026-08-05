# Spec: pi-mono LLM Maintenance

Status: **Ready to plan**
Related: [`ts-runtime-maintenance`](../ts-runtime-maintenance/spec.md) (shipped
`engram-maintain` CLI, deterministic "Light mode"), RFC-0017 (TS = LLM), ADR-0022
(surface parity + engine neutrality)

## Context

engram's maintenance ships only **deterministic baselines** because there is **no LLM
provider in-tree** (only an `EmbeddingProvider`). Reflection concatenates memory text into
one belief; contradiction detection is rule-based. Wire the **pi-mono SDK**
(`@earendil-works/pi-ai`, the multi-provider LLM client at pi.dev) into the shipped
`engram-maintain` module to drive real **belief synthesis** + **LLM-augmented contradiction
detection**, expose via facade + MCP, and keep Rust LLM-free. Milestone 2 of the next batch
(independent of ingest control).

## Decisions (finalized)

- **Default LLM**: **Anthropic Claude** (`claude-haiku-4-5` default; `claude-opus-5` for
  quality). **Switchable to a local model via env** — `PI_PROVIDER=ollama` +
  `OLLAMA_BASE_URL` + model, using pi-mono's Ollama provider. (`openai` also available.)
- **LLM config is TS/env-only** (`PI_PROVIDER`, `PI_MODEL`, `ANTHROPIC_API_KEY` /
  `OPENAI_API_KEY` / `OLLAMA_BASE_URL`). **Rust stays LLM-free** (engine neutrality).
- **Surface gap**: **extend the provider surface** (facade → N-API → TS) so maintenance
  runs over the agentzero store via the facade — NOT the standalone `NativeBeliefEngine`.

## Current state (grounded)

- **`BeliefSynthesizer`** (`core/belief/src/lib.rs:74`) is the LLM seam; in-tree
  `ReflectionSynthesizer` (`core/reflection/src/synthesizer.rs:18`) is **deterministic**
  (concat, confidence 0.5). Comments defer the LLM impl behind the same trait.
- **`ContradictionDetector`** — only impl is `SqlBeliefStore::detect_contradictions`
  (`adapters/sqlite/src/belief/detector.rs:22`), **rule-based** (groups by `subject.key`).
  Embedding-similarity helpers exist, unused.
- **`consolidate()` wired end-to-end**: facade → `NativeProvider.consolidateJson`
  (`bindings/node/src/provider.rs:174`) → TS `consolidate`. Capability `consolidation = Supported`.
- **Surface gap**: contradiction CRUD + detect exist **only on the standalone**
  `NativeBeliefEngine` (`bindings/node/src/belief.rs`) + `NativeBeliefTransport`
  (`packages/node/src/transport.ts:327`). The provider `NativeBeliefsApi` has only
  get/upsert/retract/listStale — **no list-beliefs, no contradiction ops, no detect**.
- **MCP** (`packages/runtime/src/mcp/tools.ts`) has **no maintenance tools**.
- **`contradiction` + `maintenance` are `FeatureDisabled`** (capability slots reserved,
  never flipped in `bootstrap.rs:591-608`; pinned by a regression test).
- **`packages/runtime/src/maintenance/`** exists (`engram-maintain` CLI, Light mode — runs
  `consolidate` over the facade).
- **pi-mono** (`@earendil-works/pi-ai`): `builtinModels()` → `getModel(provider, modelId)`
  → `complete(model, context, {apiKey})` / `stream(...)`. `Context = { systemPrompt,
  messages:[{role,content,timestamp}], tools:[{name,description,parameters:TypeBox}] }`.
  Multi-provider incl. anthropic + ollama. Auth via provider env or explicit key.

## Design

### LLM provider (TS, inside the maintenance module)
- A thin `LlmProvider` interface in `packages/runtime/src/maintenance/` wrapping pi-mono:
  `complete(context) → response`. Config from env: `PI_PROVIDER` (`anthropic`|`ollama`|
  `openai`), `PI_MODEL`, provider key / `OLLAMA_BASE_URL`. **Default
  anthropic/claude-haiku-4-5**; Ollama via pi-mono's Ollama provider + local base url.
- **`PI_DRY_RUN`** mode (returns canned/fixture responses) so tests are deterministic and
  token-free.

### Reflection (LLM)
1. Read active memories for scope via `listMemoriesPaged` (facade).
2. Build pi-mono `Context`: systemPrompt = belief-synthesis instructions; memories as the
   user message; a `record_belief` tool (TypeBox schema
   `{ subject, predicate, object, confidence, sourceMemoryIds[] }`).
3. `complete(model, context)` → collect `toolCall` blocks → `Belief` records.
4. Write via facade `beliefPut` with `provenance.method = "reflection-llm"`,
   `provenance.source = "pi-mono"`.
5. Runs as a **standalone maintenance op** (`engram-maintain` CLI / a `maintenance_run`)
   that ALSO composes into `consolidate()` (consolidate stays deterministic-by-default;
   opt-in LLM flag).

### Contradiction (LLM-augmented)
1. Read active beliefs for scope (needs **list-beliefs on the provider** — surface-parity
   work below).
2. Batch → pi-mono with a `find_contradiction` tool
   `{ subject, beliefIds[], kind, severity, explanation }`.
3. Write via the provider contradiction API (surface-parity work).
4. Rule-based detector stays as a **fast pre-filter**; LLM judges semantic conflicts it misses.

### Surface-parity work (provider extension — load-bearing)
Thread belief-list + contradiction through the provider so maintenance works over agentzero
via the facade:
- **Rust**: expose list-beliefs + contradiction detect/put/get/resolve through the
  provider's `BeliefRepository` + `ContradictionDetector` (traits exist; wire them on the
  provider handle).
- **N-API** (`bindings/node`): add `listBeliefsJson`, `detectContradictionsJson`,
  `putContradictionJson`, `getContradictionJson`, `resolveContradictionJson` to
  `NativeBeliefsApi`.
- **TS** (`packages/node/src/provider.ts`): matching methods on `NativeProviderTransport`.
- Mirrors `listMemoriesPaged` / `communityOverview` threading (the surface-parity precedent).

### MCP tools (`packages/runtime/src/mcp/tools.ts`)
- `maintenance_run` (consolidate + LLM reflect), `contradiction_detect`,
  `contradiction_list`, `belief_list`.

### Capability flip
- Once wired, flip `contradiction` + `maintenance` from `FeatureDisabled` in `bootstrap.rs`
  (add setters + a provider handle) + update the pinned regression test.

## Acceptance criteria

1. A maintenance run over agentzero produces **LLM-synthesized beliefs**
   (`provenance.method = "reflection-llm"`), not the concat baseline.
2. Contradiction detection surfaces **semantic** conflicts the rule-based detector misses
   (eval fixture: same-`subject.key` dup + different-key semantic conflict; only the latter
   is LLM-only).
3. Maintenance reachable from **both** the facade (TS methods) **and** MCP
   (`maintenance_run` etc.) — surface parity; reflected in `CapabilityReport`.
4. `contradiction` + `maintenance` flip to **`Supported`**.
5. **Default LLM = Claude** (`anthropic`/`claude-haiku-4-5`); switching to **Ollama via env**
   works (`PI_PROVIDER=ollama`). `PI_DRY_RUN` works for tests.
6. **Rust stays LLM-free**: no LLM type in `engram-domain`/`engram-integration`/`bindings`;
   engine-neutrality + surface-parity lints pass; LLM config is TS/env-only.

## Verification

- `engram-maintain` runs reflection + contradiction over agentzero; new `reflection-llm`
  beliefs + semantic contradictions appear.
- MCP `tools/list` includes the new tools; a `maintenance_run` call returns results.
- Capability report flips `contradiction` + `maintenance` to `Supported`.
- `cargo` engine-neutrality + surface-parity lints pass; `pnpm typecheck` + `pnpm test` green.
- `PI_DRY_RUN` deterministic test for the synthesis + contradiction prompts.
