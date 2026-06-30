# Spec: In-Memory Hierarchy Base Build

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

Engram has a concrete in-memory consolidation task for hierarchy build that
materializes deterministic base hierarchy nodes for scoped active memories.
Each created node points back to its memory target, preserves provenance and
policy, and is auditable through a `MemoryEventKind::HierarchyBuilt` event.

## Boundaries

### Always do

- Apply base-node construction only when `ConsolidationTaskKind::HierarchyBuild`
  is planned.
- Restrict reads and writes to memories allowed by the request scope.
- Create at most one active base hierarchy node per memory target.
- Preserve memory policy and scope on the created hierarchy node.
- Append one `MemoryEventKind::HierarchyBuilt` event for each created node.
- Report completed and skipped tasks through consolidation task results.

### Ask first

- Add clustering, parent aggregation, taxonomy evolution, summaries, embeddings,
  model calls, relation inference, or retrieval expansion.
- Change the public hierarchy domain contract.
- Build hierarchy nodes for knowledge chunks, beliefs, entities, or concepts.

### Never do

- Mutate records outside the requested scope.
- Create duplicate active base nodes for the same memory target.
- Build hierarchy nodes for inactive, expired, archived, redacted, or forgotten
  memories.
- Put concrete hierarchy construction algorithms into `engram-core`.
- Synthesize parent-child relations without a separate spec.

## Testing Strategy

- TDD: adapter tests seed active, inactive, duplicate-covered, and out-of-scope
  memories, execute the gated mutating consolidation service, and assert node
  creation, path navigation, events, stats, and skipped planned tasks.
- Goal-based: repository gates prove no public contract drift and no generated
  TypeScript changes are required.

## Acceptance Criteria

- [x] Scoped active memories without existing base nodes receive base hierarchy
  nodes.
- [x] Each created base node has `sourceTargetType=memory`,
  `sourceTargetId=<memory id>`, layer `0`, active status, preserved scope, and
  preserved policy.
- [x] Existing active base nodes prevent duplicate node creation.
- [x] Inactive or expired memories do not receive hierarchy nodes.
- [x] Out-of-scope memories are not read into the hierarchy build output.
- [x] Each created node has a `HierarchyBuilt` event on the source memory.
- [x] Unsupported planned tasks are reported as skipped without hidden side
  effects.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: hierarchy nodes can already point to retrieval targets through
  `source_target_type` and `source_target_id` (source:
  `core/domain/src/hierarchy.rs`).
- Technical: in-memory hierarchy repository and path navigation already exist
  (source: `adapters/memory/inmem/src/hierarchy.rs`).
- Technical: mutating consolidation is already gated by
  `GatedConsolidationService` and an injected `ConsolidationMutationExecutor`
  (source: `core/orchestration/src/consolidation/mutating.rs`).
- Process: concrete hierarchy algorithms remain outside `engram-core` (source:
  `docs/implementation-roadmap.md`).
