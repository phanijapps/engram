# Plan: In-Memory Contradiction-Aware Belief Ranking

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Pass scoped contradiction snapshots into belief retrieval. When a matching
belief is targeted by one or more open contradictions in scope, apply a
deterministic score penalty and include contradiction evidence in the retrieval
explanation. Resolved and ignored contradictions do not affect ranking.

Tempted to make contradictions omit beliefs outright; declining because review
records are advisory until a retraction policy exists. Tempted to use severity
as a learned ranker; declining because the first slice needs deterministic,
simple behavior with obvious tests.

## Constraints

- No public v1 schema changes.
- No retrieval-time mutation.
- No semantic/model-assisted contradiction inference.
- Keep ranking behavior inside the focused belief retrieval module.

## Construction tests

**Integration tests:**

- Two matching beliefs rank with the non-contradicted belief first when the
  other has an open scoped contradiction.
- Resolved contradiction does not penalize the target belief.
- Out-of-scope contradiction does not penalize the target belief.
- Explanation mentions open contradiction evidence.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

No new public Rust trait or JSON contract is required. Retrieval still returns
portable `RetrievalResult` values; score and explanation fields carry the
contradiction-aware ranking evidence.

### Component / module decomposition

- `retrieval.rs` snapshots contradictions from in-memory state.
- `belief_retrieval.rs` owns contradiction filtering and score adjustment.
- `belief_retrieval.rs` tests own deterministic belief and contradiction
  fixtures.

### Failure, edge cases & resilience

Only open contradictions with a belief target matching the retrieved belief ID
and visible to the request scope apply. The ranking penalty is deterministic and
does not remove candidates from retrieval.

## Tasks

### T1: Contradiction snapshots for belief retrieval

**Depends on:** in-memory contradiction resolution.

**Tests:**
- Matching belief with an open contradiction receives a lower score.
- Out-of-scope contradiction is ignored.

**Approach:**
- Add contradiction snapshots to the retrieval state read.
- Pass them to `belief_candidates`.
- Filter contradiction targets by belief ID and request scope.

**Done when:** belief retrieval focused tests pass.

### T2: Explanation and resolved-record behavior

**Depends on:** T1.

**Tests:**
- Resolved contradiction does not reduce score.
- Explanation mentions open contradiction review evidence.

**Approach:**
- Adjust belief score and explanation only when active contradiction count is
  non-zero.
- Keep resolved and ignored statuses out of the penalty path.

**Done when:** full repository gates pass.

## Rollout

Library code only. Automatic belief retraction, contradiction-aware fusion
weights, and semantic contradiction detection remain future slices.

## Risks

- A fixed penalty may be too blunt. That is acceptable for the deterministic
  baseline; severity-weighted ranking can follow once evaluation fixtures exist.
- Users may expect contradicted beliefs to disappear. This slice keeps them
  visible and explained because contradiction records are advisory.

## Changelog

- 2026-06-30: initial plan for contradiction-aware belief ranking.
- 2026-06-30: shipped explicit open-contradiction score penalty and retrieval
  explanation support for belief candidates.
