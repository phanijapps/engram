# Plan: FastEmbed Query Provider

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a feature-gated `FastEmbedBgeSmallQueryProvider` to `engram-store-vector`.
The provider initializes `TextEmbedding` with `EmbeddingModel::BGESmallENV15`,
embeds retrieval queries with a `query:` prefix, and implements
`VectorQueryProvider`. The existing ignored FastEmbed sqlite-vec smoke test uses
the provider for query vectors while deterministic fixed-vector tests remain the
default signal.

Tempted to add a generic provider registry; declining because one feature-gated
provider is enough for this slice. Tempted to enable the feature by default;
declining because model downloads must stay opt-in.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No default FastEmbed dependency.
- No dependency from core/domain/memory/SQL/ingest crates to FastEmbed.
- Keep target rehydration behind `VectorTargetResolver`.

## Construction tests

**Integration tests:**

- Ignored BGE-small smoke test initializes the provider and uses it to query
  sqlite-vec.
- Existing deterministic vector retrieval tests still use fixed vectors.

**Goal-based checks:**

- `cargo test -p engram-store-vector`
- `cargo check -p engram-store-vector --features fastembed-tests --tests`

## Design (LLD)

### Interfaces & contracts

`FastEmbedBgeSmallQueryProvider` implements the existing `VectorQueryProvider`
trait. No new core trait is introduced.

### Component / module decomposition

- `fastembed_provider.rs` owns FastEmbed initialization, mutex-protected model
  access, query prefixing, and error translation.
- `lib.rs` exports the provider only when the feature is enabled.
- `fastembed_bge_small.rs` owns the ignored provider smoke path.

### Failure, edge cases & resilience

Initialization, model-lock poisoning, empty embedding results, and embedding
errors become `CoreError::Adapter` values. The provider does not resolve vector
targets or authorize retrieval results.

## Tasks

### T1: Feature-gated provider module

**Depends on:** existing vector query provider port.

**Tests:**
- `cargo check -p engram-store-vector --features fastembed-tests --tests`
  compiles the provider.

**Approach:**
- Add `fastembed-provider` feature and keep `fastembed-tests` depending on it.
- Add the provider module and conditional export.

**Done when:** feature-gated compile check passes.

### T2: Smoke test wiring and docs

**Depends on:** T1.

**Tests:**
- Ignored FastEmbed BGE-small test uses the provider query path.
- Full repository gates pass.

**Approach:**
- Replace direct query embedding in the ignored smoke test with the provider.
- Update roadmap and changelog status.

**Done when:** deterministic tests, feature compile check, and docs gates pass.

## Rollout

Library code only. Consumers must opt into the feature and accept model asset
downloads when running the ignored smoke test or constructing the provider.

## Risks

- FastEmbed model initialization can download assets; the test remains ignored
  and default builds do not enable the feature.
- The provider emits query embeddings only. Passage/document embedding and
  indexing policy remain future work.

## Changelog

- 2026-06-30: initial plan for opt-in FastEmbed BGE-small query provider.
- 2026-06-30: shipped feature-gated `FastEmbedBgeSmallQueryProvider` and wired
  the ignored sqlite-vec smoke test through it.
