# Spec: Belief Network

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** data

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can persist reviewable beliefs and contradictions as derived records over
evidence without treating them as memories, knowledge chunks, or source truth.
The first slice proves repository boundaries, scope isolation, evidence-bearing
record shapes, and distinct contradiction review records.

## Boundaries

### Always do

- Preserve belief source evidence and contradiction targets.
- Keep beliefs and contradictions distinct from memory and knowledge records.
- Keep storage behind `BeliefRepository`.

### Ask first

- Add automatic synthesis, contradiction detection, propagation, or confidence
  decay.
- Include beliefs in retrieval by default.
- Promote belief contracts into accepted v1 schemas.

### Never do

- Mutate source memories, chunks, or entities when writing a belief.
- Resolve contradictions automatically without a review record.
- Treat belief confidence as source truth.
- Add model-provider dependencies to core/domain or the repository adapter.

## Testing Strategy

- Repository behavior: TDD through in-memory tests for belief and contradiction
  persistence.
- Scope isolation: TDD through cross-workspace retrieval checks.
- Contract hygiene: goal-based Rust, contract, code-doc, and TypeScript gates.

## Acceptance Criteria

- [x] In-memory storage persists beliefs through `BeliefRepository`.
- [x] In-memory storage persists contradictions through `BeliefRepository`.
- [x] Belief records retain source evidence, confidence, status, policy,
  provenance, and scope.
- [x] Contradiction records retain targets, status, severity, provenance, and
  scope.
- [x] Repository implementation does not mutate memory, knowledge, or hierarchy
  state.

## Assumptions

- Technical: domain belief types already model beliefs, sources, contradictions,
  targets, and resolutions (source: `crates/engram-domain/src/belief.rs`).
- Technical: `engram-core` already exposes `BeliefRepository` for belief and
  contradiction writes (source: `crates/engram-core/src/lib.rs`).
- Research: source attribution and confidence calibration are central to
  derived beliefs (source: `docs/research/memory-knowledge-architecture.md`).
- Process: derived behavior stays outside `engram-domain` and storage stays
  behind adapters (source: `AGENTS.md`).
- Product: first belief slice is repository behavior, not autonomous synthesis
  (source: user confirmation 2026-06-29).
