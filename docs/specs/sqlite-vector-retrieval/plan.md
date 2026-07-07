# Plan: SQLite Vector Retrieval

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add `engram-store-vector` as a focused SQLite vector adapter. The first slice
uses sqlite-vec directly with fixed vectors in normal tests and an ignored
FastEmbed BGE-small smoke test for local model-backed verification. Do not wire
this into `MemoryService::retrieve` until fusion policy and source-failure
composition are specified.

Tempted to implement full hybrid retrieval now; declining because the fusion
contract is not finalized. Tempted to make FastEmbed part of production adapter
construction; declining because PHASE09 only requires a test embedding provider
path. Tempted to store vectors in SQL memory tables; declining because vector
indexes are secondary adapter state.

## Constraints

- ADR-0005 says vector indexes are secondary indexes, not canonical storage.
- ADR-0006 keeps SQLite as the local durable test target.
- `sqlite-vec` is an alpha crate, so unsafe extension registration must stay
  isolated.
- FastEmbed BGE-small downloads model assets, so its test is opt-in.

## Construction tests

**Integration tests:** fixed-vector sqlite-vec insert/query test and ignored
FastEmbed BGE-small smoke test.

**Manual verification:** developers can run
`cargo test -p engram-store-vector --features fastembed-tests --test fastembed_bge_small -- --ignored`
when they want to exercise FastEmbed downloads locally.

## Design (LLD)

### Data & schema

The adapter stores vector rows in a sqlite-vec `vec0` table with metadata columns
for target type, target ID, model, dimensions, and content hash. The vector
payload is serialized as little-endian `f32` bytes for sqlite-vec.

### Interfaces & contracts

`SqliteVectorIndex` exposes insert and search methods over `VectorEntry` and
`VectorSearchResult`. It does not implement core retrieval fusion yet.

### Component / module decomposition

- `extension.rs` owns sqlite-vec registration.
- `vector.rs` owns vector byte serialization.
- `entry.rs` owns adapter entry/result structs.
- `index.rs` owns schema creation, insert, and query behavior.

### Failure, edge cases & resilience

Dimension mismatches and empty vectors fail before SQL writes. sqlite-vec or
SQLite failures surface as adapter errors. FastEmbed failures remain isolated to
the ignored smoke test.

## Tasks

### T1: SQLite vector index stores and queries fixed vectors

**Depends on:** none

**Tests:**
- Insert two fixed vectors and query nearest neighbors.
- Empty vectors and dimension mismatches fail.

**Approach:**
- Add `engram-store-vector`.
- Register sqlite-vec behind an adapter function.
- Store vectors in `vec0` and query by nearest neighbor.

**Done when:** normal vector tests pass without model downloads.

### T2: FastEmbed BGE-small smoke path exists

**Depends on:** T1

**Tests:**
- Ignored test initializes `EmbeddingModel::BGESmallENV15`, embeds passages and a
  query, inserts passage vectors, and retrieves the expected target.

**Approach:**
- Add `fastembed` as a dev-dependency only.
- Keep the smoke test ignored by default.

**Done when:** the ignored test compiles with `cargo test --workspace` and can
be run explicitly by developers.

## Rollout

This ships as adapter library code and tests only. It does not alter default
memory retrieval.

## Risks

- `sqlite-vec` is alpha, so API changes are likely.
- FastEmbed model downloads can be slow or unavailable in offline CI.
- Fusion policy remains incomplete until a later retrieval slice.

## Changelog

- 2026-06-29: initial plan for sqlite-vec plus FastEmbed BGE-small vector tests.
- 2026-06-29: implemented sqlite-vec fixed-vector tests and opt-in FastEmbed
  BGE-small smoke test.
