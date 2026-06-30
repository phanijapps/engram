# Spec: In-Memory Belief Assertion Synthesis

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

Engram has a concrete in-memory consolidation task for belief synthesis that
turns explicit memory assertions into derived belief records. Each synthesized
belief remains traceable to the source memory/assertion, avoids duplicate active
beliefs for the same assertion target, and records a `BeliefSynthesized` event
on the source memory.

## Boundaries

### Always do

- Apply assertion-based synthesis only when `ConsolidationTaskKind::BeliefSynthesis`
  is planned.
- Restrict reads and writes to memories allowed by the request scope.
- Synthesize beliefs only from explicit `MemoryAssertion` values.
- Preserve source memory scope and policy on created beliefs.
- Append one `MemoryEventKind::BeliefSynthesized` event for each created belief.
- Report completed and skipped tasks through consolidation task results.

### Ask first

- Add model inference, contradiction detection, belief merging, confidence
  propagation, entity resolution, or retrieval over beliefs.
- Synthesize beliefs from free text without an explicit assertion.
- Change the public belief domain contract.

### Never do

- Mutate source memories while synthesizing beliefs.
- Create duplicate active beliefs for the same memory assertion target.
- Synthesize from inactive, expired, archived, redacted, or forgotten memories.
- Put concrete belief synthesis algorithms into `engram-core`.
- Treat synthesized beliefs as source truth.

## Testing Strategy

- TDD: adapter tests seed asserted, duplicate-covered, inactive, and
  out-of-scope memories, execute the gated mutating consolidation service, and
  assert belief creation, events, stats, and skipped planned tasks.
- Goal-based: repository gates prove no public contract drift and no generated
  TypeScript changes are required.

## Acceptance Criteria

- [x] Scoped active memory assertions produce active belief records.
- [x] Each belief names its source assertion and source memory.
- [x] Existing active beliefs prevent duplicate synthesis for the same assertion
  target.
- [x] Memories without assertions and inactive or expired memories are skipped.
- [x] Out-of-scope memories are not read into synthesis output.
- [x] Each created belief has a `BeliefSynthesized` event on the source memory.
- [x] Unsupported planned tasks are reported as skipped without hidden side
  effects.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: `MemoryAssertion` already carries subject, predicate, object, and
  confidence fields (source: `core/domain/src/memory.rs`).
- Technical: belief records already carry sources and provenance separately
  from source memories (source: `core/domain/src/belief.rs`).
- Technical: mutating consolidation is already gated by
  `GatedConsolidationService` and an injected `ConsolidationMutationExecutor`
  (source: `core/orchestration/src/consolidation/mutating.rs`).
- Process: beliefs remain derived stance, not source truth (source:
  `docs/implementation-roadmap.md`).
