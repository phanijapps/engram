# Spec: In-Memory Hierarchy Retrieval Expansion

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

When a retrieval request includes hierarchical mode, the in-memory adapter can
expand from a matched memory-backed hierarchy base node to sibling memory-backed
base nodes under the same scoped parent. Expanded results remain memory
retrieval results, carry hierarchy expansion trace evidence, and pass the same
scope, lifecycle, policy, and filter checks as directly matched memory results.

## Boundaries

### Always do

- Expand only when `RetrievalMode::Hierarchical` is requested.
- Use existing active hierarchy nodes; do not create or mutate hierarchy state.
- Restrict expansion to request-scoped base nodes with
  `sourceTargetType=memory`.
- Re-apply memory scope, lifecycle, policy, authorizer, time, kind, archive, and
  confidence checks before returning expanded memory results.
- Deduplicate expanded memories against already matched or already expanded
  results.
- Preserve expansion evidence through `FusionTrace.source="hierarchy.expansion"`
  and `RetrievalScore.hierarchicalFit`.

### Ask first

- Add new public retrieval fields, schemas, or generated TypeScript.
- Expand through arbitrary graph relations, ancestors, descendants, or concepts.
- Expand non-memory targets.
- Add vector, model, or learned ranking for hierarchy expansion.

### Never do

- Leak out-of-scope or policy-denied related memories.
- Treat hierarchy expansion as source truth or mutate source memories.
- Put hierarchy expansion behavior into `engram-core`.

## Testing Strategy

- TDD: adapter tests seed sibling memory-backed base nodes, retrieve with
  hierarchical mode, and assert related memories are added with expansion trace.
- Regression: keyword-only retrieval does not expand.
- TDD: policy-denied and out-of-scope siblings do not leak.
- Goal-based: repository gates prove no public contract drift.

## Acceptance Criteria

- [x] Hierarchical retrieval expands from a matched memory base node to scoped
  sibling memory base nodes.
- [x] Expanded results use `RetrievalTargetType::Memory` and keep the sibling
  memory ID as `targetId`.
- [x] Expanded results include `hierarchy.expansion` fusion trace evidence and
  `hierarchicalFit`.
- [x] Keyword-only retrieval does not expand.
- [x] Policy-denied and out-of-scope sibling memories do not leak.
- [x] Expanded memories are deduplicated against direct matches.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: hierarchy base nodes already link back to memory targets through
  `sourceTargetType` and `sourceTargetId` (source:
  `core/domain/src/hierarchy.rs`).
- Technical: in-memory retrieval already snapshots memories and hierarchy nodes
  before fusion (source: `adapters/memory/inmem/src/retrieval.rs`).
- Technical: weighted fusion can rank mixed direct and expanded candidates
  using existing `FusionTrace` and `RetrievalScore` fields (source:
  `core/retrieval/src/lib.rs`).
- Process: expansion must stay adapter-local and keep `engram-core` free of
  concrete hierarchy algorithms (source: `AGENTS.md`).
