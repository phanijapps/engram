# ADR-0024: Batch embeddings — deferred reindex over inline VectorIndex composition

- **Status:** Proposed
- **Date:** 2026-07-10
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** ADR-0009 (retrieval composition seam), [`atomic-batch-ingest`](../specs/atomic-batch-ingest/spec.md) (S3 — embeddings step deferred), [`engram-host-sdk`](../product/briefs/engram-host-sdk.md) brief

## Decision summary

- **Decision:** The `BatchIngest` records `EmbeddingRef` metadata on records (already happens via `Provenance`) but does **not** inline-generate vectors; a separate reindex job generates actual vectors via `VectorIndex`.
- **Because:** embedding generation needs a model (slow, feature-gated); the batch's job is semantic ingest, not vector generation. Keeping the batch decoupled from `VectorIndex` is cleaner.
- **Applies to:** the `atomic-batch-ingest` embeddings step (S3's deferred `atomic-batch-evidence-embeddings`).
- **Tradeoff accepted:** vectors are not immediately available after a batch ingest — a separate reindex step must run before vector-based recall works on the new records.
- **Revisit if:** a use case requires vectors to be available immediately after ingest (inline composition becomes worth the coupling).

## Context

S3's `BatchIngest` ships the embeddings step as `Skipped` — `EmbeddingRef` is metadata (provider/model/dimension + reference), not the vector itself. Actual vectors live behind `VectorIndex` (`SqliteVectorIndex`), which `SqlBatchIngest { memory, knowledge }` does not compose. The deferred item (`atomic-batch-evidence-embeddings`) was blocked on "deciding whether the batch should own a `VectorIndex` handle or delegate to a follow-up indexing job."

Two composition models were considered:
1. **Deferred reindex** — the batch records `EmbeddingRef` metadata on records (already happens via `Provenance`); a separate reindex job reads records, generates embeddings via the configured `EmbeddingProvider`, and writes vectors to `VectorIndex`.
2. **Inline composition** — the batch composes a `VectorIndex` + `EmbeddingProvider` handle and writes vectors as part of the batch step (adding a third store handle to `SqlBatchIngest`).

## Decision

The batch uses **deferred reindex** (option 1). The batch records metadata; vector generation is a separate concern handled by a reindex job. The `SqlBatchIngest` does NOT compose `VectorIndex`.

## Consequences

**Positive:**
- The batch stays decoupled from the embedding model + vector store — no feature-gating, no model dependency in the batch path.
- Batch ingest is fast (no model inference); vectors are generated asynchronously.
- The reindex job can be scheduled, batched, or deferred without coupling to the ingest lifecycle.

**Negative:**
- Vectors are not immediately available after batch ingest — a reindex must run before vector-based recall (`UnifiedRecall` vector lane) works on the new records.
- Two-step model (ingest → reindex) adds operational complexity for hosts that want immediate vectors.

**Revisit if:** a use case requires vectors available immediately after ingest (inline composition becomes worth the coupling), or if the two-step model causes confusion.

## Alternatives considered

- **Inline VectorIndex composition** (option 2). Rejected: couples the batch to an `EmbeddingProvider` (feature-gated, model-bearing) + `VectorIndex` handle, adding a third store to `SqlBatchIngest`. The batch's job is semantic ingest (episode + facts + entities + relationships); vector generation is a different concern (needs a model, may be slow). The coupling isn't justified at demo scale.

## References

- `docs/backlog.md` → `atomic-batch-evidence-embeddings` (the deferred item this ADR resolves).
- ADR-0009 (retrieval composition seam — the VectorIndex port the reindex job writes to).
- `docs/specs/atomic-batch-ingest/spec.md` (S3 — the embeddings step).
