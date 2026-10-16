//! Recipe-owned local scorer construction and existing reranker dispatch.
use engram_domain::RerankStrategy;
#[cfg(any(feature = "fastembed", test))]
use engram_runtime::CoreResult;
use std::sync::Arc;

// ---- RFC-0019 T3/T4: reranker dispatch -----------------------------------
// Selects the reranker for unified recall from the operator-facing
// `recall_fusion.rerank` config. ADR-0022: this is the engine-specific wiring
// site (names adapter crates + the integration `EmbeddingProvider`); the
// reranker port + dispatch contract stay engine-neutral.

/// Selects the reranker by `rerank.strategy`. Returns `None` (no rerank) when
/// no rerank config is set, strategy is `None`, or the selected reranker's
/// prerequisites are unmet (MMR without an embedder; cross-encoder without its
/// feature/scorer). Unmet prerequisites degrade with a warning, never a panic.
pub(super) fn select_reranker(
    rerank: Option<&engram_retrieval::RerankConfig>,
    embedding_provider: Option<&Arc<dyn crate::EmbeddingProvider>>,
    scorer: Option<&Arc<dyn engram_retrieval::RerankScorer>>,
) -> Option<Arc<dyn engram_retrieval::RetrievalReranker>> {
    let cfg = rerank?;
    match cfg.strategy {
        RerankStrategy::None => None,
        RerankStrategy::Mmr => select_mmr(embedding_provider, cfg.lambda),
        RerankStrategy::CrossEncoder => select_cross_encoder(scorer),
        // `LlmJudge` / `PolicyPriority` are not dispatched by engram today.
        _ => None,
    }
}

/// MMR needs an `EmbeddingProvider` to embed candidate texts (RFC-0019 D4 — the
/// `RetrievalReranker` port exposes no embeddings). The embedder is wired only
/// under the `fastembed` feature; without it (or when construction failed), MMR
/// degrades to relevance-only ordering with a warning.
fn select_mmr(
    embedding_provider: Option<&Arc<dyn crate::EmbeddingProvider>>,
    lambda: f32,
) -> Option<Arc<dyn engram_retrieval::RetrievalReranker>> {
    #[cfg(feature = "fastembed")]
    {
        if let Some(emb) = embedding_provider {
            // Bridge the integration `EmbeddingProvider` to the MMR adapter's
            // local `MmrEmbedder` trait (the adapter cannot depend on the
            // facade without a package cycle — see `engram-rerank-mmr` docs).
            let embedder: Arc<dyn engram_rerank_mmr::MmrEmbedder> =
                Arc::new(FastEmbedMmrEmbedder { inner: emb.clone() });
            return Some(Arc::new(engram_rerank_mmr::MmrReranker::new(
                Some(embedder),
                lambda,
            )));
        }
        eprintln!(
            "engram-mcp: MMR reranker selected but no embedding provider is wired \
             (enable the `fastembed` feature); falling back to no rerank"
        );
        None
    }
    #[cfg(not(feature = "fastembed"))]
    {
        let _ = embedding_provider;
        let _ = lambda;
        eprintln!(
            "engram-mcp: MMR reranker selected but the `fastembed` feature is disabled \
             (no embedding provider); falling back to no rerank"
        );
        None
    }
}

fn select_cross_encoder(
    scorer: Option<&Arc<dyn engram_retrieval::RerankScorer>>,
) -> Option<Arc<dyn engram_retrieval::RetrievalReranker>> {
    let _ = scorer;
    eprintln!(
        "engram-mcp: generic cross-encoder has no admitted generic input contract; using existing RRF order"
    );
    None
}

pub(super) fn construct_scorer(
    config: &crate::EngramConfig,
    code: &engram_retrieval::CodeIntelligenceConfig,
    embedding_ready: bool,
) -> Option<Arc<dyn engram_retrieval::RerankScorer>> {
    let code_requested = embedding_ready
        && code
            .fusion
            .rerank
            .as_ref()
            .is_some_and(|r| r.strategy == RerankStrategy::CrossEncoder);
    // Code input limits do not constrain generic memory/document recall.
    // Generic cross-encoding retains its previous RRF degradation contract.
    let _ = config;
    if !code_requested {
        return None;
    }
    #[cfg(feature = "cross-encoder-rerank")]
    match engram_rerank_cross_encoder::LocalOnnxScorer::open_default(&code.local_scorer) {
        Ok(scorer) => return Some(Arc::new(scorer)),
        Err(_) => {
            eprintln!("engram-mcp: admitted local cross-encoder assets unavailable or incompatible")
        }
    }
    None
}

/// Bridge: adapts the integration [`crate::EmbeddingProvider`] to the MMR
/// adapter's local `MmrEmbedder` trait. Only the fastembed build wires an
/// embedding provider, so the bridge lives behind that feature.
#[cfg(feature = "fastembed")]
struct FastEmbedMmrEmbedder {
    inner: Arc<dyn crate::EmbeddingProvider>,
}

#[cfg(feature = "fastembed")]
impl engram_rerank_mmr::MmrEmbedder for FastEmbedMmrEmbedder {
    fn embed(&self, text: &str) -> CoreResult<Vec<f32>> {
        // Candidate texts are passages (not queries), so embed as a passage.
        self.inner.embed_passage(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct CodeOnly;
    impl engram_retrieval::RerankScorer for CodeOnly {
        fn score(&self, _: &str, _: &str) -> CoreResult<f32> {
            panic!("generic recall must not invoke the bounded code scorer")
        }
    }
    #[test]
    fn generic_cross_encoder_does_not_inherit_code_model_admission_limits() {
        let scorer: Arc<dyn engram_retrieval::RerankScorer> = Arc::new(CodeOnly);
        let config = engram_retrieval::RerankConfig {
            strategy: RerankStrategy::CrossEncoder,
            lambda: 0.5,
        };
        assert!(select_reranker(Some(&config), None, Some(&scorer)).is_none());
    }
}
