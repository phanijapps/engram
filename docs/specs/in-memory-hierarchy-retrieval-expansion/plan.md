# Plan: In-Memory Hierarchy Retrieval Expansion

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Extend the focused hierarchy retrieval module to produce additional memory
candidate results from sibling memory-backed base nodes under the same parent as
a directly matched memory result. Retrieval orchestration keeps snapshotting and
fusion; hierarchy retrieval owns graph lookup, deduplication, policy-safe memory
conversion, expansion explanations, and omitted results.

Tempted to add aggregate construction or parent-summary candidates; declining
because construction and summary policy are separate roadmap slices. Tempted to
expand arbitrarily through hierarchy relations; declining because broad graph
traversal needs its own recall and leakage specification.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` unchanged.
- Keep expansion limited to memory-backed base-node siblings.

## Construction tests

**Unit/integration tests:**

- Hierarchical mode expands from a matched memory to a sibling memory under the
  same aggregate parent.
- Keyword-only mode does not expand.
- Out-of-scope and policy-denied sibling memories do not leak.
- Directly matched memories are not duplicated by expansion.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

No trait, schema, or TypeScript API changes. The existing
`RetrievalResult`, `RetrievalScore`, `RetrievalExplanation`, and `FusionTrace`
fields carry the expansion signal.

### Component / module decomposition

- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) owns expansion lookup,
  memory candidate conversion, deduplication, and hierarchy context annotation.
- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) snapshots memory records and
  hierarchy nodes, invokes expansion before hierarchy annotation, and keeps
  final fusion/truncation unchanged.
- Tests live in
  the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`).

### Failure, edge cases & resilience

Missing parents or sibling memory records are skipped. Policy denials become
omissions. Parent cycles are not traversed for expansion; only one sibling
level is used.

## Tasks

### T1: Add sibling expansion candidates

**Depends on:** PHASE29 hierarchy retrieval context.

**Tests:**

- Hierarchical mode adds a sibling memory with expansion trace.
- Keyword-only mode does not expand.
- Scope and policy boundaries are enforced.

**Approach:**

- Add expansion helper to `hierarchy_retrieval.rs`.
- Reuse retrieval snapshots and existing authorizer.
- Append expansion candidates before context annotation and fusion.

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

Library code only. Aggregate construction, relation traversal, semantic
ranking, and model-assisted hierarchy summaries remain future phases.

## Risks

- One-level sibling expansion may add weakly related context; the lower score
  and explicit fusion trace keep the behavior auditable.
- Expansion depends on existing hierarchy quality and does not improve hierarchy
  construction.

## Changelog

- 2026-06-30: initial plan for in-memory hierarchy retrieval expansion.
- 2026-06-30: shipped one-level sibling expansion for memory-backed hierarchy
  base nodes with policy-safe omissions and shared fusion.
