# Spec: In-Memory Knowledge Retrieval

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

The in-memory retrieval service returns source-grounded `KnowledgeChunk`
matches as `RetrievalResult` candidates alongside memory matches, then composes
both sources through the existing `RetrievalFusion` port before final context
truncation.

## Boundaries

### Always do

- Keep knowledge chunks distinct from memory records in retrieval results.
- Apply source scope, source kind filters, chunk kind filters, time filters, and
  policy checks before fusion.
- Preserve source provenance, source locations, summaries, and fusion traces.
- Reuse deterministic keyword scoring for the in-memory baseline.

### Ask first

- Add vector search, embedding providers, model rerankers, hierarchy expansion,
  or graph traversal.
- Change public v1 retrieval schemas.
- Persist knowledge as memory records.

### Never do

- Return chunks outside the request scope.
- Bypass source/document/chunk policy checks.
- Hide budget omissions after memory and knowledge candidates are fused.
- Add concrete retrieval behavior to `engram-core`.

## Testing Strategy

- TDD: in-memory retrieval tests cover chunk recall, source/chunk filters,
  cross-scope isolation, and post-fusion limit omission.
- Regression: existing memory retrieval tests continue to prove policy,
  explanation, no-result, and budget behavior.
- Goal-based: full repository gates prove no public contract drift.

## Acceptance Criteria

- [x] Matching knowledge chunks are returned as `RetrievalTargetType::Chunk`.
- [x] Memory and knowledge candidates are fused before request limit/budget
  truncation.
- [x] Source kind and chunk kind filters apply to knowledge candidates.
- [x] Knowledge retrieval respects source scope and retrieval policy.
- [x] Chunk explanations preserve matched terms and source location hints.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: knowledge sources, documents, and chunks are already stored in the
  in-memory adapter (source: `adapters/memory/inmem/src/knowledge.rs`).
- Technical: retrieval already composes candidates through `RetrievalFusion`
  (source: `adapters/memory/inmem/src/retrieval.rs`).
- Process: knowledge remains distinct from memory (source:
  `docs/implementation-roadmap.md`).
