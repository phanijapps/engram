# Spec: In-Memory Hierarchy Retrieval Context

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

When a retrieval request includes hierarchical mode, the in-memory adapter uses
existing hierarchy nodes to annotate matching memory results with path evidence
and a hierarchical-fit score signal. Hierarchy context remains explanatory; it
does not create new memories, mutate hierarchy state, or perform aggregate
clustering.

## Boundaries

### Always do

- Apply hierarchy context only when `RetrievalMode::Hierarchical` is requested.
- Restrict hierarchy nodes by request scope before using them.
- Match active base hierarchy nodes to retrieval results by target type and
  target ID.
- Add parent-chain path evidence to result explanations when explanations are
  requested.
- Set `RetrievalScore.hierarchicalFit` for hierarchy-backed results before
  shared fusion.

### Ask first

- Add new public retrieval fields or generated TypeScript.
- Create hierarchy nodes or relations during retrieval.
- Expand retrieval to sibling, parent, or aggregate members.
- Add model-assisted hierarchy summaries or vector hierarchy ranking.

### Never do

- Leak out-of-scope hierarchy paths into result explanations.
- Mutate memories, hierarchy nodes, or relations during retrieval.
- Put hierarchy-context behavior into `engram-core`.

## Testing Strategy

- TDD: adapter tests seed memory-backed base nodes and aggregate parents, then
  assert hierarchical retrieval adds path evidence and hierarchical-fit scoring.
- Regression: keyword-only retrieval remains unchanged without hierarchical
  mode.
- Goal-based: repository gates prove no public contract drift.

## Acceptance Criteria

- [x] Hierarchical retrieval annotates matching memory results with hierarchy
  path evidence.
- [x] Hierarchical retrieval sets `hierarchicalFit` without changing target
  type or target ID.
- [x] Keyword-only retrieval does not add hierarchy path evidence.
- [x] Out-of-scope hierarchy nodes do not affect retrieval results.
- [x] No hierarchy state is mutated during retrieval.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: base hierarchy nodes already record `sourceTargetType` and
  `sourceTargetId` for memory records (source:
  `adapters/memory/inmem/src/consolidation/hierarchy_build.rs`).
- Technical: retrieval explanations already carry a string `path` field (source:
  `core/domain/src/retrieval.rs`).
- Technical: hierarchy path navigation is repository-local and scoped (source:
  `adapters/memory/inmem/src/hierarchy.rs`).
