# Plan: indexing-embed-performance

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

## Approach

Two independent wins in `scan_repo`'s embed path: (1) **incremental** — add `VectorIndex::embedded_ids()` and embed only chunks whose id isn't already in the index; (2) **batching** — override `EmbeddingProvider::embed_batch` in the fastembed provider with a real batch call and embed in batches. Both are additive; neither changes the model/space/existing vectors. GPU (lever #3) is deferred.

Order: T1 (the `embedded_ids` query) → T2 (real `embed_batch`) → T3 (rewire the loop: filter + batch) → T4 (verify: re-scan is idempotent; a new repo embeds only its own chunks).

## Constraints

- ADR-0022: `VectorIndex` / `EmbeddingProvider` stay mechanism-agnostic; the sqlite-vec key-listing query lives in the adapter.
- Additive only; existing vectors + model untouched.

## Tasks

### T1: `VectorIndex::embedded_ids()` (additive trait method + sqlite-vec impl)

**Depends on:** none

**Tests:** returns exactly the chunk-ids that have a vector; empty for a fresh index; grows after inserts.

**Approach:** add `async fn embedded_ids(&self, scope: &Scope, space: &EmbeddingSpace) -> CoreResult<HashSet<ChunkId>>` (or similar) to `core/retrieval/src/vector_index.rs::VectorIndex`. Implement in the sqlite-vec adapter (`adapters/sqlite/src/vector/`) by querying the vec0 table's stored keys (find the right shadow/metadata table — keys are the chunk-ids passed to `insert`). Provide a default for the trait if reasonable, else implement on `SqliteVectorIndex`.

**Done when:** `cargo test` green; the method returns the embedded-id set.

### T2: real `embed_batch` in the fastembed provider

**Depends on:** none

**Tests:** `embed_batch([t1,t2,t3])` returns one vector per text, each matching `embed_passage(ti)` within tolerance; empty slice ⇒ empty.

**Approach:** `core/integration/src/sqlite/fastembed_embedding.rs` (and the inner `FastEmbedBgeSmallQueryProvider` in `adapters/sqlite/src/vector/fastembed_provider.rs`) — override `EmbeddingProvider::embed_batch` to call fastembed's native batch embed. Confirm fastembed 5.17.2's batch API in `~/.cargo/registry/src/index.crates.io-*/fastembed-5.17.2/` (`TextEmbedding::embed` over a Vec of inputs).

**Done when:** `cargo test` green; batch path verified against per-item.

### T3: incremental + batched embed loop in `scan_repo`

**Depends on:** T1, T2

**Tests:** (integration) over a seeded index, scanning a second time embeds 0 new chunks; scanning a new repo embeds only that repo's chunks.

**Approach:** `mcp/engram-mcp/src/codegraph.rs::scan_repo` embed block (~:64-90): get `embedded = vector_index.embedded_ids(...)`, filter `chunks` to those not in `embedded`, then embed the remainder in batches of N (e.g. 64) via `embedder.embed_batch(&texts)`, inserting each. Keep the skip-empty-text + per-chunk-warn behavior. Report `embedded` as the count of *newly* embedded.

**Done when:** re-scan is idempotent (0 new); new-repo scan embeds only new chunks.

### T4: end-to-end verify (real release binary)

**Depends on:** T3

**Tests:** (manual/goal-based) `cargo build --release -p engram-mcp --features fastembed`; against the existing `/tmp/engram-smoke` DB (mem-alpha + agentzero already embedded), scan `~/projects/pi` and confirm it embeds **only pi's chunks** (seconds, not minutes) and the scan summary's `embedded` count ≈ pi's chunk count (not the cumulative 25k). Optionally re-scan pi a second time → `embedded ≈ 0`.

**Done when:** pi indexes in seconds; re-scan embeds ~0.

## Rollout

Additive + behavior change (faster, idempotent). Reversible. No schema change. No GPU.

## Risks

- The sqlite-vec key-listing query must be correct (or we'd skip chunks that need embedding, or re-embed). Mitigation: T1 test asserts the set matches inserted ids.
- Batch tolerance: batched embeddings must match per-item within tolerance. Mitigation: T2 test.
- If fastembed's batch API differs from assumed, fall back to the default loop-batch (still batches the insert loop, just not one model call) — note it.

## Changelog

- 2026-08-02: initial plan — incremental (#1) + batching (#2); GPU (#3) deferred (NVIDIA CUDA + Mac CoreML/Metal).
