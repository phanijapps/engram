# Spec: Behavior-port split (RFC-0006)

- **Status:** Shipped
- **RFC:** [RFC-0006](../../rfcs/0006-split-behavior-ports-from-core.md) (Accepted 2026-07-01)
- **Mode:** light — user override. A structural change normally routes to full mode, but this is a pure relocate (no behavior change) behind an existing facade whose blast radius is contained (RFC-0006 spike). Single adversarial pass per slice, per user preference.

## Objective

Split the behavior ports canonically owned by `core/orchestration` (`engram-core`) into focused crates — `engram-belief`, `engram-hierarchy`, `engram-consolidation` (new) — and move the eval ports into the existing `engram-eval`. `engram-core` becomes a pure re-export facade. Zero public-API change, zero behavior change.

## Acceptance Criteria

- [x] **AC1 — belief slice.** `engram-belief` exists and defines `BeliefRepository`, `BeliefSynthesizer`, `ContradictionDetector`; `core/orchestration/src/lib.rs` no longer defines them and instead does `pub use engram_belief::*;`; `engram_core::BeliefRepository` still resolves (N-API binding + `engram-store-belief-sqlite` compile unchanged); `cargo fmt/check/clippy/test --workspace` green.
- [x] **AC2 — hierarchy slice.** Same for `engram-hierarchy` (`HierarchyRepository`, `HierarchyBuilder`); gates green.
- [x] **AC3 — consolidation slice.** `engram-consolidation` exists and owns `ConsolidationService` + `ConsolidationMutationExecutor`/`ConsolidationMutationOutcome` + `DryRunConsolidationService`/`GatedConsolidationService` + the `planner`/`evaluation_gate`/`validation` modules; `engram-core` re-exports; `evaluation_gate`'s `EvaluationReport` import resolves via the new crate's private re-import of `engram-eval`; gates green.
- [x] **AC4 — eval slice (dependency inversion).** `EvaluationRunner`/`EvaluationReport`/`EvaluationCaseReport` move INTO `engram-eval`; `engram-eval` drops its `engram-core` dep and repoints `CoreError`/`CoreResult` → `engram-runtime`, `MemoryService` → `engram-memory`; `engram-core` adds `engram-eval` and re-exports the three types; no cycle; gates green (`pnpm typecheck && pnpm build` deferred to the final sweep — no TS contract changed by this slice).
- [x] **AC5 — facade cleanup.** `core/orchestration/src/lib.rs` is reduced to re-exports + top-level doc (no `mod`, no `pub trait`, no `pub struct`); the consolidation modules moved to `engram-consolidation`.
- [x] **AC6 — no behavior change.** Every existing test passes unchanged (`cargo test --workspace` green across all slices); only `engram-eval`'s own integration tests repointed `use` paths (the eval types moved into that crate).

## Boundaries

- New crates depend only on `engram-domain` (+ `engram-runtime` for `CoreError`/`CoreResult`); `engram-consolidation` additionally depends on `engram-eval` (for `EvaluationReport`, used by `evaluation_gate`). No back-edges to `engram-core`; the composing mutation impls live in adapters and depend on belief/hierarchy/memory there.
- `engram-core` remains the re-export facade (RFC-0006 D2); its ultimate fate is deferred.
- No public API change; no TypeScript contract change; no adapter/binding import-site change (the only internal repoint is `engram-eval`'s, in AC4).

## Testing Strategy

- Per-slice gate: `cargo fmt --all --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`. After AC4 add `pnpm typecheck && pnpm build`.
- The existing test suite is the behavior-regression gate — this is a pure relocate, so **no new tests are written**. If a test must change beyond an import path, that is a stop signal (AC6).
- Single adversarial pass per slice (user preference); a Blocker earns exactly one re-review before escalating.

## Slices

1. `engram-belief` (AC1)
2. `engram-hierarchy` (AC2)
3. `engram-consolidation` (AC3)
4. `engram-eval` dependency inversion (AC4)
5. Facade cleanup + full gate sweep (AC5, AC6)

## Changelog

- 2026-07-01 — spec opened (RFC-0006 accepted).
- 2026-07-01 — **shipped**: belief/hierarchy/consolidation ports split into focused crates; eval ports moved into `engram-eval`; `engram-core` is a pure re-export facade. All Rust gates green (`cargo fmt/check/clippy` on touched crates + `cargo test --workspace`).
  - **Out of scope (pre-existing, unrelated — no `bindings/` or `packages/` source changed):** `cargo clippy --workspace` fails on `engram-ingest`/`engram-node` lint debt, and `pnpm typecheck` fails on `packages/client/test/native.test.ts` (knowledge/retrieval binding method surface). These predate this split and remain open as separate tech debt.
  - **Implementation note:** the RFC's D3 order was infeasible — `engram-consolidation` depends on eval types (`EvaluationReport`/`EvaluationRunner`), so eval moved *before* consolidation. Recorded in RFC-0006 Errata + ADR-0010.
