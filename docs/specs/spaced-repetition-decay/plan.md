# Plan: spaced-repetition-decay (Slice A)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

Three pieces: (1) a `CompositeConsolidationExecutor` in `core/consolidation`
(dispatches each planned task to the first registered executor that handles it);
(2) an Ebbinghaus curve pure helper; (3) a `DecayExecutor` in a new adapter
crate. All zero-contract-change. The composite is the architectural enabler that
lets Decay + Reflection coexist.

## Tasks

### T1: Composite executor + Ebbinghaus curve (TDD)
- `core/consolidation/src/composite.rs` — `CompositeConsolidationExecutor { executors: Vec<Arc<dyn ConsolidationMutationExecutor>> }`; execute() calls each
  executor; merges outcomes (first non-Skipped result per task wins).
- Ebbinghaus curve pure fn in composite or a sibling module.
- Tests: 2 stub executors handling different kinds → correct dispatch.

### T2: DecayExecutor (TDD)
- New crate `adapters/consolidation/decay/` — `DecayExecutor` handles `Decay`:
  reads scoped active memories via an injected port, filters by
  `policy.expires_at <= now`, marks `Expired`, emits `MemoryEventKind::Expired`.
- Tests: stub memory port with expired/active/legal-hold records.

### T3: Full gates + ship.
