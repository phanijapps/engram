# Spec: In-Memory Consolidation Compaction

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

Engram has a concrete in-memory consolidation executor for compaction. For an
explicit mutating consolidation request, exact duplicate active memories inside
the requested scope are compacted by preserving the earliest record and
archiving later duplicates. The executor reports all task outcomes through
`ConsolidationMutationOutcome` and records a `Consolidated` memory event for
each archived duplicate.

## Boundaries

### Always do

- Apply compaction only when `ConsolidationTaskKind::Compaction` is planned.
- Restrict duplicate detection and mutation to memories allowed by the request
  scope.
- Preserve one canonical active record for each duplicate content group.
- Archive duplicate records instead of deleting or redacting content.
- Append one `MemoryEventKind::Consolidated` event for each archived duplicate.
- Report completed and skipped tasks through consolidation task results.

### Ask first

- Add summarization, model calls, embeddings, hierarchy rebuild, belief
  synthesis, contradiction detection, decay, pruning, or schedulers.
- Change duplicate grouping beyond exact normalized memory text.
- Change public v1 JSON schemas or domain contract fields.

### Never do

- Mutate records outside the requested scope.
- Compact inactive, redacted, forgotten, archived, or expired records.
- Hard-delete memory records during compaction.
- Put concrete compaction algorithms into `engram-core`.
- Grow `engram-store-memory` crate roots or service entry points into god
  modules.

## Testing Strategy

- TDD: adapter tests seed duplicate and non-duplicate memories, execute the
  gated mutating consolidation service with the in-memory executor, and assert
  status changes, event audit records, scope isolation, and task stats.
- Goal-based: repository gates prove no public contract drift and no generated
  TypeScript changes are required.

## Acceptance Criteria

- [x] Duplicate active memories with the same normalized text in the same
  request scope leave the earliest record active and archive later records.
- [x] Each archived duplicate has a `Consolidated` event whose payload names the
  preserved memory and consolidation reason.
- [x] Memories outside the request scope are not mutated even when their content
  is identical.
- [x] Non-duplicate active memories remain active and produce zero archive
  events.
- [x] Unsupported planned tasks are reported as skipped without hidden side
  effects.
- [x] The executor returns consolidation stats and task counters that match the
  records read, updated, skipped, and pruned.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: mutating consolidation is already gated by
  `GatedConsolidationService` and an injected `ConsolidationMutationExecutor`
  (source: `core/orchestration/src/consolidation/mutating.rs`).
- Technical: in-memory storage exposes `MemoryStatus::Archived` and
  `MemoryEventKind::Consolidated` for auditable compaction (source:
  `core/domain/src/memory.rs`).
- Technical: scope matching already exists in the in-memory adapter and should
  be reused for mutation boundaries (source:
  the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`)).
- Process: concrete consolidation algorithms remain outside `engram-core`
  (source: `docs/implementation-roadmap.md`).
