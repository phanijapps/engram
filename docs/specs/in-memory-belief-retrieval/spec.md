# Spec: In-Memory Belief Retrieval

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

Engram can retrieve active, scoped belief records through the in-memory
`MemoryService::retrieve` path. Belief results remain distinct from memories and
knowledge chunks, participate in the same deterministic fusion step, and are
returned only when their scope, lifecycle, policy, and filters allow retrieval.

## Boundaries

### Always do

- Return beliefs as `RetrievalTargetType::Belief`, not memory records.
- Restrict belief candidates by request scope before scoring.
- Return only active beliefs that are not stale or expired.
- Apply belief policy `allowedUses`, authorizer checks, time filters, and
  `minConfidence` filters before candidate creation.
- Feed belief candidates into the existing retrieval fusion path before final
  limit or budget truncation.
- Preserve explanations and fusion traces when explanations are requested.
- Reduce ranking for matching beliefs that have scoped open contradiction
  review records.

### Ask first

- Add new retrieval request fields, public schemas, or generated TypeScript.
- Retrieve stale, retracted, superseded, or archived beliefs by default.
- Add belief graph expansion, automatic contradiction inference, or model
  reranking.
- Expose a dedicated belief query API.

### Never do

- Treat beliefs as source truth or mutate them during retrieval.
- Leak out-of-scope or policy-denied belief content through returned items.
- Put belief retrieval behavior into `engram-core`.

## Testing Strategy

- TDD: adapter tests seed active, stale, expired, low-confidence, policy-denied,
  and out-of-scope beliefs, then assert retrieval results and omissions.
- Regression: existing memory and knowledge retrieval tests continue to pass,
  proving shared fusion and truncation behavior still composes.
- Goal-based: repository gates prove no public contract drift.

## Acceptance Criteria

- [x] Matching active in-scope beliefs are returned as belief retrieval results.
- [x] Belief retrieval preserves explanations and `belief.keyword` fusion trace
  evidence.
- [x] Out-of-scope beliefs, inactive beliefs, stale beliefs, and expired beliefs
  do not leak as returned items.
- [x] Policy-denied beliefs become policy-denied omissions without leaking
  content.
- [x] `minConfidence`, `since`, and `until` filters apply to beliefs.
- [x] Belief candidates participate in shared fusion and final budget omission.
- [x] Open contradiction review records can reduce belief ranking without
  hiding the belief result.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: `RetrievalTargetType::Belief` already exists in the accepted
  retrieval contract (source: `core/domain/src/retrieval.rs`).
- Technical: belief records already carry scope, lifecycle status, policy,
  provenance, confidence, and timestamps (source:
  `core/domain/src/belief.rs`).
- Technical: the in-memory retrieval path already fuses memory and knowledge
  candidates through `RetrievalFusion` before final truncation (source:
  the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`)).
- Process: belief results must remain distinct from source truth (source:
  `docs/domain-data-model.md`).
