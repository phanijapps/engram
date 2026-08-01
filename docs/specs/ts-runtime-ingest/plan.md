# Plan: TS runtime layer + ingest module

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done <!-- Drafting | Executing | Done -->

> **Plan contract:** the implementation strategy; changes noted in the changelog.

## Approach

A new `packages/runtime` package (`@engram/runtime`) is the TS operational layer
home for all three RFC-0017 modules; this slice ships the first one — `engram-ingest`.
Four moves:

1. **(T1)** Scaffold the package (package.json with an `engram-ingest` bin,
   tsconfig, tsup build) + shared config/scope helpers in `src/shared/` + the
   package-root facade (`src/index.ts`). Micro-vitest on the helpers as T1's own
   gate.
2. **(T2)** The ingest module: argv via `node:util/parseArgs` (zero-dep, stdlib)
   → config → `createNativeProviderTransport` → `scan(path)`. One-shot by
   default; `--every <ms>` wraps it in `setInterval`, with a SIGINT/SIGTERM
   handler that clears the interval + exits cleanly.
3. **(T3)** TDD: the scheduling + dispatch logic over a mock transport + fake
   timers; argv parsing → the expected `{path, scope}` scan request.
4. **(T4)** A subprocess integration test (spawn the built bin against a named
   fixture, assert the scan summary reports entities > 0) as the automated
   regression net for the write path, plus a manual smoke and the recursive gates.

Riskiest part: keeping the package non-god (each module isolated) and the write
path having a real regression net. Mitigated by `src/<module>/` + `src/shared/`
layout, `parseArgs`, and the T4 subprocess test.

## Constraints

- **RFC-0017 Phase C** — one module per concern; TS owns scheduling/transport;
  Rust owns deterministic operations.
- **ADR-0022 / Phase A** — route through the held-provider facade; no engine
  bypass.
- **AGENTS.md** — no god-packages; focused modules; package `index.ts` a facade.

## Construction tests

**Integration tests:**
- `vitest` unit tests over a mock `NativeProviderTransport` + fake timers: one-shot
  runs `scan` once; `--every 50` calls `scan` 3× over 150 ms; argv → scan shape.
- A subprocess test (T4): spawn `engram-ingest` against `examples/rust-integration/`
  with a temp SQLite config; assert stdout's `ScanSummary` reports `entities >= 1`.

**Manual verification:**
- Run the bin against the real addon; record the `ScanSummary`.

## Design (LLD)

### Design decisions

- **One shared `@engram/runtime` package** for all three modules — they share
  config/scope construction + the facade; `src/<module>/` per module prevents a
  god-package. Traces to: AC1, AC4.
- **Package-root facade** (`src/index.ts`) exports `{ runIngest, buildEngramConfig,
  buildScope, type ScanSummary }` — a narrow programmatic API alongside the bin
  (the modules are reusable, not bin-only). Traces to: AC1.
- **`node:util/parseArgs`** for argv (Node 22 stdlib, zero-dep) — not hand-rolled.
  Traces to: AC2.
- **Scope via `--tenant <t> --workspace <w>`** (two flags) — matches the two-field
  `Scope` contract + how the MCP exposes scope as separate flags. Traces to: AC2.
- **`--every <ms>` is integer milliseconds** (`Number(arg)`); unset/0 → one-shot.
  Traces to: AC3.
- **`setInterval` scheduling, no cron dep** — cron-first slice; a cron library /
  queue client is a later, ask-first slice. Traces to: AC3.
- **Periodic shutdown** — SIGINT/SIGTERM clears the interval + exits 0 (the
  operator can Ctrl-C cleanly). Traces to: AC3.
- **Local `ScanSummary` interface** — the facade types `scan` as `Promise<unknown>`
  and `@engram/contracts` has no `ScanSummary`; the runtime defines
  `{ scanned, ingested, entities, relationships, errors, … }` locally (mirrors
  `adapters/ingest/src/scanner.rs:40-51`) for logging + the subprocess assertion.
- **Config arg `<json|path>` auto-detect** — if the `--config` value starts with
  `{`, parse as inline JSON; else treat as a file path. Traces to: AC2.
- **Rejected: one package per module** — would duplicate config/scope helpers.

### Component / module decomposition

- `packages/runtime/package.json` — `@engram/runtime`, `"type":"module"`, bin
  `engram-ingest` → `dist/ingest/cli.js`, deps `@engram/node` + `@engram/contracts`.
- `src/index.ts` — package-root facade: `export { runIngest, buildEngramConfig,
  buildScope, type ScanSummary }`.
- `src/shared/config.ts` — `buildEngramConfig(configArg)` (inline JSON or file
  path, auto-detected) + `buildScope({tenant, workspace})` + the `ScanSummary`
  interface.
- `src/ingest/cli.ts` — `parseArgs` + `runIngest({transport, path, scope, every?})`
  orchestration (one-shot or periodic) + signal handlers.
- `src/ingest/bin.ts` — `#!/usr/bin/env node` entry wiring argv → runIngest.
- `test/ingest.test.ts` — TDD dispatch/scheduling (mock transport, fake timers).
- `test/ingest.cli.test.ts` — T4 subprocess integration test.

### Failure, edge cases & resilience

- Missing addon → the facade's load error, bin exits non-zero with the message.
- `--every` unset/0 → one-shot (run once, exit 0).
- Periodic mode: scan errors are logged and do NOT kill the schedule; SIGINT/SIGTERM
  clears the interval and exits 0.
- One-shot mode: scan error → logged, exit non-zero.

## Tasks

### T1: Scaffold packages/runtime + shared helpers + package-root facade

**Depends on:** none

**Tests:** micro-vitest on `buildEngramConfig` (inline JSON vs file path) +
`buildScope({tenant, workspace})` shape — T1's own gate (no T2 dependency).

**Approach:**
- `package.json` (`@engram/runtime`, `"type":"module"`, bin `engram-ingest` →
  `dist/ingest/cli.js`, tsup, vitest, tsc, deps `@engram/node`, `@engram/contracts`).
- `tsconfig.json` (mirror `@engram/client`; extend `tsconfig.base.json`).
- `src/shared/config.ts`: `buildEngramConfig`, `buildScope`, `ScanSummary` interface.
- `src/index.ts`: re-export the shared helpers + (later) `runIngest`.

**Done when:** `pnpm --filter @engram/runtime typecheck` + the helper vitest pass.

### T2: engram-ingest CLI (parseArgs, one-shot, --every, signal handling)

**Depends on:** T1

**Tests:** no stub (covered by T3/T4).

**Approach:**
- `src/ingest/cli.ts`: `parseArgs` (`--config`, `--path`, `--tenant`, `--workspace`,
  `--every`); build config + scope; `createNativeProviderTransport({configJson})`;
  export `runIngest({transport, path, scope, every?})` — one-shot
  `await transport.scan({path, scope})` + log `ScanSummary`; if `every > 0`,
  `setInterval` + SIGINT/SIGTERM → `clearInterval` + exit 0.
- `src/ingest/bin.ts`: `#!/usr/bin/env node` → parseArgs → `runIngest`.

**Done when:** bin runs (T4); `runIngest` exported for T3.

### T3: TDD ingest scheduling + dispatch

**Depends on:** T2

**Tests:**
- `vitest` mock transport + fake timers: one-shot calls `scan` once; `every: 50`
  calls `scan` 3× over 150 ms; argv parse produces the expected `{path, scope}`.

**Approach:**
- Drive `runIngest` with an injected mock transport + `vi.useFakeTimers()`; assert
  dispatch + cadence.

**Done when:** dispatch + scheduling tests green.

### T4: Subprocess integration test + smoke + gates

**Depends on:** T3

**Tests:**
- Subprocess test: `engram-ingest --config <temp sqlite> --path examples/rust-integration/
  --tenant t --workspace w`; assert stdout `ScanSummary` reports `entities >= 1`.
  Skipped if the addon isn't built.
- Manual: record the observed `ScanSummary`.

**Approach:**
- `test/ingest.cli.test.ts` spawns the built bin; `pnpm run typecheck` (recursive)
  + `pnpm --filter @engram/runtime test`.

**Done when:** subprocess test green (addon present) / skipped (absent); all gates
green; `git status` clean.

## Rollout

- **Delivery:** additive new package + bin; nothing removed. Reversible. No Rust
  change, no contract change, no data migration.

## Risks

- **God-package drift** — `src/<module>/` + `src/shared/` layout; review checks it.
- **Addon dependency for the bin** — tests inject a mock; the subprocess test
  skips if the addon is absent.

## Changelog

- 2026-07-31: initial plan (full mode, RFC-0017 Phase C — first TS module; cron-first).
- 2026-08-01: pre-EXECUTE review (0 Blockers, 7 Concerns + 3 Nits) folded in —
  `node:util/parseArgs`, `--tenant/--workspace`, integer-ms `--every`, SIGINT/SIGTERM
  shutdown, package-root facade, local `ScanSummary`, config-arg auto-detect, T1
  micro-test, T4 subprocess integration test, named fixture.
