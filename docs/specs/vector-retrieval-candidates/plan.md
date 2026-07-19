# Plan: Vector Retrieval Candidates

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a focused retrieval module to `engram-store-vector` that implements
`RetrievalIndex` by composing `SqliteVectorIndex`, a query-vector provider, and a
target resolver. The module returns vector candidates only after resolver
rehydration supplies content, policy, provenance, and portable target type.

Tempted to wire FastEmbed directly; declining because model/provider lifecycle
is not yet a production contract. Tempted to make the vector index own target
lookup; declining because canonical records live in memory/knowledge stores.

## Constraints

- No public v1 schema changes.
- No concrete embedding provider in default code paths.
- No dependency from `engram-core` to sqlite-vec.
- Keep crate root as a facade and retrieval behavior in a focused module.

## Construction tests

**Integration tests:** nearest-neighbor candidate order, missing target skip,
trace/score evidence, and query vector dimension mismatch.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Add `VectorQueryProvider`, `VectorTargetResolver`, `VectorResolvedTarget`, and
`VectorRetrievalIndex` in `engram-store-vector`. `VectorRetrievalIndex`
implements the core `RetrievalIndex` trait.

### Component / module decomposition

- `retrieval.rs` owns query-vector lookup, sqlite-vec search, target
  rehydration, and candidate construction.
- `index.rs` remains raw sqlite-vec storage/search only.
- `entry.rs` remains adapter-local row/result types.
- Tests own deterministic query providers and target resolvers.

### Failure, edge cases & resilience

Query-vector failures and sqlite dimension mismatches return `CoreError`.
Missing targets resolve to `None` and are skipped so stale vector rows do not
break retrieval. Resolver errors propagate because they may indicate storage or
policy failure.

## Tasks

### T1: Vector retrieval candidate adapter

**Depends on:** sqlite-vec vector index baseline and `RetrievalIndex` core port.

**Tests:**
- Nearest vector hit returns first with vector trace evidence.
- Missing target resolver result is skipped.
- Query vector dimension mismatch returns an error.

**Approach:**
- Add retrieval module and public port structs/traits.
- Add `async-trait` dependency for implementing `RetrievalIndex`.
- Keep FastEmbed only in existing opt-in feature tests.

**Done when:** vector retrieval tests and full repository gates pass.

## Rollout

Library code only. Production embedding providers and full memory/knowledge
service wiring remain future slices.

## Risks

- A resolver can accidentally bypass policy; the target type requires policy and
  provenance fields, and the spec keeps resolver ownership explicit.
- Stale vector rows are expected; skipping unresolved hits prevents false
  positives without hiding resolver errors.

## Changelog

- 2026-06-30: initial plan for vector retrieval candidates.
- 2026-06-30: shipped vector `RetrievalIndex` adapter with injected query-vector
  provider and target resolver.
