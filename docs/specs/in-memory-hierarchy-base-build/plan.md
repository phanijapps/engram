# Plan: In-Memory Hierarchy Base Build

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a focused hierarchy-build task module under the in-memory consolidation
executor. The task scans scoped active memories, skips memories that already
have an active base hierarchy node, creates one layer-0 base node for each
remaining memory, and appends hierarchy-built memory events.

Tempted to build topic aggregates or broader relations; declining because base
nodes are the minimum durable hierarchy substrate. Tempted to add a generic
builder trait adapter; declining because `engram-core` already has the trait and
this slice only wires one in-memory consolidation task.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` free of concrete task algorithms.
- Keep base-node construction separate from clustering and retrieval expansion.

## Construction tests

**Unit/integration tests:**

- Scoped active memories receive base hierarchy nodes and hierarchy-built
  events.
- Existing base nodes prevent duplicate creation.
- Expired and out-of-scope memories are skipped.
- Created nodes can be found through `HierarchyRepository::path_for`.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

`InMemoryConsolidationExecutor` continues to implement the existing
`ConsolidationMutationExecutor` port. No new public JSON schema or TypeScript
contract is introduced.

### Component / module decomposition

- `adapters/memory/inmem/src/consolidation/hierarchy_build.rs` owns
  base-node construction and event creation.
- `adapters/memory/inmem/src/consolidation/mod.rs` only routes the planned
  task kind to the focused module.
- Tests live in `adapters/memory/inmem/tests/consolidation_hierarchy_build.rs`.

### Failure, edge cases & resilience

The build is idempotent for active base nodes. Expired memories are skipped
using the run start timestamp so the scan has one stable time boundary.

## Tasks

### T1: Add focused in-memory hierarchy base-node build

**Depends on:** in-memory hierarchy repository and PHASE24 split executor.

**Tests:**

- Base nodes and events are created for scoped active memories.
- Duplicate, expired, and out-of-scope candidates are skipped.

**Approach:**

- Extend the executor's planned-task match with `HierarchyBuild`.
- Reuse `scope_allows` and existing service state lock.
- Store nodes directly in in-memory hierarchy state and event audit state.

**Done when:** adapter hierarchy-build tests pass.

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

Library code only. Aggregate hierarchy construction, relation inference,
retrieval expansion, and model-assisted summaries remain future phases.

## Risks

- Base nodes alone do not improve retrieval; they create the durable substrate
  for later hierarchy expansion.
- Event output references cannot directly target hierarchy nodes in the current
  `EvidenceTargetType`; task output references point at source memories.

## Changelog

- 2026-06-30: initial plan for deterministic in-memory hierarchy base nodes.
- 2026-06-30: shipped scoped memory base-node construction with duplicate
  prevention, hierarchy-built events, and path navigation tests.
