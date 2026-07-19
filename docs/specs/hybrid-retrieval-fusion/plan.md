# Plan: Hybrid Retrieval Fusion

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add an `engram-retrieval` crate with a deterministic weighted-sum fusion
implementation over existing `RetrievalResult` values. The crate consumes
policy-checked candidates and returns ranked results with updated
`FusionTrace`.

Tempted to add learned reranking now; declining because model/provider
contracts are not specified. Tempted to wire it into memory retrieval services
immediately; declining because the first slice should prove fusion behavior as a
portable collaborator.

## Constraints

- No public v1 schema changes.
- No store, vector, embedding, model, async runtime, or TypeScript dependency.
- `lib.rs` remains a facade over focused modules.

## Construction tests

**Unit tests:** deterministic ranking, duplicate collapse, source weights, and
limit behavior.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Implement `RetrievalFusion` for `WeightedRetrievalFusion`. Inputs are candidate
`RetrievalResult` values from any index. Output is ranked fused results.

### Component / module decomposition

- `config.rs` owns source weighting configuration.
- `weighted.rs` owns fusion algorithm and duplicate handling.
- `lib.rs` re-exports public types.
- Tests own retrieval result fixtures.

### Failure, edge cases & resilience

Missing source traces use a default source name. Negative or non-finite weights
are rejected. Empty inputs return empty results.

## Tasks

### T1: Weighted retrieval fusion crate

**Depends on:** core retrieval contracts.

**Tests:**
- Higher weighted score sorts first.
- Duplicate target records collapse into one result with dedup trace.
- Source weights affect final score.
- Request limit is applied after fusion.

**Approach:**
- Add `engram-retrieval` workspace crate.
- Implement `WeightedRetrievalFusion`.
- Add focused tests.

**Done when:** fusion tests and full repository gates pass.

## Rollout

Library code only. Wiring into concrete services and advanced rerankers remain
future slices.

## Risks

- Fusion can be mistaken for policy enforcement; docs and code keep policy
  decisions outside this crate.

## Changelog

- 2026-06-30: initial plan for deterministic hybrid retrieval fusion.
- 2026-06-30: shipped `engram-retrieval` weighted fusion slice with duplicate
  trace tests and invalid-weight validation.
