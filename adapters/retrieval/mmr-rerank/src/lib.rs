//! MMR (Maximal Marginal Relevance) diversity reranker adapter.
//!
//! This crate is the MMR leg of reranking: given fused retrieval candidates
//! with relevance scores, it reorders them to balance relevance against
//! diversity, implementing the contracted `RerankStrategy::Mmr` (RFC-0019 D4).
//! Diversity is computed from candidate-content embeddings produced by an
//! injected [`MmrEmbedder`]. The `RetrievalReranker` port exposes no
//! embeddings, so MMR injects its own embedder rather than extending the port
//! or `RetrievalResult`.
//!
//! `MmrEmbedder` is intentionally a local trait (not the integration facade's
//! `EmbeddingProvider`): the bootstrap wiring site (`engram-integration`) both
//! constructs this adapter and re-exports `EmbeddingProvider`, so a hard
//! dependency on the facade would form a package cycle. The bootstrap provides
//! a one-line bridge impl (behind the `fastembed` feature) — the same injection
//! pattern the sibling `engram-rerank-cross-encoder` crate uses with its
//! `RerankScorer`.
//!
//! ADR-0022: the MMR algorithm lives in this adapter crate; the reranker trait
//! + dispatch stay engine-neutral. No `Sql*`, no engine type is named here.

mod reranker;

pub use reranker::{MmrEmbedder, MmrReranker};
