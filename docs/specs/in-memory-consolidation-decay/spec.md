# Spec: In-Memory Consolidation Decay

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

Engram has a concrete in-memory consolidation executor for decay. For an
explicit mutating consolidation request, active memories inside the requested
scope whose policy expiry is due are marked expired, unless they are under
legal hold. Each durable mutation is reported through
`ConsolidationMutationOutcome` and recorded as a `MemoryEventKind::Expired`
event.

## Boundaries

### Always do

- Apply decay only when `ConsolidationTaskKind::Decay` is planned.
- Restrict decay reads and mutations to memories allowed by the request scope.
- Mark due active records as `MemoryStatus::Expired`; do not erase content.
- Append one `MemoryEventKind::Expired` event for each expired record.
- Skip legal-hold records even when `expiresAt` is in the past.
- Report completed and skipped tasks through consolidation task results.

### Ask first

- Add time-weighted scoring, confidence decay, summarization, pruning,
  retention cleanup, schedulers, or model calls.
- Change policy semantics for legal hold or delete modes.
- Change public v1 JSON schemas or domain contract fields.

### Never do

- Mutate records outside the requested scope.
- Mutate archived, redacted, forgotten, or already expired records.
- Hard-delete, redact, archive, or compact records during decay.
- Put concrete decay algorithms into `engram-core`.
- Treat legal hold as advisory.

## Testing Strategy

- TDD: adapter tests seed expired, future-expiring, legal-hold, and
  out-of-scope memories, execute the gated mutating consolidation service, and
  assert status changes, events, stats, and skipped planned tasks.
- Goal-based: repository gates prove no public contract drift and no generated
  TypeScript changes are required.

## Acceptance Criteria

- [x] Due active memories in the request scope are marked expired.
- [x] Each expired memory has an `Expired` event whose payload names the policy
  expiry and decay reason.
- [x] Future-expiring memories remain active.
- [x] Legal-hold memories remain active even when their expiry is due.
- [x] Out-of-scope memories are not mutated.
- [x] Unsupported planned tasks are reported as skipped without hidden side
  effects.
- [x] The executor returns consolidation stats and task counters that match the
  records read, updated, skipped, and decayed.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: mutating consolidation is already gated by
  `GatedConsolidationService` and an injected `ConsolidationMutationExecutor`
  (source: `core/orchestration/src/consolidation/mutating.rs`).
- Technical: policy expiry and legal-hold retention are domain contract fields
  (source: `core/domain/src/policy.rs`).
- Technical: in-memory storage exposes `MemoryStatus::Expired` and
  `MemoryEventKind::Expired` for auditable decay (source:
  `core/domain/src/memory.rs`).
- Process: concrete consolidation algorithms remain outside `engram-core`
  (source: `docs/implementation-roadmap.md`).
