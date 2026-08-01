# Plan: TS runtime maintenance module

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting <!-- Drafting | Executing | Done -->

## Approach

Add the maintenance module to `@engram/runtime`, mirroring the ingest module.
Three tasks:

1. **(T1)** `src/maintenance/cli.ts`: `runMaintain({transport, scope, dryRun?, every?})`
   (one-shot or `setInterval`; errors propagate one-shot / swallowed periodic),
   `parseMaintainArgs` (`--config/--tenant/--workspace/--dry-run/--every`),
   `runMaintainFromArgs`. `src/maintenance/bin.ts` `#!` entry (signals wired there).
2. **(T2)** TDD: dispatch + scheduling + stop-clears-interval + error-survival
   (mock transport, fake timers) + argv validation — mirroring `test/ingest.test.ts`.
3. **(T3)** Subprocess test (real bin → `consolidate` execution → `ConsolidationRun`) +
   gates. Export `runMaintain` from the package-root facade; declare the bin in
   `package.json`; extend `tsup.config.ts` entry.

The module is a near-clone of ingest; the differences: `consolidate` returns a
`ConsolidationRun` (logged as JSON), takes `dryRun` (+ optional `--since`/`--until`
for delta consolidation) instead of a `--path`, and there is no `--path` flag.

## Constraints
- RFC-0017 Phase E; ADR-0022; the Phase A facade + Phase C runtime package.
- AGENTS.md — no god-packages; reuse shared helpers.

## Design (LLD)
- `runMaintain({transport, scope, dryRun?, since?, until?, every?})` → one-shot
  `await transport.consolidate({scope, dryRun, since, until})` + log the run;
  periodic via `setInterval`. Returns `{stop}` in periodic mode. No
  `process.exit`/signals (those live in `bin.ts`, mirroring ingest).
- `parseMaintainArgs`: `--config/--tenant/--workspace` required; `--dry-run`
  (boolean flag), `--since <iso>` / `--until <iso>` (forwarded as-is),
  `--every <ms>` (integer, `/^\d+$/`).
- `ConsolidationRun` is logged as JSON (the facade returns `unknown`); a minimal
  local type `{ status: string; tasks?: unknown[] }` (`tasks` is
  `skip_serializing_if = Vec::is_empty`, `core/domain/src/operations.rs:277`).

## Tasks

### T1: maintenance module (runMaintain + CLI + bin + package wiring)
**Depends on:** none
**Tests:** no stub (T2 covers it).
**Approach:** `src/maintenance/cli.ts` + `src/maintenance/bin.ts`; add
`engram-maintain` bin to `package.json`; add `src/maintenance/bin.ts` to
`tsup.config.ts` entry; export `runMaintain` from `src/index.ts`.
**Done when:** typecheck + build green; bin has the shebang.

### T2: TDD dispatch + scheduling
**Depends on:** T1
**Tests:** mock transport + fake timers: one-shot calls `consolidate` once with
`{scope, dryRun}`; `--dry-run` forwards `dryRun: true`; `--since`/`--until`
forward; periodic cadence; stop-clears-interval; consolidate-error-survival; argv
validation (required flags, integer `--every`).
**Done when:** maintenance tests green.

### T3: Subprocess test + gates
**Depends:** T2
**Tests:** spawn the real bin against the live addon (temp sqlite config) → assert
stdout is a `ConsolidationRun` with a present `status` (`tasks` may be absent —
empty corpus, `skip_serializing_if`). Skips if the build chain isn't ready.
**Done when:** subprocess test green/skipped; recursive typecheck + runtime tests
green; `git status` clean.

## Rollout
Additive (new module in an existing package); nothing removed; reversible.

## Changelog
- 2026-08-01: initial plan (light mode; near-clone of the ingest module).
