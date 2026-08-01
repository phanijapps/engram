# Plan: TS runtime layer + ingest module

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting <!-- Drafting | Executing | Done -->

> **Plan contract:** the implementation strategy; changes noted in the changelog.

## Approach

A new `packages/runtime` package (`@engram/runtime`) is the TS operational layer
home for all three RFC-0017 modules; this slice ships the first one — `engram-ingest`.
Four moves:

1. **(T1)** Scaffold the package (package.json with an `engram-ingest` bin,
   tsconfig, tsup build) + shared config/scope helpers in `src/shared/`.
2. **(T2)** The ingest module: argv parsing → config → `createNativeProviderTransport`
   → `scan(path)`. One-shot by default; `--every <duration>` wraps it in
   `setInterval`.
3. **(T3)** TDD: the scheduling + dispatch logic (parse argv, one-shot vs
   periodic, calls `transport.scan` with the right shape) over a mock transport.
4. **(T4)** Manual-QA smoke: run the built bin against a fixture repo + the real
   addon, record the scan summary; recursive typecheck + runtime tests.

Riskiest part: keeping the package non-god (each module isolated) and avoiding a
scheduler/CLI-arg dep. Mitigated by manual argv parse (mirror the MCP's
`config.rs` minimalism) + `setInterval`, and the `src/<module>/` layout.

## Constraints

- **RFC-0017 Phase C** — one module per concern; TS owns scheduling/transport;
  Rust owns deterministic operations.
- **ADR-0022 / Phase A** — route through the held-provider facade; no engine
  bypass.
- **AGENTS.md** — no god-packages; focused modules; package `index.ts` a facade.

## Construction tests

**Integration tests:**
- `vitest` unit tests over a mock `NativeProviderTransport`: argv → scan call
  shape; one-shot runs once; `--every` schedules (use fake timers).

**Manual verification:**
- Build the bin; run `engram-ingest --config … --path <fixture>` against the real
  addon; record the `ScanSummary` (entities > 0).

## Design (LLD)

### Design decisions

- **One shared `@engram/runtime` package** for all three modules (ingest now;
  maintenance/HTTP-MCP later) — they share config/scope construction + the
  facade; sub-directories per module prevent a god-package. Traces to: AC1, AC4.
- **`setInterval` scheduling, no cron dep** — the cron-first slice; a cron
  library / queue client is a later, ask-first slice. Traces to: AC3.
- **Manual argv parse** — the bin takes `--config` (JSON string or `@file` path),
  `--path`, `--scope` (tenant/workspace), `--every`. No CLI-framework dep (mirrors
  the MCP's hand-rolled argv parsing). Traces to: AC2.
- **Rejected: one package per module** — would duplicate config/scope helpers
  across three packages; the shared-runtime layout centralizes them.

### Component / module decomposition

- `packages/runtime/package.json` — `@engram/runtime`, bin `engram-ingest` →
  `dist/ingest.js`, deps `@engram/node` (facade) + `@engram/contracts`.
- `packages/runtime/src/shared/config.ts` — build the `EngramConfig` JSON (from
  `--config` arg: inline JSON or `@path`) + `Scope`.
- `packages/runtime/src/ingest/cli.ts` — argv parse + orchestrate (construct
  transport, scan one-shot or periodic).
- `packages/runtime/src/ingest/index.ts` — package entry re-export.
- `packages/runtime/test/ingest.test.ts` — TDD dispatch/scheduling tests.

### Failure, edge cases & resilience

- A missing addon surfaces as the facade's load error (clear); the bin exits
  non-zero with the message.
- `--every 0` / unset → one-shot (run once, exit 0).
- Scan errors are logged + (in periodic mode) don't kill the schedule; in
  one-shot mode they exit non-zero.

## Tasks

### T1: Scaffold packages/runtime + shared config/scope helpers

**Depends on:** none

**Tests:** no stub (goal-based). `pnpm --filter @engram/runtime typecheck` passes
once T2 lands; here just the scaffold + a `buildEngramConfig`/`buildScope` helper.

**Approach:**
- `package.json` (`@engram/runtime`, `"type":"module"`, bin, tsup, vitest, tsc,
  deps `@engram/node`, `@engram/contracts`).
- `tsconfig.json` (mirror `@engram/client`).
- `src/shared/config.ts`: `buildEngramConfig(configArg)` (inline JSON or `@path`)
  + `buildScope({tenant, workspace})`.

**Done when:** package is workspace-wired; `shared/config.ts` typechecks.

### T2: engram-ingest CLI (one-shot + --every)

**Depends on:** T1

**Tests:** no stub (manual QA — see T4).

**Approach:**
- `src/ingest/cli.ts`: parse argv (`--config`, `--path`, `--scope`, `--every`);
  build config + scope; `createNativeProviderTransport({configJson})`; one-shot
  `await transport.scan({path, scope})` + log summary; if `--every`, wrap in
  `setInterval`. Export the orchestration (separate from argv) so T3 can test it.
- `src/ingest/index.ts`: the bin entry (`#!/usr/bin/env node`) calling cli.

**Done when:** `engram-ingest` runs (T4 smoke); orchestration is exported for T3.

### T3: TDD ingest scheduling + dispatch

**Depends on:** T2

**Tests:**
- `vitest` with a mock transport + fake timers: one-shot calls `scan` once then
  resolves; `--every 50ms` calls `scan` 3× over 150ms (fake timers); argv parsing
  produces the expected `{path, scope}` scan request.

**Approach:**
- Extract `runIngest({transport, path, scope, every?})` (pure orchestration,
  injectable transport + clock) so the test drives it without argv/real addon.

**Done when:** dispatch + scheduling tests green.

### T4: Smoke + gates

**Depends on:** T3

**Tests:** manual QA — `pnpm --filter @engram/runtime build`, then run the bin
against a fixture repo + the real addon; record the `ScanSummary`.

**Approach:**
- Run the smoke; then `pnpm run typecheck` (recursive) + `pnpm --filter
  @engram/runtime test`; confirm the bin resolves via `pnpm --filter
  @engram/runtime exec engram-ingest -- --help` (or equivalent).

**Done when:** smoke recorded; all gates green; `git status` clean.

## Rollout

- **Delivery:** additive new package + bin; nothing removed. Reversible (delete
  the package). No Rust change, no contract change, no data migration.

## Risks

- **God-package drift** — mitigated by `src/<module>/` layout + shared-only-in-
  `shared/`; the adversarial review checks the boundary.
- **Addon dependency for the bin** — the bin needs the built `.node`; the smoke
  verifies it loads; tests inject a mock so CI doesn't need the addon.

## Changelog

- 2026-07-31: initial plan (full mode, RFC-0017 Phase C — first TS module; cron-first).
