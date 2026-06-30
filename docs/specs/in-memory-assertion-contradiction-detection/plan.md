# Plan: In-Memory Assertion Contradiction Detection

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add `BeliefContradictionDetection` to the hybrid consolidation plan and route it
to a focused in-memory detection module. The detector scans scoped active memory
assertions, groups them by subject and predicate, creates contradictions for
different object values, and uses deterministic assertion-pair keys to avoid
duplicate open records.

Tempted to infer contradictions from belief text; declining because explicit
assertions are the only stable structured claim surface. Tempted to auto-resolve
or retract beliefs; declining because detection and resolution are separate
review lifecycles.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` limited to task planning; concrete detection remains in
  the in-memory adapter.
- Keep contradiction detection separate from belief synthesis and resolution.

## Construction tests

**Unit/integration tests:**

- Hybrid dry-run includes `BeliefContradictionDetection`.
- Conflicting scoped assertions create contradiction-detected events and task
  counters.
- Re-running detection creates no duplicate contradiction/events.
- Compatible, expired, and out-of-scope assertions are skipped.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

`InMemoryConsolidationExecutor` continues to implement the existing
`ConsolidationMutationExecutor` port. `DryRunConsolidationService` continues to
return planned task reports. No new public JSON schema or TypeScript contract is
introduced.

### Component / module decomposition

- `crates/engram-core/src/consolidation/planner.rs` adds the task to hybrid
  planning.
- `crates/engram-store-memory/src/consolidation/contradiction_detection.rs`
  owns assertion grouping, duplicate detection, contradiction construction, and
  event creation.
- Tests live in `crates/engram-core/tests/consolidation_dry_run.rs` and
  `crates/engram-store-memory/tests/consolidation_contradiction_detection.rs`.

### Failure, edge cases & resilience

Assertion pair keys are deterministic after sorting assertion target IDs. The
task uses the consolidation run start timestamp to skip expired evidence.

## Tasks

### T1: Plan and execute contradiction detection

**Depends on:** PHASE26 assertion-backed belief synthesis and existing
contradiction domain types.

**Tests:**

- Hybrid planning includes `BeliefContradictionDetection`.
- In-memory detection creates one open contradiction and source events for one
  conflicting assertion pair.

**Approach:**

- Update the hybrid planned task list.
- Add a focused contradiction-detection module and route it from the executor.

**Done when:** focused core and adapter tests pass.

### T2: Document and wire the phase status

**Depends on:** T1.

**Tests:**

- Full repository gates pass.
- `docs/implementation/phases.json` marks the phase done after gates pass.

**Approach:**

- Update roadmap status documents and changelog.
- Run the repository validation suite before commit.

**Done when:** docs, phase JSON, and code are committed together.

## Rollout

Library code only. Semantic contradiction detection, resolution workflows,
belief retraction, and model-assisted review remain future phases.

## Risks

- Exact JSON object comparison is conservative and may miss semantic conflicts;
  this is intentional until model-assisted detection has its own spec.
- There is no contradiction read API yet, so duplicate prevention and events are
  the observable adapter contract in tests.

## Changelog

- 2026-06-30: initial plan for deterministic assertion contradiction detection.
- 2026-06-30: shipped hybrid planning and in-memory explicit assertion-pair
  contradiction detection with idempotent review records and source events.
