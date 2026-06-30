# Spec: Accepted Retrieval Fixtures

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0002, ADR-0003
- **Brief:** none
- **Contract:** `contracts/v1/schemas/evaluation-fixture.schema.json`
- **Shape:** contract fixture set

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram has accepted v1 retrieval evaluation fixtures for positive recall,
forbidden recall, budget omission, and no-result behavior. The fixtures run
through the shared `MemoryFixtureRunner` so future SQL, native, and TypeScript
paths can reuse the same contract examples.

## Boundaries

### Always do

- Store accepted fixture JSON under `contracts/v1/examples/`.
- Keep fixtures valid against the existing evaluation-fixture schema.
- Exercise retrieval through `MemoryService`, not adapter internals.
- Keep fixture IDs stable and descriptive.

### Ask first

- Change the evaluation fixture schema.
- Add generated TypeScript contract outputs.
- Add provider-backed vector, embedding, or model behavior.
- Add benchmark or release claims from fixture results.

### Never do

- Encode adapter-specific database or in-memory state in portable fixtures.
- Treat forbidden recall as a passing retrieval result.
- Hide budget omissions behind a no-result fixture.
- Change accepted write/retrieval request examples in this slice.

## Testing Strategy

- TDD: add an `engram-eval` test that loads each accepted retrieval fixture and
  runs it against `InMemoryMemoryService`.
- Regression: existing evaluation fixture runner tests continue to pass.
- Schema: run contract generation/check scripts to prove fixture shape remains
  valid.

## Acceptance Criteria

- [x] Positive recall fixture passes through `MemoryFixtureRunner`.
- [x] Forbidden recall fixture passes by excluding an out-of-scope or
  non-matching target.
- [x] Budget omission fixture passes with `maxResults`/budget-constrained
  expectations.
- [x] No-result fixture passes with an empty required result set and explicit
  exclusions for setup aliases.
- [x] All new fixtures deserialize as `EvaluationFixture`.
- [x] No schema, Rust domain, or generated TypeScript contract changes.

## Assumptions

- Technical: current `EvaluationExpectation` supports `mustInclude`,
  `mustExclude`, and `maxResults`, which are enough for this slice.
- Technical: `MemoryFixtureRunner` resolves setup memory aliases in insertion
  order, so fixtures may refer to `memory-001`, `memory-002`, and later aliases.
- Process: richer omission reason assertions can come later with evaluation
  report generation.
