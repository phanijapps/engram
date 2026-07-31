//! Postgres (pgvector) backend bootstrap — engine-specific zone (ADR-0022 exempt).

use std::sync::Arc;

use engram_domain::{CapabilityState, EmbeddingSpace};
use engram_runtime::{CoreError, CoreResult};
use engram_store_pgvector::{
    PgBeliefStore, PgConnection, PgHierarchyStore, PgKnowledgeStore, PgMemoryService,
    PgProcedureStore, PgVectorIndex, schema,
};

use crate::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder, UnifiedRecall};

/// Bootstraps a Postgres-backed provider.
pub fn bootstrap_pgvector(config: &EngramConfig) -> CoreResult<EngramProvider> {
    let conn_str = config
        .pgvector_connection_string
        .as_deref()
        .ok_or_else(|| CoreError::InvalidRequest {
            reason: "pgvector_connection_string is required for the pgvector backend".to_owned(),
        })?;

    let pg_err = |e: String| CoreError::Adapter {
        adapter: "engram-store-pgvector".to_owned(),
        message: e,
    };

    // Connect + apply schema (idempotent).
    let schema_conn = PgConnection::connect(conn_str).map_err(pg_err)?;
    let dims = config.embedding_provider.dimensions;
    schema_conn
        .block_on(async {
            schema_conn
                .client
                .batch_execute(&schema::schema_sql(dims))
                .await
        })
        .map_err(|e| pg_err(e.to_string()))?;

    // Construct cells (each gets its own connection; a pool follows).
    let mk_conn = || PgConnection::connect(conn_str).map_err(pg_err);

    let knowledge = Arc::new(PgKnowledgeStore::new(mk_conn()?));
    let memory = Arc::new(PgMemoryService::new(mk_conn()?));
    let beliefs = Arc::new(PgBeliefStore::new(mk_conn()?));
    let hierarchy = Arc::new(PgHierarchyStore::new(mk_conn()?));
    let procedures = Arc::new(PgProcedureStore::new(mk_conn()?));

    let space = EmbeddingSpace::new(
        &config.embedding_provider.provider_type,
        &config.embedding_provider.model,
        dims,
        &config.embedding_provider.prompt_profile,
        config.embedding_provider.normalization.clone(),
    );
    let vectors = Arc::new(PgVectorIndex::new(mk_conn()?, space));

    // Unified recall: composes memory.retrieve + beliefs via RRF.
    let recall = Arc::new(PgUnifiedRecall {
        memory: memory.clone(),
        beliefs: beliefs.clone(),
    });

    let report = CapabilityReport::builder()
        .memory(CapabilityState::Supported)
        .knowledge(CapabilityState::Supported)
        .graph(CapabilityState::Supported)
        .beliefs(CapabilityState::Supported)
        .contradiction(CapabilityState::Supported)
        .hierarchy(CapabilityState::Supported)
        .procedures(CapabilityState::Supported)
        .vectors(CapabilityState::Supported)
        .unified_recall(CapabilityState::Supported)
        .build();

    let provider = EngramProviderBuilder::new(report)
        .memory(memory)
        .knowledge(knowledge.clone())
        .graph(knowledge)
        .beliefs(beliefs)
        .hierarchy(hierarchy)
        .procedures(procedures)
        .vectors(vectors)
        .recall(recall);

    Ok(provider.build())
}

/// Postgres-backed UnifiedRecall: composes memory.retrieve + beliefs via RRF.
struct PgUnifiedRecall {
    memory: Arc<PgMemoryService>,
    beliefs: Arc<PgBeliefStore>,
}

#[async_trait::async_trait]
impl UnifiedRecall for PgUnifiedRecall {
    async fn recall(
        &self,
        request: engram_domain::RetrievalRequest,
    ) -> CoreResult<engram_domain::ContextPayload> {
        use engram_belief::{BeliefQuery, BeliefRepository};
        use engram_domain::{RetrievalResult, RetrievalScore, RetrievalTargetType};
        use engram_memory::MemoryService;
        use engram_retrieval::{ReciprocalRankFusion, RetrievalCompositionInput, compose_context};

        let now = chrono::Utc::now();
        let mut candidates: Vec<engram_domain::RetrievalResult> = Vec::new();

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
