//! Vector candidate retrieval over sqlite-vec rows.
//!
//! This module adapts raw nearest-neighbor rows into portable retrieval
//! candidates. Query embedding and target rehydration are injected so vector
//! storage remains secondary adapter state rather than domain truth.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engram_core::{CoreError, CoreResult, RetrievalIndex};
use engram_domain::{
    FusionStrategy, FusionTrace, Metadata, Policy, Provenance, RerankStrategy,
    RetrievalExplanation, RetrievalRequest, RetrievalResult, RetrievalScore, RetrievalTargetType,
};

use crate::{SqliteVectorIndex, VectorSearchResult};

/// Provides a query vector for a retrieval request.
///
/// Implementations may use deterministic fixtures, cached embeddings, or
/// provider-backed embeddings. Provider lifecycle and model downloads stay out
/// of the vector index adapter.
pub trait VectorQueryProvider: Send + Sync {
    /// Returns the vector used for sqlite-vec search.
    fn query_vector(&self, request: &RetrievalRequest) -> CoreResult<Vec<f32>>;
}

/// Rehydrates a vector hit into a portable retrieval target.
///
/// Resolvers own canonical record lookup and policy-aware target visibility.
/// Returning `Ok(None)` means the vector row is stale or not visible for this
/// request and should be skipped.
pub trait VectorTargetResolver: Send + Sync {
    /// Resolves one sqlite-vec hit into a retrieval target.
    fn resolve(
        &self,
        hit: &VectorSearchResult,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<VectorResolvedTarget>>;
}

/// Canonical target data required before a vector hit can become a result.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorResolvedTarget {
    pub target_type: RetrievalTargetType,
    pub target_id: String,
    pub content: String,
    pub provenance: Provenance,
    pub policy: Policy,
    pub explanation: Option<RetrievalExplanation>,
    pub metadata: Option<Metadata>,
}

/// Retrieval candidate source backed by sqlite-vec nearest-neighbor search.
///
/// The index is protected by a mutex because rusqlite connections are not
/// shared concurrently. Search remains deterministic and resolver-owned target
/// rehydration keeps stale vector rows from leaking into context.
pub struct VectorRetrievalIndex {
    index: Arc<Mutex<SqliteVectorIndex>>,
    query_provider: Arc<dyn VectorQueryProvider>,
    target_resolver: Arc<dyn VectorTargetResolver>,
    default_limit: u32,
}

impl VectorRetrievalIndex {
    /// Creates a vector retrieval index with the default candidate limit.
    ///
    /// The default limit applies only when the retrieval request does not
    /// provide `limit` or `budget.max_items`.
    pub fn new(
        index: SqliteVectorIndex,
        query_provider: Arc<dyn VectorQueryProvider>,
        target_resolver: Arc<dyn VectorTargetResolver>,
    ) -> Self {
        Self::with_default_limit(index, query_provider, target_resolver, 20)
    }

    /// Creates a vector retrieval index with an explicit fallback limit.
    ///
    /// A zero fallback limit is allowed and makes requests without their own
    /// limit return no vector candidates.
    pub fn with_default_limit(
        index: SqliteVectorIndex,
        query_provider: Arc<dyn VectorQueryProvider>,
        target_resolver: Arc<dyn VectorTargetResolver>,
        default_limit: u32,
    ) -> Self {
        Self {
            index: Arc::new(Mutex::new(index)),
            query_provider,
            target_resolver,
            default_limit,
        }
    }
}

#[async_trait]
impl RetrievalIndex for VectorRetrievalIndex {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let query = self.query_provider.query_vector(request)?;
        let limit = search_limit(request, self.default_limit);
        let hits = {
            let index = self.index.lock().map_err(|_| CoreError::Adapter {
                adapter: "engram-store-vector".to_owned(),
                message: "vector index lock poisoned".to_owned(),
            })?;
            index.search(&query, limit)?
        };

        let mut results = Vec::new();
        for (rank, hit) in hits.iter().enumerate() {
            let Some(target) = self.target_resolver.resolve(hit, request)? else {
                continue;
            };
            results.push(vector_result(rank, hit, target));
        }
        Ok(results)
    }
}

fn search_limit(request: &RetrievalRequest, default_limit: u32) -> u32 {
    request
        .limit
        .or_else(|| request.budget.as_ref().and_then(|budget| budget.max_items))
        .unwrap_or(default_limit)
}

fn vector_result(
    rank: usize,
    hit: &VectorSearchResult,
    target: VectorResolvedTarget,
) -> RetrievalResult {
    let score = similarity_from_distance(hit.distance);
    RetrievalResult {
        id: format!("vector-result-{}", hit.id),
        target_type: target.target_type,
        target_id: target.target_id,
        content: target.content,
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
            recency: None,
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: target.provenance,
        policy: target.policy,
        explanation: target.explanation,
        fusion_trace: Some(FusionTrace {
            source: "vector.semantic".to_owned(),
            source_rank: Some((rank + 1) as u32),
            source_score: Some(score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(score),
            deduplicated_with: Vec::new(),
        }),
        metadata: target.metadata,
    }
}

fn similarity_from_distance(distance: f32) -> f32 {
    1.0 / (1.0 + distance.max(0.0))
}
