# Plan: In-Memory Contradiction Resolution

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Extend the existing `BeliefRepository` port with scoped contradiction lookup and
resolution methods, then implement them in the in-memory adapter. The resolver
updates only the contradiction review record: targets and source evidence stay
unchanged.

Tempted to add a contradiction service; declining because this slice is a
repository contract with no orchestration policy yet. Tempted to retract target
beliefs automatically; declining because resolution side effects need a separate
policy and evaluation spec.

## Constraints

- No public v1 schema changes.
- No target record mutation.
- No model, embedding, vector, SQL, scheduler, runtime, or TypeScript
  dependency.
- Keep repository behavior in `belief.rs`; do not grow crate root or service
  modules.

## Construction tests

**Integration tests:**

- Scoped lookup returns stored contradictions inside scope and hides records
  outside scope.
- Resolution updates status, resolution, and `updated_at` while preserving
  targets and detection provenance.
- Cross-scope resolution returns `NotFound` and leaves the original record open.
- Resolution does not mutate a target belief.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

`BeliefRepository` adds `get_contradiction` and `resolve_contradiction`. The
resolution method accepts a domain `ContradictionResolution` and returns the
updated contradiction record.

### Component / module decomposition

- `engram-core/src/lib.rs` owns the port method signatures.
- `engram-store-memory/src/belief.rs` owns scoped lookup and resolution update.
- `belief_repository.rs` owns deterministic repository tests.

### Failure, edge cases & resilience

Out-of-scope or missing contradictions return `CoreError::NotFound`. Resolution
kind maps to status deterministically: manual ignore becomes `Ignored`, needs
more evidence remains `Open`, and other resolution kinds become `Resolved`.

## Tasks

### T1: Port and in-memory repository update

**Depends on:** existing belief repository baseline.

**Tests:**
- Scoped lookup returns a stored contradiction.
- Cross-scope lookup hides a stored contradiction.
- Cross-scope resolution returns `NotFound`.

**Approach:**
- Add methods to `BeliefRepository`.
- Implement lookup and resolution in the focused in-memory belief module.

**Done when:** belief repository tests compile and pass.

### T2: Resolution behavior coverage

**Depends on:** T1.

**Tests:**
- Resolved records preserve targets/provenance and update resolution/status.
- Target beliefs are unchanged after resolving their contradiction.

**Approach:**
- Add deterministic helper fixtures in `belief_repository.rs`.
- Assert the contradiction record is the only changed object.

**Done when:** focused tests and full repository gates pass.

## Rollout

Library code only. Higher-level review APIs, automatic retraction, and
contradiction-aware ranking remain future slices.

## Risks

- A reviewer may expect resolution to mutate target beliefs. This slice keeps
  mutation out of scope and makes the behavior explicit in tests.
- `NeedsMoreEvidence` can carry a resolution note while status remains `Open`.
  This preserves the review outcome without pretending the contradiction is
  resolved.

## Changelog

- 2026-06-30: initial plan for in-memory contradiction resolution.
- 2026-06-30: shipped scoped contradiction lookup and explicit resolution on
  the in-memory belief repository.
