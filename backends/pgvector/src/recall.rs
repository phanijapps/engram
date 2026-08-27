//! Postgres-backed unified recall: composes memory (facts), beliefs, and the
//! vector lane via RRF (pgvector-backend PS2).
//!
//! Lanes, each attempted independently (a lane error degrades via
//! `source_failures` — the unified-recall-api contract: never fail the whole
//! recall because one lane is unavailable):
//! - **Facts** — `memory.retrieve`.
//! - **Beliefs** — `BeliefQuery::live_subject` (0-or-1 candidate).
//! - **Vector** — when an `EmbeddingProvider` is wired: embed the query,
//!   `PgVectorIndex::search` (chunk-id keyed), rehydrate chunks via the
//!   knowledge store, and emit cosine-similarity-scored candidates. Absent
//!   embedder or empty index ⇒ lane contributes nothing (no failure).
//!
//! The lexical (tsvector) lane remains a documented gap on this engine —
//! SQLite fuses BM25 there; Postgres deployments run recall degraded for
//! keyword until the tsvector lane ships (pgvector-backend spec, PS2 note).

use std::sync::Arc;

use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::{
    ContextPayload, RetrievalRequest, RetrievalResult, RetrievalScore, RetrievalSourceFailure,
    RetrievalTargetType, SourceFailureSeverity,
};
use engram_integration::{EmbeddingProvider, UnifiedRecall};
use engram_knowledge::KnowledgeRepository as _;
use engram_memory::MemoryService;
use engram_retrieval::{
    ReciprocalRankFusion, RetrievalCompositionInput, VectorIndex as _, compose_context,
};
use engram_runtime::{CoreError, CoreResult};
use engram_store_pgvector::{PgBeliefStore, PgKnowledgeStore, PgMemoryService, PgVectorIndex};

/// Vector-lane candidate cap per request (mirrors the sqlite lane's bounded k).
const VECTOR_LANE_LIMIT: usize = 10;
/// Chars of chunk text surfaced per candidate (agent-facing budget).
const CHUNK_TEXT_CAP: usize = 2 * 1024;

/// Composes the Postgres facts + beliefs + vector lanes into one fused recall.
pub(crate) struct PgUnifiedRecall {
    pub(crate) memory: Arc<PgMemoryService>,
    pub(crate) beliefs: Arc<PgBeliefStore>,
    pub(crate) knowledge: Arc<PgKnowledgeStore>,
    /// Vector index over knowledge chunks (chunk-id keyed). `None` ⇒ no vector
    /// lane (degraded recall, no failure).
    pub(crate) vectors: Option<Arc<PgVectorIndex>>,
    /// Query-vector provider; `None` ⇒ no vector lane.
    pub(crate) embedder: Option<Arc<dyn EmbeddingProvider>>,
}

impl PgUnifiedRecall {
    /// The vector lane: embed → pgvector search → chunk rehydration. Errors
    /// degrade (the caller records a source failure); an unwired lane or an
    /// empty index is a no-op, not a failure.
    async fn vector_lane(&self, request: &RetrievalRequest) -> CoreResult<Vec<RetrievalResult>> {
        let (Some(vectors), Some(embedder)) = (&self.vectors, &self.embedder) else {
            return Ok(Vec::new());
        };
        let query_vector = embedder.embed_query(&request.query)?;
        let space = embedder.embedding_space();
        let hits = vectors
            .search(&space, query_vector, VECTOR_LANE_LIMIT)
            .await?;
        let mut results = Vec::with_capacity(hits.len());
        for (chunk_id, score) in hits {
            let Some(chunk) = self.knowledge.get_chunk(&chunk_id, &request.scope).await? else {
                continue;
            };
            if chunk.text.trim().is_empty() {
                continue;
            }
            let text: String = chunk.text.chars().take(CHUNK_TEXT_CAP).collect();
            results.push(RetrievalResult {
                id: format!("result-{chunk_id}"),
                target_type: RetrievalTargetType::Chunk,
                target_id: chunk_id.to_string(),
                content: text,
                score: RetrievalScore {
                    total: score,
                    relevance: Some(score),
                    recency: None,
                    confidence: None,
                    cue_match: None,
                    hierarchical_fit: None,
                    policy_fit: Some(1.0),
                },
                provenance: chunk.provenance,
                policy: chunk.policy,
                explanation: None,
                fusion_trace: None,
                metadata: chunk.metadata,
            });
        }
        Ok(results)
    }
}

#[async_trait::async_trait]
impl UnifiedRecall for PgUnifiedRecall {
    async fn recall(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        let now = chrono::Utc::now();
        let mut candidates: Vec<RetrievalResult> = Vec::new();
        let mut source_failures: Vec<RetrievalSourceFailure> = Vec::new();
        let lane = |name: &'static str, err: CoreError| RetrievalSourceFailure {
            source: name.to_owned(),
            mode: None,
            severity: SourceFailureSeverity::Warning,
            reason: err.to_string(),
            message: None,
            degraded: true,
        };

        // Facts lane: memory.retrieve. A failure degrades, never errors.
        match self.memory.retrieve(request.clone()).await {
            Ok(payload) => {
                candidates.extend(payload.items);
                source_failures.extend(payload.source_failures);
            }
            Err(e) => source_failures.push(lane("facts", e)),
        }

        // Beliefs lane: 0-or-1 live belief for the query subject.
        let bq = BeliefQuery::live_subject(request.scope.clone(), request.query.clone(), now);
        match self.beliefs.get_belief(bq).await {
            Ok(Some(belief)) => {
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
            Ok(None) => {}
            Err(e) => source_failures.push(lane("belief", e)),
        }

        // Vector lane (PS2): embed → pgvector → chunks.
        match self.vector_lane(&request).await {
            Ok(vector_results) => candidates.extend(vector_results),
            Err(e) => source_failures.push(lane("vector", e)),
        }

        compose_context(RetrievalCompositionInput {
            request: &request,
            fusion: &ReciprocalRankFusion::default(),
            reranker: None,
            candidates,
            omitted: vec![],
            source_failures,
            created_at: now,
        })
    }
}
