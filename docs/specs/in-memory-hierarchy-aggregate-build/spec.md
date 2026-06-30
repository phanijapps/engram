# Spec: In-Memory Hierarchy Aggregate Build

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

Hybrid consolidation builds deterministic layer-1 hierarchy aggregate nodes for
memory-backed base nodes that share the same first source-memory entity. Each
aggregate is scoped, auditable, idempotent, and links its member base nodes
without creating model summaries, vector clusters, or new public contract
fields.

## Boundaries

### Always do

- Build aggregates only from active, scoped base hierarchy nodes whose
  `sourceTargetType` is `memory`.
- Use the source memory's first entity identifier or normalized entity name as
  the deterministic aggregate key.
- Require at least two eligible base nodes for an aggregate group.
- Create at most one active aggregate node per scope and aggregate key.
- Attach eligible base nodes to their aggregate through `parentId` and aggregate
  membership entries.
- Record `HierarchyBuilt` events on source memories when aggregate membership is
  created.

### Ask first

- Add vector clustering, LLM summaries, relation inference, or semantic topic
  modeling.
- Aggregate records without explicit source-memory entities.
- Change hierarchy schemas or generated TypeScript.
- Create aggregate nodes for non-memory targets.

### Never do

- Create duplicate active aggregate nodes for the same scope and aggregate key.
- Cross request-scope boundaries while grouping or attaching nodes.
- Mutate source memory content or policy while building aggregates.
- Put aggregate construction behavior into `engram-core`.

## Testing Strategy

- TDD: adapter tests seed entity-bearing memories, run gated hybrid
  consolidation, and assert aggregate node creation, membership, parent links,
  events, and task counters.
- Regression: repeated consolidation does not duplicate aggregate nodes or
  events.
- TDD: out-of-scope, expired, entity-less, and singleton groups are skipped.
- Goal-based: repository gates prove no public contract drift.

## Acceptance Criteria

- [x] Hybrid hierarchy build creates one aggregate node for two or more scoped
  base memory nodes sharing the first source-memory entity.
- [x] Created aggregate nodes are layer 1, active, scoped, provenance-linked,
  and carry deterministic aggregate metadata.
- [x] Member base nodes receive `parentId` links to the aggregate and aggregate
  memberships reference those base nodes.
- [x] Source memories receive `HierarchyBuilt` events for new aggregate
  membership.
- [x] Re-running consolidation is idempotent for existing aggregate groups.
- [x] Out-of-scope, expired, entity-less, and singleton groups are skipped.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: memory content already carries `entities` as structured
  `EntityRef` values (source: `core/domain/src/memory.rs`).
- Technical: hierarchy nodes already support aggregate kind, parent links, and
  membership entries (source: `core/domain/src/hierarchy.rs`).
- Technical: `HierarchyBuild` is already part of hybrid consolidation and
  adapter-local algorithms live under `engram-store-memory` (source:
  `adapters/memory/inmem/src/consolidation/mod.rs`).
- Process: no god modules; aggregate construction belongs in a focused module
  separate from base-node construction (source: `AGENTS.md`).
