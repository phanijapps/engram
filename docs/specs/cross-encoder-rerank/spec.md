# Spec: cross-encoder-rerank

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, `docs/codegraph-parity-roadmap.md` (item B2), `docs/domain-data-model.md` (contract-freeze policy)
- **Brief:** none
- **Contract:** none — implements the already-contracted `RerankStrategy::cross_encoder`; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

**Scope.** A self-contained cross-encoder **reranker adapter** crate that reranks
fused retrieval candidates by query-vs-content relevance. Wiring it into
`compose_context` (a `RetrievalReranker` port + hook) is a follow-up spec — this
ships the adapter unit, mirroring how B1 shipped the lexical adapter before its
wiring.

## Objective

A cross-encoder reranker reorders retrieval candidates by a query-aware
relevance score (the model scores the query and each candidate together), so the
final top-K reflects semantic relevance rather than only the fusion rank. It
implements `RerankStrategy::CrossEncoder`, is general-purpose (any
`RetrievalResult`), and populates `FusionTrace.rerank_strategy` /
`rerank_score` so the rerank is explainable.

## Boundaries

### Always do
- Score through an injected `RerankScorer` trait, so tests use a deterministic
  stub and the real model is feature-gated (mirrors the fastembed precedent).
- Populate `FusionTrace { rerank_strategy: CrossEncoder, rerank_score: <score> }`
  on reranked results.
- Preserve each candidate's policy, provenance, and target identity — rerank
  only reorders and re-scores.

### Ask first
- The real scorer backend: FastEmbed cross-encoder (if the pinned `fastembed`
  crate exposes rerankers) vs. a direct ONNX reranker. Resolved at T2.

### Never do
- Change the accepted v1 retrieval contract or the `RetrievalMode` /
  `RerankStrategy` enums.
- Put a model dependency or the reranker in `engram-domain` or `engram-retrieval`
  core — keep it in a focused adapter crate.
- Drop candidates below a score threshold silently — report via the existing
  budget/omission path at composition time, not inside the reranker.

## Testing Strategy

- **TDD** — rerank ordering (stub scorer ranks a known-best candidate first),
  `FusionTrace` population, stable order on ties, and limit truncation.
- **Goal-based check** — feature-gated real-scorer compile path; per-crate gates
  (fmt/clippy `-D warnings`/test).

## Acceptance Criteria

- [ ] `rerank(query, candidates, limit)` reorders candidates by the injected
  scorer's query-vs-content score, best-first, verified by a deterministic unit
  test with a stub scorer.
- [ ] Reranked results carry `FusionTrace { rerank_strategy: CrossEncoder,
  rerank_score: <score> }`; original provenance/policy/target are preserved.
- [ ] Ties break stably (input order preserved); `limit` truncates after rerank.
- [ ] The real cross-encoder scorer is feature-gated and compiles under that
  feature; default build pulls no model dependency.
- [ ] No public v1 contract or enum changes.
- [ ] Per-crate gates green: `cargo fmt --check`, `cargo clippy --all-targets
  -- -D warnings`, `cargo test` on the new crate.
- [ ] (deferred: follow-up wiring spec) Reranker applied inside `compose_context`
  between fusion and budget.

## Assumptions

- Technical: `RerankStrategy::CrossEncoder` and `FusionTrace.rerank_strategy` /
  `rerank_score` are contracted but no rerank port or step exists;
  `compose_context` runs fusion → budget with no rerank. (source: A1 audit +
  grep of `core/retrieval/src/{composer,reciprocal,weighted}.rs`)
- Technical: the fastembed feature-gating in `engram-store-vector` is the
  precedent for an optional model-backed scorer. (source:
  `adapters/retrieval/sqlite-vec/Cargo.toml`)
- Process: light mode, single adversarial pass. (source: user confirmation 2026-07-08)
