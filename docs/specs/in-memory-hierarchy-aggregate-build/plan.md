# Plan: In-Memory Hierarchy Aggregate Build

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Keep base-node construction in `hierarchy_build.rs` and add a focused
`hierarchy_aggregate.rs` consolidation collaborator. The `HierarchyBuild` task
will create missing base nodes first, then build deterministic entity aggregate
nodes from scoped memory-backed base nodes and attach eligible base nodes to
their aggregate.

Tempted to cluster by keywords from free text; declining because explicit
entities provide the only stable deterministic grouping surface. Tempted to
generate aggregate summaries; declining because model summaries and summary
policy need their own spec.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep aggregate construction adapter-local.
- Keep base-node construction and aggregate construction in separate modules.

## Construction tests

**Unit/integration tests:**

- Two scoped memories sharing the same first entity produce one active aggregate
  with parent links, memberships, events, and counters.
- Re-running consolidation creates no duplicate aggregate or events.
- Singleton, entity-less, expired, and out-of-scope base nodes are skipped.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

No trait, schema, or TypeScript API changes. Existing `HierarchyNode`,
`HierarchyMembership`, `MemoryEvent`, and `ConsolidationTaskResult` fields carry
the behavior.

### Component / module decomposition

- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) remains
  focused on base-node construction.
- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) owns
  entity grouping, aggregate node creation, base-node parent assignment,
  membership construction, audit events, and aggregate counters.
- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) composes both helpers
  under the existing `HierarchyBuild` task.
- Tests live in
  the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`).

### Failure, edge cases & resilience

Missing source memories, inactive memories, expired memories, base nodes outside
scope, singleton groups, and entity-less records are skipped. Existing active
aggregate metadata prevents duplicate aggregate creation.

## Tasks

### T1: Build deterministic entity aggregates

**Depends on:** PHASE25 base-node construction and PHASE30 hierarchy retrieval
expansion.

**Tests:**

- Aggregate creation and parent/membership wiring.
- Idempotent re-run.
- Skip singleton, entity-less, expired, and out-of-scope groups.

**Approach:**

- Add `hierarchy_aggregate.rs`.
- Compose it after `build_base_nodes`.
- Report combined hierarchy task counters.

**Done when:** focused adapter tests and adjacent hierarchy tests pass.

### T2: Document and wire phase status

**Depends on:** T1.

**Tests:**

- Full repository gates pass.
- `docs/implementation/phases.json` marks the phase done after gates pass.

**Approach:**

- Update roadmap status documents and changelog.
- Run the repository validation suite before commit.

**Done when:** docs, phase JSON, and code are committed together.

## Rollout

Library code only. Keyword clustering, semantic clustering, model summaries,
relationship inference, and durable SQL hierarchy build behavior remain future
phases.

## Risks

- First-entity grouping is simple and may be coarse; this is intentional until
  semantic clustering has a separate quality spec.
- Parent assignment mutates existing base hierarchy nodes, so tests must prove
  idempotency and scope isolation.

## Changelog

- 2026-06-30: initial plan for deterministic in-memory entity aggregate
  hierarchy construction.
- 2026-06-30: shipped deterministic first-entity aggregate hierarchy build with
  parent links, memberships, audit events, and idempotency coverage.
