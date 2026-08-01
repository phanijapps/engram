# Plan: TypeScript provider facade over NativeProvider

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done <!-- Drafting | Executing | Done -->

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

A vertical slice that makes the held `NativeProvider` consumable from TypeScript
and adds the one missing operation (scan). Four moves, in dependency order:

1. **(Rust)** Promote the scan fan-in (`KnowledgeRepoGraph`) from the MCP into
   `engram-ingest`, the crate that owns `scan_repository` and its trait bound.
2. **(Rust)** Add `scanRepositoryJson` to `NativeProvider`, mirroring the
   `consolidateJson` precedent — pull knowledge+graph handles, build the fan-in,
   run `scan_repository`, return the summary.
3. **(TS binding)** Add a hand-written `NativeProviderBinding` interface to
   `@engram/node`'s `binding.ts` (+ the handle-proxy sub-interfaces) and register
   it on `NativeBinding`.
4. **(TS facade)** A thin `createNativeProviderTransport` facade in `@engram/node`
   that dispatches recall/write/scan/consolidate/putEntity/batchIngest to
   `NativeProvider` with typed encode/decode.

The riskiest part is the boundary call (facade in `@engram/node` vs a new
package) and keeping the Rust scan path testable without the napi wrapper — both
are pressure-tested by the pre-EXECUTE adversarial review and addressed in T2's
approach (extract the scan body into a testable helper).

## Constraints

- **RFC-0017 Phase A** — this is the keystone adoption slice; the held provider +
  consolidation execution already exist (do not rebind them).
- **ADR-0022** — engine neutrality + surface parity. The `check-surface-parity.sh`
  gate must stay green; the facade must not introduce engine types.
- **RFC-0015 D4** — no LLM in the server/facade; extraction stays agent-side.
- **AGENTS.md** — the binding package is transport over Rust, not a second
  implementation; no god modules.

## Construction tests

Most construction tests live under **Tasks** below (per-task `Tests:`
subsections). This top-level section is only for cross-cutting tests that
span tasks.

**Integration tests:**
- None beyond per-task tests. The Rust scan check (T2) is a same-module unit
  test over a private helper; the facade dispatch check (T4) is a `vitest` unit
  test over a mock binding. The real cross-cutting integration is the T5 smoke
  below, which exercises the built addon end-to-end.

**Manual verification:**
- The T5 smoke script (construct → write → scan → consolidate **execution** →
  recall), recording observed JSON.

## Design (LLD)

### Design decisions

- **Facade lives in `@engram/node`**, not a new package — mirrors the existing
  `createNative*Transport` pattern; the facade is transport over Rust. A new
  `@engram/runtime` package is deferred until the facade grows non-transport
  logic (scheduling, retries). Traces to: AC1, AC4.
- **Scan fan-in promoted to `engram-ingest`** — that crate owns `scan_repository`
  and its `KnowledgeRepository + KnowledgeGraphRepository` bound, so the fan-in
  type belongs there; the MCP and the binding both import it. Traces to: AC5.
- **`scanRepositoryJson` follows the `consolidateJson` shape** — a direct method
  on `NativeProvider` (not a handle proxy), because scan composes two handles
  internally. Unlike `consolidateJson`, scan is **invisible to the parity lint**
  (`check-surface-parity.sh` iterates facade `require_*` only; scan is a free
  function in `engram-ingest` with no facade `require_*`), so no SPECIAL_CASES
  entry is needed. Traces to: AC2.
- **Rejected: feeding the lexical lane + embed-on-scan inside `scanRepositoryJson`.**
  Those are Module-1 (Phase C) concerns; Phase A makes scan *reachable*, not
  feature-complete. Traces to: AC2 (entities land; full keyword/semantic recall
  is Phase C).

### Interfaces & contracts

- **Rust** (`bindings/node/src/provider.rs`): one new `#[napi]` method
  `scan_repository_json(&self, request_json) -> Result<String>` on
  `NativeProvider`; request shape `{ path, scope, scanFilter? }`, response the
  serialized `ScanSummary`.
- **TS** (`packages/node/src/binding.ts`): `NativeProviderBinding` interface
  mirroring the Rust surface, plus minimal handle-proxy sub-interfaces
  (`NativeMemoryApiBinding`, `NativeRecallApiBinding`, `NativeGraphApiBinding`,
  …) for the operations the facade dispatches. Registered on `NativeBinding`.
- **TS** (`packages/node/src/provider.ts`, new): `createNativeProviderTransport`
  returning a `NativeProviderTransport` with `recall`, `write`, `scan`,
  `consolidate`, `putEntity`, `batchIngest`.

### Component / module decomposition

- `engram-ingest` — gains `KnowledgeRepoGraph` (relocated from MCP). New module
  `repo_graph.rs` (or appended to an existing module).
- `mcp/engram-mcp/src/codegraph.rs` — imports `KnowledgeRepoGraph` from
  `engram-ingest` instead of defining it; the local definition is deleted.
- `bindings/node/src/provider.rs` — gains `scan_repository_json` + a private
  testable helper `scan_via_provider(provider, path, opts)`.
- `packages/node/src/binding.ts` — gains the `NativeProvider*` interfaces.
- `packages/node/src/provider.ts` (new) — the facade; re-exported from
  `packages/node/src/index.ts`.

### Failure, edge cases & resilience

- Scan path canonicalization + confinement already live in `scan_repository`
  (rejects `..`/symlink escape) — the binding does not re-implement them.
- A capability not wired by the active backend surfaces as the typed
  `CapabilityUnsupported` N-API error from `require_*`; the facade lets it
  propagate (no silent empty-result swallowing), matching the existing proxies.

### Dependencies & integration

- `engram-node` already depends on `engram-ingest` and `engram-integration`
  (sqlite feature) — no new Rust dependencies.
- `@engram/node` gains no new npm dependencies (tsup/tsc/vitest already present).
- No `contracts/` artifact (typed from Rust source).

## Tasks

### T1: Promote the scan fan-in `KnowledgeRepoGraph` to `engram-ingest`

**Depends on:** none

**Tests:**
- `cargo test -p engram-ingest` green (the fan-in type compiles + any existing
  ingest tests unaffected).
- `cargo check -p engram-mcp` green — the MCP now imports `KnowledgeRepoGraph`
  from `engram-ingest` and its local definition is gone.

**Approach:**
- Move `KnowledgeRepoGraph` (struct + `KnowledgeRepository` + `KnowledgeGraphRepository`
  impls) from `mcp/engram-mcp/src/codegraph.rs` into a new module in `engram-ingest`
  (e.g. `adapters/ingest/src/repo_graph.rs`), `pub` re-exported from the crate.
- Update `engram-mcp`'s `codegraph.rs` to `use engram_ingest::KnowledgeRepoGraph;`
  and delete the local definition.
- Run `cargo fmt --all && cargo check --workspace`.

**Done when:** `KnowledgeRepoGraph` resolves from `engram-ingest` in both the MCP
and (in T2) the binding; `cargo check --workspace` green; no duplicate definition.

### T2: Add `scanRepositoryJson` to `NativeProvider` (Rust)

**Depends on:** T1

**Tests:**
- Rust unit test (same-module `#[cfg(test)] mod tests`): open a tmp-dir SQLite
  provider via `EngramProvider::open`, call the private `scan_via_provider`
  helper against a small fixture tree (one `.rs`/`.ts` file), assert
  `ScanSummary.files > 0` and that `require_knowledge_query().list_entities(scope)`
  returns the scanned symbols.
- `check-surface-parity.sh` still passes — scan is invisible to the lint (a free
  function in `engram-ingest`, not a facade `require_*`), so no SPECIAL_CASES
  entry is needed or meaningful.

**Approach:**
- Extract a private, testable helper
  `fn scan_via_provider(provider: &EngramProvider, path, opts) -> CoreResult<ScanSummary>`
  that pulls `require_knowledge` + `require_graph`, builds `KnowledgeRepoGraph`,
  and calls `scan_repository`. Kept `fn` (private); tested in-file.
- Add `#[napi(js_name = "scanRepositoryJson")] pub fn scan_repository_json(&self,
  request_json: String) -> Result<String>` that decodes `{ path, scope,
  scanFilter? }`, calls the helper, encodes the `ScanSummary`.
- Keep the lexical-feed + embed steps out of scope (Phase C); a code comment
  names the deferral.

**Done when:** `NativeProvider.scanRepositoryJson` exists, the integration test
is green, parity lint passes.

### T3: Add `NativeProvider` to the `@engram/node` binding surface

**Depends on:** T2

**Tests:**
- `pnpm --filter @engram/node typecheck` green.
- Grep parity: every method on the Rust `NativeProvider` (+ the handle proxies
  the facade uses) has a matching member on `NativeProviderBinding` / its
  sub-interfaces.

**Approach:**
- In `packages/node/src/binding.ts`, add `NativeProviderBinding` (constructor
  `new(configJson)`, `fromProfileFile`, `capabilitiesJson`, `consolidateJson`,
  `scanRepositoryJson`, and the `require*Api` accessors returning the proxy
  sub-interfaces), the minimal proxy sub-interfaces
  (`NativeMemoryApiBinding` with `writeJson`/`searchJson`/`forgetJson`,
  `NativeRecallApiBinding` with `recallJson`, `NativeGraphApiBinding` with
  `putEntityJson`/`getEntityJson`/`neighborsJson`, `NativeBatchApiBinding` with
  `ingestJson`, `NativeBeliefsApiBinding` with `upsertBeliefJson`), and add
  `NativeProvider` to `NativeBinding`.

**Done when:** `NativeProvider` is typed and exported; typecheck green.

### T4: TS provider facade in `@engram/node`

**Depends on:** T3

**Tests:**
- `vitest` unit tests with a mock `NativeProviderBinding`: `recall` calls
  `requireRecallApi().recallJson` with a serialized `RetrievalRequest`; `write`
  calls `requireMemoryApi().writeJson`; `scan` calls `scanRepositoryJson`;
  `consolidate` calls `consolidateJson` with `dryRun` forwarded; `putEntity`
  calls `requireGraphApi().putEntityJson`; `batchIngest` calls
  `requireBatchApi().ingestJson`. Assert the dispatched method + JSON shape per
  operation.

**Approach:**
- New `packages/node/src/provider.ts`: `createNativeProviderTransport(options)`
  loads the native binding, constructs `NativeProvider` from a config, and returns
  a `NativeProviderTransport` whose methods encode typed args to JSON, call the
  binding, and decode the JSON result. Inject the binding loader for tests
  (mirrors `loadNativeBinding`).
- Re-export `createNativeProviderTransport` + types from
  `packages/node/src/index.ts`.

**Done when:** facade unit tests green; exported from `@engram/node`.

### T5: End-to-end smoke + gates

**Depends on:** T4

**Tests:**
- Manual QA smoke: build the native addon (`pnpm run build:native`), then a
  script constructs `NativeProvider` from a tmp SQLite config, writes a memory,
  scans a tiny fixture directory, runs `consolidate({ dryRun: false })`
  (**execution** — observe a `ConsolidationRun` with task-level outcomes), and
  recalls — recording the observed JSON at each step.

**Approach:**
- Add the smoke as a `vitest` test (or a `scripts/` one-shot) gated behind the
  built addon; skip with a clear message if the addon is absent.
- Run `pnpm run typecheck`, `pnpm run test`, `cargo test -p engram-node -p
  engram-ingest`, and `.codex/hooks/check-surface-parity.sh`.

**Done when:** smoke records observed results; all gates green; `git status`
clean.

## Rollout

- **Delivery:** additive — the new facade + binding surface land alongside the
  existing flat engines; nothing is removed or deprecated in this spec (the flat
  engines remain until the TS modules in Phases C–E consume `NativeProvider`).
  Fully reversible (delete the new files).
- **Infrastructure / external-system integration / deployment sequencing:** none
  — pure library change, no infra, no migrations.

## Risks

- **Boundary call (facade in `@engram/node` vs new package)** — pressure-tested
  by the pre-EXECUTE adversarial review; reversible by extraction if the facade
  grows non-transport logic.
- **Hand-written binding drift** — the `binding.ts` interface can fall behind the
  Rust `#[napi]` surface; mitigated by the T3 grep-parity check (a true generated
  surface is a Phase-A-follow-up, not this spec).
- **Scan feature gap** — Phase A scan writes entities/graph but does not feed the
  lexical lane or embed; full keyword/semantic recall of scanned code is Phase C.
  Named in Design decisions, not silently dropped.

## Changelog

- 2026-07-31: initial plan (full mode, RFC-0017 Phase A keystone adoption slice).
