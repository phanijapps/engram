# Spec: Evaluation Report Generation

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0002, ADR-0003
- **Brief:** none
- **Contract:** none
- **Shape:** evaluation utility

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can generate stable, serializable summaries from executable evaluation
fixture reports. The reporting layer makes fixture pass/fail totals and case
failures easy to emit in CI or future CLIs without changing the core
`EvaluationRunner` trait or portable v1 schemas.

## Boundaries

### Always do

- Build reports from existing `EvaluationReport` and `EvaluationCaseReport`
  values.
- Keep report generation in `engram-eval`.
- Preserve per-case failure strings.
- Keep output deterministic and serializable.

### Ask first

- Change `engram-core` report structs or traits.
- Add a CLI, file writer, or terminal formatter.
- Add JSON schema files for evaluation reports.
- Add TypeScript report APIs.

### Never do

- Re-run fixtures while summarizing existing reports.
- Hide failed cases behind only aggregate counts.
- Make adapters depend on report formatting.
- Put storage, vector, model, or runtime dependencies in `engram-eval`.

## Testing Strategy

- TDD: unit tests summarize passing and failing `EvaluationReport` values.
- Goal-based: runner tests summarize the accepted retrieval fixture set after
  execution.
- Regression: existing fixture runner behavior remains unchanged.

## Acceptance Criteria

- [x] `engram-eval` exposes serializable fixture report summary types.
- [x] A passing report summarizes passed/failed case counts correctly.
- [x] A failing report preserves fixture ID, case ID, and failure details.
- [x] A fixture-set summary aggregates multiple fixture reports.
- [x] Accepted retrieval fixture runner tests can produce a fixture-set summary.
- [x] No v1 schema, domain model, adapter behavior, or generated TypeScript
  changes.

## Assumptions

- Technical: `EvaluationReport` remains the core execution result, and
  `engram-eval` owns presentation-friendly report shapes.
- Technical: summary JSON can use Rust serde types without adding a v1 contract
  schema yet.
- Process: a future CLI can serialize these summaries without reimplementing
  pass/fail counting.
