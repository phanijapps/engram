# Spec: Mutating Consolidation Gates

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

Engram can execute explicitly requested mutating consolidation through a gated
service that runs protected evaluation before and after durable work, records
all task outcomes in a `ConsolidationRun`, and keeps concrete mutation
algorithms behind an injected executor port.

## Boundaries

### Always do

- Require explicit `dryRun=false` before the mutating service executes tasks.
- Run a protected evaluation fixture before invoking any mutation executor.
- Run the same protected evaluation fixture after the mutation executor returns.
- Return an auditable `ConsolidationRun` for successful, failed, and
  regression-detected cycles.
- Keep mutation task execution behind a focused Rust trait.

### Ask first

- Implement actual summarization, pruning, decay, hierarchy rebuild, belief
  synthesis, or contradiction detection algorithms.
- Add schedulers, background workers, model providers, or repository adapters to
  `engram-core`.
- Change public v1 JSON schemas.

### Never do

- Execute mutation when the pre-evaluation gate fails.
- Hide post-mutation evaluation regressions behind a successful run status.
- Treat dry-run and mutating services as interchangeable.
- Let the core crate own concrete store, vector, model, scheduler, or runtime
  dependencies.

## Testing Strategy

- TDD: core service tests cover successful pre/evaluate/mutate/evaluate order,
  pre-gate failure, post-gate regression reporting, and explicit mutating-mode
  validation.
- Goal-based: full repository gates prove no public contract drift.

## Acceptance Criteria

- [x] A mutating consolidation service rejects requests unless `dryRun=false`
  is explicit.
- [x] A failed pre-evaluation returns a failed run and does not call the
  mutation executor.
- [x] Successful mutation is wrapped by pre- and post-evaluation gates.
- [x] A failed post-evaluation returns a non-successful auditable run with
  regression evidence.
- [x] The implementation introduces no concrete store, model, scheduler,
  vector, runtime, or TypeScript dependency.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: `ConsolidationRun` and task/error shapes already exist in the
  domain contract (source: `core/domain/src/operations.rs`).
- Technical: `EvaluationRunner` already exists as the protected-fixture gate
  boundary (source: `core/orchestration/src/lib.rs`).
- Process: durable consolidation mutations must be auditable and evaluation
  gated (source: `docs/implementation-roadmap.md`).
