# Spec: Consolidation And Sleep Cycle

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

Engram can plan and report bounded consolidation work without hiding durable
mutations. The first sleep-cycle slice creates auditable `ConsolidationRun`
records for dry-run tasks and establishes the orchestration boundary before any
summarization, hierarchy rebuild, belief synthesis, pruning, or decay mutates
stored records.

## Boundaries

### Always do

- Return a `ConsolidationRun` for every consolidation request.
- Preserve scope, requester, trigger, task statuses, errors, stats, and dry-run
  behavior.
- Keep irreversible mutation out of the first slice.

### Ask first

- Add model calls, summarization, pruning, decay, hierarchy rebuilds, belief
  synthesis, or contradiction detection.
- Run consolidation automatically in the background.
- Mutate memories, knowledge, beliefs, hierarchy nodes, or vector indexes.

### Never do

- Hide failed tasks or partial work.
- Run unbounded consolidation across all tenants.
- Treat improved-looking summaries as safe without evaluation gates.
- Add scheduler/runtime dependencies to `engram-core` or `engram-domain`.

## Testing Strategy

- Dry-run orchestration: TDD through service tests that produce completed
  `ConsolidationRun` records with task-level results and no durable mutations.
- Scope and validation: TDD through invalid and missing-scope request tests.
- Workspace hygiene: goal-based Rust, contract, code-doc, and TypeScript gates.

## Acceptance Criteria

- [x] A consolidation service accepts bounded dry-run requests and returns a
  completed `ConsolidationRun`.
- [x] Run records include scope, requester, trigger, task results, stats, and
  recoverable errors where applicable.
- [x] Dry-run execution writes no memories, beliefs, hierarchy nodes, chunks, or
  vector rows.
- [x] Invalid requests fail before task execution.
- [x] The implementation introduces no scheduler, model provider, or background
  runtime dependency.

## Assumptions

- Technical: consolidation request and run domain types already exist (source:
  `crates/engram-domain/src/operations.rs`).
- Process: durable mutations must be explicit and auditable (source:
  `docs/implementation-roadmap.md`).
- Product: first sleep-cycle slice is dry-run planning/reporting, not automatic
  mutation (source: user confirmation 2026-06-29).
