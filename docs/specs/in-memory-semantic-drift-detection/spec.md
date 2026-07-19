# Spec: In-Memory Semantic Drift Detection

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** behavior

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

`InMemoryConsolidationExecutor` implements the planned
`SemanticDriftDetection` task for time-window consolidation by detecting
deterministic temporal assertion drift: the same scoped subject and predicate
receives a later explicit assertion with a different object. Detection writes an
open temporal contradiction review record and audit events without mutating
source memories or beliefs.

## Boundaries

### Always do

- Keep the algorithm deterministic and model-free.
- Use explicit memory assertions only.
- Respect request scope and active, unexpired memory status.
- Treat drift records as review records, not source-truth mutation.
- Keep duplicate open drift records idempotent by assertion pair.
- Report `SemanticDriftDetection` as completed when the task runs.

### Ask first

- Add embedding, LLM, or fuzzy semantic comparison.
- Mark older memories, assertions, or beliefs stale automatically.
- Resolve contradictions or choose a winning target.
- Change public v1 JSON schemas or generated TypeScript types.

### Never do

- Infer assertions from free text in this task.
- Write cross-scope drift records.
- Hide source memories from retrieval because drift was detected.
- Combine drift detection with decay, belief synthesis, or hierarchy building
  in one module.

## Testing Strategy

- TDD: add in-memory consolidation tests for temporal drift and idempotency.
- Regression: existing assertion contradiction detection remains exact-value
  conflict detection for hybrid consolidation.
- Goal-based: TimeWindow consolidation no longer reports semantic drift as a
  skipped task when scoped temporal drift candidates exist.

## Acceptance Criteria

- [x] TimeWindow consolidation executes `SemanticDriftDetection` in the
  in-memory executor.
- [x] A later scoped assertion with the same subject and predicate but a
  different object creates one open temporal contradiction review record.
- [x] Drift detection emits contradiction-detected events for affected memories.
- [x] Re-running the same consolidation request does not duplicate open drift
  records or events.
- [x] Cross-scope, inactive, expired, and same-object assertions are skipped.
- [x] No source memory, belief, schema, or generated contract is mutated.

## Assumptions

- Technical: `ConsolidationStrategy::TimeWindow` already plans
  `SemanticDriftDetection`.
- Technical: `ContradictionKind::Temporal` is the accepted review-record kind
  for time-ordered claim changes.
- Technical: explicit `MemoryAssertion.valid_from` and record `created_at`
  provide deterministic ordering.
