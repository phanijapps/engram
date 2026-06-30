# Plan: In-Memory Knowledge Retrieval

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Extend the in-memory retrieval adapter with a focused knowledge-candidate
module. The main retrieval orchestration keeps validation, memory candidates,
knowledge candidates, fusion, and final truncation as separate steps.

Tempted to wire sqlite-vec search now; declining because query embedding and
policy rehydration are not specified. Tempted to fold knowledge retrieval into
memory candidate helpers; declining because source-grounded chunks have
different scope and filter rules.

## Constraints

- No public v1 schema changes.
- No vector, embedding, model, SQL, runtime, or TypeScript dependency.
- Keep knowledge chunks as chunks, not memory records.
- Keep adapter behavior outside `engram-core`.

## Construction tests

**Integration tests:** chunk recall, source/chunk filters, scope isolation, and
post-fusion truncation over memory plus knowledge candidates.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Use existing `KnowledgeChunk`, `KnowledgeSource`, `SourceDocument`,
`RetrievalResult`, and `RetrievalFusion` contracts. No new public contract file
is introduced.

### Component / module decomposition

- `knowledge_retrieval.rs` owns knowledge candidate filtering and result
  construction.
- `retrieval.rs` owns request validation, memory candidates, fusion, and final
  context composition.
- Tests own source/document/chunk fixtures.

### Failure, edge cases & resilience

Chunks missing their source or document are skipped because they cannot prove
scope or provenance. Denied or expired chunks are omitted as chunk results, not
silently returned.

## Tasks

### T1: Source-grounded chunk candidates

**Depends on:** PHASE20 in-memory fusion wiring.

**Tests:**
- Matching chunk returns as `RetrievalTargetType::Chunk`.
- Source kind and chunk kind filters include/exclude chunks.
- Cross-scope chunk does not leak.
- Limit truncation happens after memory and knowledge fusion.

**Approach:**
- Add focused knowledge retrieval helper module.
- Snapshot sources, documents, and chunks from in-memory state.
- Convert matching chunks into retrieval candidates before fusion.

**Done when:** retrieval tests and full repository gates pass.

## Rollout

Library code only. Vector-backed semantic recall, hierarchy expansion, and
model reranking remain future slices.

## Risks

- Knowledge policy can be weaker than memory policy if only chunk policy is
  checked; the implementation checks source, document, and chunk policies.
- Retrieval result content can lose source context; explanations carry location
  path and summary where present.

## Changelog

- 2026-06-30: initial plan for in-memory knowledge retrieval candidates.
- 2026-06-30: shipped source-grounded in-memory chunk retrieval through the
  shared fusion path.
