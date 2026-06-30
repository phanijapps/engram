# Spec: In-Memory Assertion Contradiction Detection

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

Engram has a deterministic in-memory consolidation task for contradiction
detection over explicit memory assertions. When scoped active memories contain
assertions with the same subject and predicate but different object values, the
task creates reviewable contradiction records and records
`ContradictionDetected` lifecycle events on the source memories.

## Boundaries

### Always do

- Plan contradiction detection as part of hybrid consolidation.
- Detect contradictions only from explicit `MemoryAssertion` values.
- Restrict reads and writes to memories allowed by the request scope.
- Create review records without mutating source memories, assertions, or beliefs.
- Avoid duplicate open contradictions for the same assertion pair.
- Append `MemoryEventKind::ContradictionDetected` events for affected source
  memories when a new contradiction is created.

### Ask first

- Add model inference, semantic contradiction detection, natural-language claim
  extraction, contradiction resolution, or belief retraction.
- Detect contradictions from free text without explicit assertions.
- Change public v1 schemas or domain contract fields.

### Never do

- Treat a contradiction as automatically resolved.
- Mutate source records, beliefs, or hierarchy nodes while detecting.
- Create contradiction records across request-scope boundaries.
- Put concrete contradiction detection algorithms into `engram-core`.

## Testing Strategy

- TDD: adapter tests seed conflicting, compatible, duplicate-covered, expired,
  and out-of-scope assertions, execute gated hybrid consolidation, and assert
  contradiction records indirectly through idempotency, lifecycle events, stats,
  and task outputs.
- Goal-based: core dry-run tests prove hybrid planning includes contradiction
  detection without running adapter mutations.
- Goal-based: repository gates prove no public contract drift and no generated
  TypeScript changes are required.

## Acceptance Criteria

- [x] Hybrid consolidation plans `BeliefContradictionDetection`.
- [x] Conflicting scoped active assertions create an open contradiction record.
- [x] Duplicate open contradictions are not created for the same assertion pair.
- [x] Compatible assertions, expired memories, and out-of-scope memories are
  skipped.
- [x] Each new contradiction emits `ContradictionDetected` events for affected
  source memories.
- [x] Detection task stats and output refs reflect read, written, skipped, and
  detected counts.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: memory assertions carry subject, predicate, object, confidence, and
  validity fields (source: `crates/engram-domain/src/memory.rs`).
- Technical: contradiction records already model reviewable targets and open
  status (source: `crates/engram-domain/src/belief.rs`).
- Technical: hybrid consolidation is the existing strategy that groups
  compaction, belief, hierarchy, and evaluation tasks (source:
  `crates/engram-core/src/consolidation/planner.rs`).
- Process: contradictions must not silently mutate source truth (source:
  `docs/implementation-roadmap.md`).
