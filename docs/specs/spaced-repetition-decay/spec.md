# Spec: spaced-repetition-decay (Slice A)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** AGENTS.md, `retire-memory-inmem/spec.md` (reintroduction gate), `in-memory-consolidation-decay/spec.md` (the retired behavior spec)
- **Contract:** none — reuses `ConsolidationTaskKind::Decay` + existing `MemoryStatus`/`MemoryEventKind`/`Policy`; no v1 change
- **Shape:** service

## Objective

Restore the decay consolidation capability (retired with the in-memory executor)
as a production `ConsolidationMutationExecutor` backed by SQLite, introduce the
**composite-executor pattern** so multiple executors coexist in a Hybrid run, and
add the **Ebbinghaus forgetting-curve** (`R = e^(-t/S)`) as a ranking signal for
due records. Zero contract change. True spaced repetition (S grows with recall)
is deferred until recall tracking is added (Slice B).

## Boundaries

### Always do
- Implement a `DecayExecutor` that handles `ConsolidationTaskKind::Decay`: mark
  in-scope active records with `policy.expires_at <= now` as `Expired`, skip
  `Retention::LegalHold`, emit `Expired` events, report `records_decayed`.
- Implement a `CompositeConsolidationExecutor` that dispatches each planned task
  to the first registered executor that doesn't Skip it.
- Add the Ebbinghaus curve as a pure helper; use it to rank/prioritize due
  records + enrich the Expired event reason.
- Keep both in engine-neutral crates (ports only).

### Never do
- Change any v1 contract (no new MemoryRecord fields; recall tracking = Slice B).
- Put decay algorithms in `engram-core`.
- Introduce real spaced repetition without recall tracking (deferred).

## Testing Strategy
- **DecayExecutor: TDD** — stub memory service with active + expired + legal-hold
  records; assert expired are marked, legal-hold skipped, events emitted.
- **Composite executor: TDD** — two stub executors handling different task kinds;
  assert each task dispatched to the right executor.
- **Ebbinghaus curve: TDD** — pure function; assert R values at known t/S ratios.

## Acceptance Criteria
- [x] A `DecayExecutor` handles `Decay` tasks: marks expired records, skips
  LegalHold, emits events, reports stats.
- [x] A `CompositeConsolidationExecutor` dispatches each planned task to the
  first non-Skipping executor.
- [x] The Ebbinghaus curve `R = e^(-t/S)` is a tested pure function.
- [x] Zero v1 contract change; all gates green.
