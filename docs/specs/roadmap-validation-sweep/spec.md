# Spec: Roadmap Validation Sweep

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003
- **Brief:** none
- **Contract:** none
- **Shape:** validation sweep

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram's current implementation roadmap loop has a recorded full validation
sweep after the near-term queue items were completed. The phase ledger should
show the validation sweep as done instead of leaving a stale queue item.

## Boundaries

### Always do

- Run Rust workspace checks.
- Run TypeScript generation, typecheck, and tests.
- Run contract and documentation hooks.
- Confirm generated files do not drift.

### Ask first

- Add new runtime behavior.
- Change public schemas or generated TypeScript contracts.
- Remove validation gates.

### Never do

- Mark the roadmap loop validated while generated files are dirty.
- Hide failing checks behind documentation-only status.
- Stage unrelated local Codex assets.

## Testing Strategy

- Goal-based: run the full validation commands listed in the plan.
- Regression: confirm `git diff --check` and worktree drift checks are clean.

## Acceptance Criteria

- [x] Rust formatting, workspace check, and workspace tests pass.
- [x] TypeScript contract generation, typecheck, and tests pass.
- [x] Contract and documentation hooks pass.
- [x] FastEmbed sqlite-vec feature compile and clippy gates pass.
- [x] Generated files have no tracked drift.
- [x] Implementation roadmap near-term queue is cleared.
