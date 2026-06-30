# Plan: Mutating Consolidation Gates

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a gated mutating consolidation service in `engram-core` that composes an
`EvaluationRunner`, one protected `EvaluationFixture`, and an injected
`ConsolidationMutationExecutor`. The service owns orchestration and audit
translation only; concrete mutation algorithms remain outside core.

Tempted to add compaction or summarization now; declining because this slice is
the safety gate those algorithms must pass through. Tempted to add a scheduler;
declining because background execution is an integration concern.

## Constraints

- No public v1 schema changes.
- No concrete store, vector, embedding, model, scheduler, runtime, or
  TypeScript dependency in `engram-core`.
- Keep existing consolidation modules focused; do not grow `service.rs` into a
  mutating god module.

## Construction tests

**Unit/integration tests:** successful gated mutation, failed pre-evaluation,
failed post-evaluation, and explicit `dryRun=false` validation.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Add `ConsolidationMutationExecutor` as the executor port and
`ConsolidationMutationOutcome` as the executor's audit payload. Add
`GatedConsolidationService` as the orchestrator implementing
`ConsolidationService`.

### Component / module decomposition

- `validation.rs` owns dry-run vs mutating request checks.
- `planner.rs` exposes planned task kinds without executing them.
- `evaluation_gate.rs` owns evaluation report interpretation and task results.
- `mutating.rs` owns orchestration and executor-port composition.
- Tests own fake evaluators/executors and ordering assertions.

### Failure, edge cases & resilience

Pre-evaluation failure returns a failed run without calling the executor.
Executor failure returns a failed run with adapter error evidence.
Post-evaluation failure returns `CompletedWithErrors` because durable work may
already have happened and must remain auditable.

## Tasks

### T1: Gated mutating consolidation service

**Depends on:** dry-run consolidation service and evaluation runner port.

**Tests:**
- Passing pre/post reports wrap executor execution.
- Failed pre-report prevents executor execution.
- Failed post-report marks the run `CompletedWithErrors`.
- Missing or non-false `dryRun` is rejected before evaluation.

**Approach:**
- Add focused evaluation-gate and mutating modules.
- Keep task execution behind `ConsolidationMutationExecutor`.
- Reuse existing domain `ConsolidationRun` and `EvaluationFixture` types.

**Done when:** consolidation tests and full repository gates pass.

## Rollout

Library code only. Concrete consolidation algorithms, service wiring, and
schedulers remain future slices.

## Risks

- A generic executor port can hide weak audit data; tests assert task outcomes
  and errors are carried into the returned run.
- Post-evaluation failure happens after possible mutation; status and errors
  must make that clear.

## Changelog

- 2026-06-30: initial plan for gated mutating consolidation orchestration.
- 2026-06-30: shipped `GatedConsolidationService` with protected pre/post
  evaluation gates and an injected mutation executor port.
