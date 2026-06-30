# Plan: In-Memory Hierarchy Retrieval Context

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a focused hierarchy retrieval helper in `engram-store-memory`. Retrieval
will snapshot hierarchy nodes, build scoped parent chains for matching memory
results, and annotate those results before shared fusion. The helper does not
create candidates or mutate hierarchy state.

Tempted to expand from a matched memory to sibling or parent members; declining
because expansion can affect recall and ranking, so it needs a separate spec.
Tempted to synthesize aggregate summaries; declining because summaries need
model or deterministic aggregation policy.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` unchanged.
- Keep hierarchy retrieval context separate from hierarchy construction and
  aggregate clustering.

## Construction tests

**Unit/integration tests:**

- Hierarchical mode annotates matching memory results with path evidence and
  `hierarchicalFit`.
- Keyword-only mode leaves path evidence empty.
- Out-of-scope hierarchy nodes do not annotate results.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

No trait, schema, or TypeScript API changes. The existing
`RetrievalExplanation.path` and `RetrievalScore.hierarchical_fit` fields carry
the new explanatory signal.

### Component / module decomposition

- `adapters/memory/inmem/src/hierarchy_retrieval.rs` owns hierarchy-path
  annotation.
- `adapters/memory/inmem/src/retrieval.rs` snapshots hierarchy nodes and
  invokes the helper before fusion.
- Tests live in
  `adapters/memory/inmem/tests/hierarchy_retrieval.rs`.

### Failure, edge cases & resilience

Missing hierarchy nodes are a no-op. Parent cycles are bounded by visited-node
tracking. Out-of-scope nodes are ignored before path construction.

## Tasks

### T1: Annotate retrieval results with hierarchy context

**Depends on:** existing hierarchy repository support and in-memory retrieval
fusion.

**Tests:**

- Hierarchical mode annotation.
- Keyword-only no-op.
- Scope isolation.

**Approach:**

- Create focused hierarchy retrieval helper.
- Snapshot hierarchy nodes from in-memory state.
- Apply hierarchy context to memory candidates before fusion.

**Done when:** focused adapter tests pass.

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

Library code only. Hierarchical expansion, aggregate construction,
model-assisted summaries, and graph traversal remain future phases.

## Risks

- Path evidence can look more authoritative than the underlying base hierarchy;
  result type and provenance still identify the returned memory as the source.
- This slice does not improve recall by itself; it only makes hierarchy impact
  visible when requested.

## Changelog

- 2026-06-30: initial plan for in-memory hierarchy retrieval context.
- 2026-06-30: shipped hierarchy-mode retrieval annotations for memory results
  with scoped path evidence and hierarchical-fit scoring.
