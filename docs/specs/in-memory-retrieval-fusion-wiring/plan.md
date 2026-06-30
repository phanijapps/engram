# Plan: In-Memory Retrieval Fusion Wiring

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a `RetrievalFusion` dependency to `InMemoryMemoryService`. Existing
constructors use `WeightedRetrievalFusion::default()`, while a new constructor
accepts an injected fusion collaborator for tests and future composition.
Retrieval builds policy-checked keyword candidates, fuses them, then applies
limit/budget truncation.

Tempted to add vector or hierarchy candidates in this slice; declining because
this slice is wiring only. Tempted to add a context composer abstraction now;
declining until another adapter needs the same composition behavior.

## Constraints

- No public v1 schema changes.
- No vector, embedding, model, SQL, runtime, or TypeScript dependency.
- Keep `lib.rs` and `service.rs` as facades/construction surfaces, not scoring
  modules.

## Construction tests

**Integration tests:** custom fusion controls in-memory retrieval order before
limit truncation; existing retrieval tests cover policy and omissions.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Use the existing `RetrievalFusion` core trait. Add
`InMemoryMemoryService::with_retrieval_fusion` for explicit injection.

### Component / module decomposition

- `service.rs` owns dependency construction.
- `retrieval.rs` owns candidate production, fusion call, and final context
  truncation.
- Tests own fake fusion implementations.

### Failure, edge cases & resilience

Fusion errors bubble through `CoreResult`. Budget-exceeded omissions are created
from fused results so post-fusion ranking remains explainable.

## Tasks

### T1: Wire in-memory retrieval through fusion

**Depends on:** PHASE18 weighted fusion.

**Tests:**
- Injected fusion collaborator controls result ordering.
- Limit truncation happens after injected fusion.
- Existing retrieval tests still pass.

**Approach:**
- Add `engram-retrieval` dependency to `engram-store-memory`.
- Add fusion field and constructor.
- Split candidate creation from final truncation in retrieval.

**Done when:** retrieval tests and full repository gates pass.

## Rollout

Library code only. SQL/vector/hierarchy retrieval composition remains future
slices.

## Risks

- Fusion could obscure budget omissions; tests cover post-fusion truncation.
- Adapter dependency direction must remain one-way: store-memory may depend on
  retrieval, core must not.

## Changelog

- 2026-06-30: initial plan for in-memory retrieval fusion wiring.
- 2026-06-30: shipped in-memory retrieval fusion injection and post-fusion
  truncation behavior.
