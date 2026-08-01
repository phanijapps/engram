# Spec: TS runtime maintenance module

- **Status:** Shipped <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0017 (Phase E / Module 3, Accepted), ADR-0022, the Phase A facade + Phase C `@engram/runtime` (shipped)
- **Mode: light** — a new module within the existing `@engram/runtime` package, mirroring the shipped ingest module (`ts-provider-facade` → `ts-runtime-ingest`); no new package, no new dep, no Rust change. No structural risk trigger fires.
- **Contract:** none — composes `NativeProviderTransport.consolidate`; no new surface
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

## Objective

The third deployable TS module: **`engram-maintain`**, a CLI that runs
consolidation (reflection + decay) over the held provider on the Phase A facade —
one-shot or on an `--every <ms>` schedule. This is RFC-0017's Module 3 (the
"sleep-time" keeper), the maintenance leg of the 3-mode target. It mirrors the
ingest module exactly (same package, same scheduling/signal pattern), swapping
`scan` for `consolidate`. No new dependency.

## Boundaries

### Always do
- Route through `createNativeProviderTransport` (the held provider); reuse the shared helpers from `@engram/runtime`.
- Mirror the ingest module's structure (`src/maintenance/`) + scheduling/signal pattern (signals in the bin, not the library).

### Ask first
- Adding a runtime dependency; changing the consolidation contract.

### Never do
- Bypass the facade; put the scheduler in Rust; re-implement domain logic in TS; make `@engram/runtime` a god-package (each module isolated).

## Testing Strategy
- **TDD** — `runMaintain` dispatch + scheduling over a mock transport + fake timers (mirror the ingest tests).
- **Goal-based** — recursive typecheck + `@engram/runtime` build; the `engram-maintain` bin resolves.
- **Manual / integration QA** — a subprocess test (real bin → `consolidate` execution → a `ConsolidationRun`).

## Acceptance Criteria
- [x] An `engram-maintain` bin exists in `@engram/runtime` and runs consolidation over the facade (one-shot), emitting a `ConsolidationRun`.
- [x] `--dry-run` forwards `dryRun: true` to `consolidate`; `--since <iso>` / `--until <iso>` forward into the request (delta consolidation) — TDD-asserted.
- [x] `engram-maintain ... --every <ms>` runs consolidate on a `setInterval`; SIGINT/SIGTERM (in the bin) clear the interval + exit cleanly.
- [x] `runMaintain` is a pure library function (no `process.exit`/signal listeners), exported from the package-root facade alongside `runIngest`.
- [x] TDD: dispatch + scheduling + stop-clears-interval + consolidate-error-survival (mock transport, fake timers) green; argv validation (required flags, integer `--every`) covered.
- [x] A subprocess test runs the real bin against the live addon and observes a `ConsolidationRun` — asserts `status` is present (`tasks` is `skip_serializing_if = Vec::is_empty`, so an empty corpus omits it).
- [x] `pnpm run typecheck` (recursive) + `pnpm --filter @engram/runtime test` green.
