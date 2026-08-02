# Spec: indexing-embed-performance

- **Status:** Implementing <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine neutrality — VectorIndex/EmbeddingProvider stay mechanism-agnostic; sqlite-vec impl in the adapter)
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

## Objective

`scan_repo` embeds **only chunks not already in the vector index**, in **batches** — so adding a repo to a populated multi-repo DB (or re-scanning an unchanged repo) embeds only the new chunks, in seconds, instead of re-embedding the entire scope every scan (the current O(total) per scan / O(N²) cumulative behavior that made adding a small repo take ~18 minutes).

## Boundaries

### Always do
- Additive only: new `VectorIndex` method + `embed_batch` override; do not change the embedding model/space or existing vectors.
- Keep `VectorIndex` / `EmbeddingProvider` mechanism-agnostic (the sqlite-vec query lives in the adapter).
- Batch size sane (e.g. 32–64); degrade gracefully if a batch partially fails.

### Ask first
- Changing the `EmbeddingProvider` or `VectorIndex` trait shape beyond an additive method.
- GPU execution providers (lever #3 — deferred; NVIDIA→CUDA, Mac→CoreML/Metal).

### Never do
- Re-embed chunks that already have a vector.
- Change the embedding model, dimensions, or the on-disk vector schema.
- Block the scan on a single failed chunk (skip + warn, as today).

## Testing Strategy

- **TDD** — `embedded_ids()` returns the set of already-embedded ids; the incremental loop skips them; `embed_batch` produces the same vectors as `embed_passage` per-item (within tolerance) and is called in batches.
- **Integration** — re-scanning an unchanged repo embeds ~0 new chunks (idempotent); scanning a second repo into a populated DB embeds only the new repo's chunks.

## Acceptance Criteria

- [ ] `VectorIndex` exposes the set of already-embedded chunk-ids (additive method, e.g. `embedded_ids`), with a sqlite-vec impl.
- [ ] `scan_repo` embeds only chunks whose id is **not** already embedded (incremental).
- [ ] `scan_repo` embeds in **batches** (real `embed_batch`, not the one-at-a-time loop).
- [ ] Re-scanning an unchanged repo embeds ~0 new chunks (idempotent — fast).
- [ ] Adding a repo to a populated multi-repo DB embeds **only the new repo's** chunks (verified: a small repo indexes in seconds, not minutes).

## Assumptions

- Technical: the embed loop is `mcp/engram-mcp/src/codegraph.rs` `scan_repo` (~:64-90) — `query.list_chunks(&scope)` then `embed_passage` per chunk.
- Technical: `EmbeddingProvider` (`core/integration/src/embedding.rs:37`) already has `embed_batch` with a default that loops `embed_passage` — override it in the fastembed provider with a real batch call.
- Technical: `VectorIndex` (`core/retrieval/src/vector_index.rs:24`) has `insert` + `search`, no list/contains — add an additive `embedded_ids`.
- Technical: sqlite-vec stores vectors in a vec0 virtual table; the chunk-id keys are queryable via its shadow/metadata tables (the impl finds the right query).
- Product: GPU (CUDA/CoreML) is **deferred** (lever #3) — this spec is CPU-only incremental + batching. User hardware noted for #3: NVIDIA GPU (Linux) + Mac.
