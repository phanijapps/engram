//! Cross-encoder reranker adapter.
//!
//! This crate is the rerank leg: it reorders fused retrieval candidates by a
//! query-aware cross-encoder relevance score, implementing the contracted
//! `RerankStrategy::CrossEncoder`. It is a focused adapter — any model
//! dependency stays behind a feature gate, never in `engram-domain` or
//! `engram-retrieval` core. Scoring is injected through [`RerankScorer`], so
//! tests use a deterministic stub and the real model is feature-gated (T2).
//!
//! Wiring into `compose_context` (a `RetrievalReranker` port + hook between
//! fusion and budget) is a follow-up spec; this crate ships the adapter unit.

mod rerank;

pub use rerank::{CrossEncoderReranker, RerankScorer};
