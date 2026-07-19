# Spec: In-Memory Retrieval Fusion Wiring

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

The in-memory retrieval service composes candidate retrieval through the
`RetrievalFusion` port before final context truncation, using deterministic
weighted fusion by default and allowing tests or future adapters to inject a
different fusion collaborator.

## Boundaries

### Always do

- Keep candidate production, fusion, and context truncation as separate steps.
- Apply request limits and item budgets after fusion so omitted candidates remain
  explainable.
- Preserve existing policy, scope, and omission behavior.
- Allow dependency injection for the fusion collaborator.

### Ask first

- Add vector calls, embeddings, SQL joins, hierarchy expansion, or model
  rerankers to in-memory retrieval.
- Change public v1 retrieval schemas.
- Move concrete fusion behavior into `engram-core`.

### Never do

- Bypass policy checks before candidate fusion.
- Hide budget-exceeded candidates introduced by post-fusion ranking.
- Turn `engram-store-memory` into a hybrid retrieval god adapter.

## Testing Strategy

- TDD: in-memory retrieval tests inject a custom fusion collaborator and verify
  it controls result order before context truncation.
- Regression: existing retrieval tests continue to prove scope, policy,
  explanation, and budget behavior.
- Goal-based: full repository gates prove no public contract drift.

## Acceptance Criteria

- [x] In-memory retrieval invokes a `RetrievalFusion` collaborator.
- [x] Default construction uses deterministic weighted fusion.
- [x] Tests can inject a custom fusion collaborator without changing core.
- [x] Request limit/budget truncation happens after fusion.
- [x] Existing scope, policy, explanation, and omitted-result behavior remains
  intact.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: `engram-retrieval` owns deterministic weighted fusion (source:
  `core/retrieval/src/lib.rs`).
- Technical: `engram-store-memory` is an adapter and may depend on retrieval
  collaborators without changing core boundaries (source: `AGENTS.md`).
- Process: concrete adapters stay outside `engram-core` (source: ADR-0003).
