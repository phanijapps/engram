# Plan: In-Memory Belief Assertion Synthesis

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a focused belief-synthesis task module under the in-memory consolidation
executor. The task scans scoped active memories, converts explicit assertions
into active beliefs, skips assertion targets that already have active beliefs,
and appends belief-synthesized memory events.

Tempted to infer beliefs from text; declining because explicit assertions are
the only deterministic evidence shape in the current contract. Tempted to add
contradiction detection; declining because synthesis and contradiction review
are separate lifecycle tasks.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` free of concrete task algorithms.
- Keep belief synthesis separate from contradiction detection and belief
  retrieval.

## Construction tests

**Unit/integration tests:**

- Scoped active assertions create beliefs and belief-synthesized events.
- Existing active beliefs prevent duplicate synthesis.
- Memories without assertions, expired memories, and out-of-scope memories are
  skipped.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

`InMemoryConsolidationExecutor` continues to implement the existing
`ConsolidationMutationExecutor` port. No new public JSON schema or TypeScript
contract is introduced.

### Component / module decomposition

- `adapters/memory/inmem/src/consolidation/belief_synthesis.rs` owns
  assertion-to-belief mapping and event creation.
- `adapters/memory/inmem/src/consolidation/mod.rs` only routes the planned
  task kind to the focused module.
- Tests live in `adapters/memory/inmem/tests/consolidation_belief_synthesis.rs`.

### Failure, edge cases & resilience

Assertion target IDs are deterministic: `<memory-id>#assertion-<index>`. Expired
memories are skipped using the run start timestamp.

## Tasks

### T1: Add focused in-memory assertion belief synthesis

**Depends on:** in-memory belief repository and PHASE25 split executor.

**Tests:**

- Assertion-backed beliefs and events are created for scoped active memories.
- Duplicate, no-assertion, expired, and out-of-scope candidates are skipped.

**Approach:**

- Extend the executor's planned-task match with `BeliefSynthesis`.
- Reuse `scope_allows` and existing service state lock.
- Store beliefs directly in in-memory belief state and event audit state.

**Done when:** adapter belief-synthesis tests pass.

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

Library code only. Model inference, contradiction detection, belief retrieval,
and confidence propagation remain future phases.

## Risks

- Assertion-backed beliefs are only as good as the explicit assertions provided;
  this slice does not infer missing assertions from text.
- There is no public belief read API yet, so tests inspect effects through
  repository state indirectly where available and event audit records.

## Changelog

- 2026-06-30: initial plan for deterministic assertion-backed belief synthesis.
- 2026-06-30: shipped assertion-backed belief synthesis with duplicate
  prevention, source-memory audit events, and scoped adapter tests.
