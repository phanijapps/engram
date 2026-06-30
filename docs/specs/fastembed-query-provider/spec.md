# Spec: FastEmbed Query Provider

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

`engram-store-vector` exposes an opt-in FastEmbed BGE-small query provider that
implements the existing `VectorQueryProvider` trait, so local semantic retrieval
can generate query vectors for sqlite-vec without making FastEmbed, model
downloads, or embeddings part of default Engram behavior.

## Boundaries

### Always do

- Keep FastEmbed behind an explicit Cargo feature.
- Use BGE-small as the first local model-backed provider.
- Prefix retrieval queries using the BGE query convention before embedding.
- Translate provider failures into `CoreError::Adapter`.
- Keep vector bytes and provider lifecycle outside portable domain contracts.

### Ask first

- Enable FastEmbed by default.
- Add hosted embedding providers or network services.
- Change public v1 JSON schemas or generated TypeScript.
- Make `engram-store-memory`, `engram-core`, or `engram-domain` depend on
  FastEmbed.

### Never do

- Download model assets during default tests or default builds.
- Store embedding vectors as canonical memory or knowledge records.
- Couple target rehydration or policy checks to the query embedding provider.
- Replace deterministic fixed-vector tests with provider-backed tests.

## Testing Strategy

- TDD: feature-gated vector tests compile and use the provider in the ignored
  BGE-small sqlite-vec smoke path.
- Regression: deterministic sqlite-vec and vector retrieval tests continue to
  pass without the provider feature.
- Goal-based: `cargo check -p engram-store-vector --features fastembed-tests --tests`
  proves the opt-in provider path compiles without running model downloads.

## Acceptance Criteria

- [x] A feature-gated FastEmbed BGE-small query provider implements
  `VectorQueryProvider`.
- [x] FastEmbed is not enabled by default and is not pulled into core/domain,
  memory, SQL, or ingest crates.
- [x] Provider initialization and embedding errors are translated into
  `CoreError::Adapter`.
- [x] The existing ignored BGE-small sqlite-vec smoke test exercises the
  provider for query vectors.
- [x] Deterministic non-FastEmbed vector tests remain unchanged and pass.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: `VectorQueryProvider` is the existing query-vector seam (source:
  `adapters/retrieval/sqlite-vec/src/retrieval.rs`).
- Technical: FastEmbed BGE-small smoke coverage already exists as an ignored
  feature-gated test (source:
  `adapters/retrieval/sqlite-vec/tests/fastembed_bge_small.rs`).
- Technical: FastEmbed is currently optional in the vector crate only (source:
  `adapters/retrieval/sqlite-vec/Cargo.toml`).
- Process: vector stores and embedding providers stay outside core/domain
  contracts (source: `AGENTS.md`).
