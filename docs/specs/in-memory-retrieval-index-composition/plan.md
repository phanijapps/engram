# Plan: In-Memory Retrieval Index Composition

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add an injected list of `RetrievalIndex` sources to `InMemoryMemoryService`.
Retrieval collects local candidates first, asks each external index for
additional candidates, records non-fatal failures, then routes all candidates
through the existing `RetrievalFusion` and final budget logic.

Tempted to wire `engram-store-vector` directly; declining because vector
storage is one adapter behind the core port. Tempted to add an index registry;
declining because a constructor-injected list is enough for this slice.

## Constraints

- No public v1 schema changes.
- No dependency from `engram-store-memory` to vector, sqlite-vec, FastEmbed, SQL,
  or provider crates.
- Keep crate roots and package entry points as facades.
- Keep external-index error translation in a focused module, not in `lib.rs`.

## Construction tests

**Integration tests:**

- Injected external candidates appear in retrieved context and preserve source
  trace evidence.
- External candidates are considered by the existing fusion path before request
  limits and omitted-result reporting.
- A failing external index produces a degraded source failure while local
  keyword candidates still return.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

`InMemoryMemoryService` keeps the existing constructors and adds a constructor
for callers that need explicit retrieval indexes. Each index implements
`engram_core::RetrievalIndex`; the in-memory adapter neither knows nor imports
the concrete adapter type.

### Component / module decomposition

- `service.rs` owns dependency construction and stores the injected index list.
- `external_retrieval.rs` calls external indexes and translates errors into
  retrieval source failures.
- `retrieval.rs` composes local candidates, external candidates, hierarchy
  context, fusion, and final truncation.
- Tests own deterministic fake indexes.

### Failure, edge cases & resilience

External index failures are non-fatal degraded sources because local retrieval
can still satisfy the request. Indexes are expected to return policy-checked,
portable `RetrievalResult` candidates; failures are visible through
`sourceFailures` rather than hidden empty lists.

## Tasks

### T1: External index injection

**Depends on:** existing `RetrievalIndex` port and in-memory retrieval fusion.

**Tests:**
- Existing constructors still compile and produce no source failures.
- A new constructor accepts fake retrieval indexes.

**Approach:**
- Add an index list field to `InMemoryMemoryService`.
- Keep default constructors delegating to an empty list.
- Add a constructor that accepts both fusion and retrieval indexes.

**Done when:** in-memory retrieval tests compile with the new constructor.

### T2: Candidate composition and failure reporting

**Depends on:** T1.

**Tests:**
- External candidate appears in context through shared fusion.
- External candidate omitted by request limit is reported as
  `BudgetExceeded`.
- External failure becomes a degraded `RetrievalSourceFailure` while local
  candidates still return.

**Approach:**
- Add `external_retrieval.rs` for calling indexes and mapping errors.
- Append external candidates before fusion.
- Pass accumulated source failures into the final context payload.

**Done when:** focused integration tests and adjacent retrieval tests pass.

## Rollout

Library code only. Callers can compose `VectorRetrievalIndex` with this service,
including sqlite-vec and FastEmbed BGE-small test wiring, without making those
dependencies canonical.

## Risks

- A concrete external index may return unfiltered candidates. The core port
  contract already requires candidates to carry policy; this slice keeps
  policy-aware adapter responsibility explicit rather than guessing how to
  reauthorize arbitrary target types.
- Multiple indexes may duplicate targets. Existing fusion duplicate handling
  remains the single place where duplicate collapse should happen.

## Changelog

- 2026-06-30: initial plan for in-memory retrieval index composition.
- 2026-06-30: shipped injected retrieval-index composition with external
  candidate fusion, budget omissions, and degraded source-failure reporting.
