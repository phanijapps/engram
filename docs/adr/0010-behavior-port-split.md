# ADR 0010: Behavior-port split (RFC-0006)

## Status

Accepted

## Context

`engram-core` (`core/orchestration`) was a god-module: alongside its role as
the compatibility facade, it canonically owned 8 behavior `pub trait`s and 5
`pub struct`s spanning four unrelated concerns (belief, hierarchy,
consolidation, evaluation). This violated `AGENTS.md`'s "no god modules" rule
and left the `engram-belief` / `engram-hierarchy` / `engram-consolidation`
crates that the roadmap names as aspirational-only. [RFC-0006](../rfcs/0006-split-behavior-ports-from-core.md)
(accepted 2026-07-01) decided the split; this ADR records the durable decisions
and two refinements that implementation revealed.

## Decision

Move the behavior ports into focused crates; `engram-core` becomes a pure
re-export facade (module doc + `pub use` only, no `pub trait` or `pub struct`).

- **`engram-belief`** (`core/belief`): `BeliefRepository`, `BeliefSynthesizer`,
  `ContradictionDetector`. Depends on `engram-domain` + `engram-runtime`.
- **`engram-hierarchy`** (`core/hierarchy`): `HierarchyRepository`,
  `HierarchyBuilder`. Depends on `engram-domain` + `engram-runtime`.
- **`engram-consolidation`** (`core/consolidation`): `ConsolidationService` +
  `ConsolidationMutationExecutor`/`ConsolidationMutationOutcome` +
  `DryRunConsolidationService`/`GatedConsolidationService` + the
  `planner`/`evaluation_gate`/`validation` modules. Depends on `engram-domain`
  + `engram-runtime` + `engram-eval`.
- **`engram-eval`** (`core/eval`, existing): now also owns `EvaluationRunner`,
  `EvaluationReport`, `EvaluationCaseReport` (moved out of `engram-core`).
  Depends on `engram-domain` + `engram-memory` + `engram-runtime`; it no longer
  depends on `engram-core` (the edge inverted).
- **`engram-core`**: pure facade — `pub use` of all behavior crates +
  `engram-retrieval` + `engram-runtime`. Owns no port definitions.

The public API is unchanged: every `engram_core::<port>` path still resolves via
re-export.

## Refinements discovered in implementation

1. **Dependency inversion (eval).** `engram-eval` previously depended on
   `engram-core`; the move flips the edge (`engram-core` → `engram-eval`).
   `engram-eval` repointed `CoreError`/`CoreResult` → `engram-runtime` and
   `MemoryService` → `engram-memory` to avoid a cycle.
2. **Ordering (vs RFC-0006 D3).** `engram-consolidation` depends on
   `engram-eval` (`EvaluationReport`/`EvaluationRunner`), so eval moved
   *before* consolidation. RFC-0006 D3's "consolidation before eval" was
   infeasible. Recorded as RFC-0006 Errata.
3. **Consolidation's edge (vs RFC-0006 body).** `engram-consolidation` depends
   on `engram-eval`, not on belief/hierarchy/memory. The composing mutation
   impls live in adapters (`engram-store-memory`), which depend on
   belief/hierarchy/memory there.

## Consequences

- `engram-core` is a pure facade (`AGENTS.md`-compliant); the god-module is
  gone. `arch_divergence.md` Area 2 moves from 80% to 95%.
- Zero public API change; zero behavior change (`cargo test --workspace` green).
  The 35 `engram_core::` consumer sites and the N-API binding resolve unchanged
  via re-export; only `engram-eval`'s own integration tests repointed `use`
  paths.
- The facade's ultimate fate (permanent vs eventual deprecation) remains
  deferred (RFC-0006 D2), tracked against `arch_divergence.md` Area 1.
- `engram-core`'s consolidation tests still live in `core/orchestration/tests/`
  (they exercise the re-exported API); relocating them to
  `engram-consolidation` is optional future cleanup.
