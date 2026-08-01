//! Postgres-backed unified recall: composes memory.retrieve + beliefs via RRF.

use std::sync::Arc;

use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::{
    ContextPayload, RetrievalRequest, RetrievalResult, RetrievalScore, RetrievalTargetType,
};
use engram_memory::MemoryService;
use engram_integration::UnifiedRecall;
use engram_retrieval::{ReciprocalRankFusion, RetrievalCompositionInput, compose_context};
use engram_runtime::CoreResult;
use engram_store_pgvector::{PgBeliefStore, PgMemoryService};

/// Composes the Postgres memory + belief cells into one fused recall lane.
pub(crate) struct PgUnifiedRecall {
    pub(crate) memory: Arc<PgMemoryService>,
    pub(crate) beliefs: Arc<PgBeliefStore>,
}

#[async_trait::async_trait]
impl UnifiedRecall for PgUnifiedRecall {
    async fn recall(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        let now = chrono::Utc::now();
        let mut candidates: Vec<RetrievalResult> = Vec::new();

        // Facts lane: memory.retrieve.
        if let Ok(payload) = self.memory.retrieve(request.clone()).await {
            candidates.extend(payload.items);
        }

        // Beliefs lane.
        let bq = BeliefQuery::live_subject(request.scope.clone(), request.query.clone(), now);
        if let Ok(Some(belief)) = self.beliefs.get_belief(bq).await {
            candidates.push(RetrievalResult {
                id: format!("result-{}", belief.id),
                target_type: RetrievalTargetType::Belief,
                target_id: belief.id.to_string(),
                content: belief.content,
                score: RetrievalScore {
                    total: belief.confidence,
                    relevance: Some(belief.confidence),
                    recency: None,
                    confidence: Some(belief.confidence),
                    cue_match: None,
                    hierarchical_fit: None,
                    policy_fit: Some(1.0),
                },
                provenance: belief.provenance,
                policy: belief.policy,
                explanation: None,
                fusion_trace: None,
                metadata: belief.metadata,
            });
        }

        compose_context(RetrievalCompositionInput {
            request: &request,
            fusion: &ReciprocalRankFusion::default(),
            reranker: None,
            candidates,
            omitted: vec![],
            source_failures: vec![],
            created_at: now,
        })
    }
}
