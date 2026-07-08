# Plan: cross-encoder-rerank

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy; may change as we learn.

**Light-mode lean fill.** Approach + short Tasks list.

## Approach

A focused adapter crate `engram-rerank-cross-encoder` that reranks fused
`RetrievalResult` candidates by a query-aware score:

1. A `RerankScorer` port scores `(query, candidate_content) -> f32`. Injected, so
   tests use a deterministic stub and the real model is feature-gated.
2. `CrossEncoderReranker::rerank(query, candidates, limit)` scores each
   candidate via the scorer, sorts best-first (stable on ties), stamps
   `FusionTrace { rerank_strategy: CrossEncoder, rerank_score }`, and truncates
   to `limit`. Provenance/policy/target are preserved.
3. A feature-gated real cross-encoder scorer (FastEmbed if the pinned crate
   exposes rerankers, else ONNX) — default build pulls no model.

Wiring into `compose_context` (a `RetrievalReranker` port + a hook between
fusion and budget) is a follow-up spec, mirroring B1's adapter/wiring split.

## Constraints

- RFC-0012 + codegraph-parity-roadmap (B2); contract-freeze policy.
- `AGENTS.md`: model + reranker stay in the adapter crate, never in core.

## Construction tests (cross-cutting)

- **Integration:** rerank a small candidate list with a stub scorer; assert order,
  `FusionTrace`, tie stability, truncation.

## Tasks

### T1: Crate skeleton + `RerankScorer` port + `CrossEncoderReranker` + stub test
**Depends on:** none
**Tests:**
- `rerank` reorders a fixture list so the stub-best candidate is first; ties keep
  input order; `limit` truncates; `FusionTrace.rerank_strategy == CrossEncoder`
  and `rerank_score` set; provenance/policy preserved.
**Approach:**
- Create `adapters/retrieval/cross-encoder-rerank/` (`engram-rerank-cross-encoder`),
  workspace member; `lib.rs` facade + `rerank.rs` (port, reranker, `rerank`); a
  `StubRerankScorer` in tests. No model dep yet.
**Done when:** `cargo test -p engram-rerank-cross-encoder` green; fmt + clippy clean.

### T2: Feature-gated real cross-encoder scorer
**Depends on:** T1
**Tests:**
- Under the feature, the real scorer compiles and scores a (query, text) pair.
**Approach:**
- Ground the pinned `fastembed` crate's reranker API (does it expose
  `TextCrossEncoding` / a reranker model?); if yes, implement
  `FastEmbedRerankScorer` behind a `cross-encoder-provider` feature; else use a
  direct ONNX reranker. Default build unchanged.
**Done when:** `cargo check -p engram-rerank-cross-encoder --features
cross-encoder-provider` green; default `cargo check` still pulls no model.

### T3 (deferred): wire into `compose_context`
**Depends on:** T1 (T2 optional)
- Follow-up spec: add a `RetrievalReranker` port to `engram-retrieval`, an
  optional reranker on `RetrievalCompositionInput`, applied between fusion and
  budget. (Mirrors the B1 → `lexical-wiring` split.)

## Risks

- The pinned `fastembed` may not expose rerankers — T2 grounds this first and
  falls back to ONNX; the adapter (T1) is useful regardless via the injected
  scorer.
- Rerank cost on large candidate sets — bounded by `limit` and by scoring only
  the pre-budget candidate set at wiring time.

## Changelog

- 2026-07-08: initial plan (light mode); adapter/wiring split mirroring B1.
