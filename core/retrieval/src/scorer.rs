//! Model-neutral pair-scoring and exact input-admission port.
use crate::TextTokenization;
use engram_domain::CodeTokenCoverage;
use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

/// Actual loaded scorer/inputs, independent of an operator's configured alias.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerankScorerIdentity {
    pub model_id: String,
    pub manifest_id: String,
    pub tokenizer_id: String,
    pub input_contract_id: String,
    pub score_contract_id: String,
    pub window: u32,
}

/// Generic reranking remains compatible with existing per-pair implementations.
/// Code processing additionally requires explicit locality and exact admission.
pub trait RerankScorer: Send + Sync {
    fn score(&self, query: &str, candidate_text: &str) -> CoreResult<f32>;

    fn score_batch(
        &self,
        query: &str,
        documents: &[String],
        batch_size: usize,
    ) -> CoreResult<Vec<f32>> {
        if batch_size == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "rerank batch size must be positive".into(),
            });
        }
        let mut scores = Vec::with_capacity(documents.len());
        for batch in documents.chunks(batch_size) {
            for document in batch {
                let score = self.score(query, document)?;
                if !score.is_finite() {
                    return Err(CoreError::ValidationFailed {
                        reason: "rerank scorer returned a nonfinite score".into(),
                    });
                }
                scores.push(score);
            }
        }
        Ok(scores)
    }

    /// Unknown/external implementations receive no code input by default.
    fn is_local(&self) -> bool {
        false
    }
    /// True only after construction has admitted the complete code-input
    /// contract, including untruncated query/document/pair inspectors. Generic
    /// scorers retain a false default even when they expose an identity.
    fn supports_code_input(&self) -> bool {
        false
    }
    fn identity(&self) -> Option<RerankScorerIdentity> {
        None
    }

    /// Query-only, unpadded/untruncated tokens before any model inference.
    fn inspect_query(&self, _query: &str) -> CoreResult<TextTokenization> {
        unavailable()
    }

    /// Unpadded/untruncated document offsets for deterministic continuations.
    fn inspect_document(&self, _document: &str) -> CoreResult<TextTokenization> {
        unavailable()
    }

    /// Exact complete pair. `context_prefix_bytes` identifies a UTF-8 boundary
    /// in `document`; preceding tokens count as context, subsequent ones as code.
    /// Includes the pair processor's actual special tokens, without truncation.
    fn inspect_pair(
        &self,
        _query: &str,
        _document: &str,
        _context_prefix_bytes: usize,
    ) -> CoreResult<CodeTokenCoverage> {
        unavailable()
    }
}

fn unavailable<T>() -> CoreResult<T> {
    Err(CoreError::CapabilityUnsupported {
        capability: "local_code_rerank_input".into(),
        reason: "scorer has no verified local input contract".into(),
    })
}
