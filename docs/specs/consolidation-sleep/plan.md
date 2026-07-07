# Plan: Consolidation And Sleep Cycle

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Start with a dry-run consolidation service that creates auditable run records
from existing domain types. It should validate scope and requester, execute a
small set of no-op task planners, and return task outcomes without mutating
repositories.

Tempted to implement summarization and pruning immediately; declining because
mutation policy and evaluation gates are not specified. Tempted to add a
background scheduler; declining because first slice is a library boundary.
Tempted to combine consolidation with belief/hierarchy construction; declining
because each mutating task needs its own acceptance criteria.

## Constraints

- ADR-0003 keeps behavior in Rust crates and runtime integrations outside core.
- ADR-0004 keeps accepted contracts stable while consolidation behavior is
  hardened.
- Evaluation gates must precede durable mutation.

## Construction tests

**Integration tests:** dry-run service tests for completed runs, validation
failures, and no durable mutation.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Add a focused consolidation service crate or module that consumes
`ConsolidationRequest` and returns `ConsolidationRun`. Repository mutation ports
are not needed for the first dry-run slice.

### Component / module decomposition

- `validation.rs` owns request checks.
- `planner.rs` owns task planning.
- `service.rs` owns run orchestration.
- Tests own deterministic request fixtures.

### Failure, edge cases & resilience

Invalid scope or missing requester fails before task execution. Recoverable
planner errors appear inside the run instead of being dropped.

## Tasks

### T1: Dry-run consolidation service returns auditable runs

**Depends on:** none

**Tests:**
- Valid dry-run request returns a completed run with task results and stats.
- Invalid request fails before task planning.
- Dry-run has no repository mutation path.

**Approach:**
- Add the smallest Rust service surface for consolidation dry-runs.
- Keep task planners deterministic and model-free.

**Done when:** dry-run consolidation tests and full repository gates pass.

## Rollout

Library code only. No scheduler, daemon, model call, or mutation task ships in
the first slice.

## Risks

- A no-op dry-run service can look more complete than it is; docs must keep
  mutation tasks clearly deferred.
- Future mutation tasks need evaluation regression gates before write paths.

## Changelog

- 2026-06-29: initial plan for dry-run consolidation run reporting.
- 2026-06-29: shipped dry-run consolidation service in `engram-core`.
