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
    ContextPayload, Metadata, RetrievalRequest, RetrievalResult, RetrievalScore,
    RetrievalSourceFailure, RetrievalTargetType, SourceFailureSeverity,
};
use engram_integration::{EmbeddingProvider, KnowledgeQuery, UnifiedRecall};
use engram_knowledge::{CodeBindingRepository as _, KnowledgeRepository as _};
use engram_memory::MemoryService;
use engram_retrieval::{
    CodeEligibilityResolver, ReciprocalRankFusion, RetrievalCompositionInput, VectorIndex as _,
    compose_context, resolve_view,
};
use engram_runtime::{CoreError, CoreResult};
use engram_store_pgvector::{PgBeliefStore, PgKnowledgeStore, PgMemoryService, PgVectorIndex};

/// Vector-lane candidate cap per request (mirrors the sqlite lane's bounded k).
const VECTOR_LANE_LIMIT: usize = 10;
/// Chars of chunk text surfaced per candidate (agent-facing budget).
const CHUNK_TEXT_CAP: usize = 2 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure classification slice of the constrained-lane contract (corrective
    /// r2, finding 4): classification is durable-lineage driven — a lineage
    /// record on a published binding passes; a lineage record on another
    /// generation fails; a code-kind record without lineage fails; a
    /// non-code record without lineage passes.
    #[test]
    fn allowed_target_classification_uses_durable_lineage() {
        use engram_retrieval::{BoundBinding, DurableLineage};
        let eligibility = engram_retrieval::BoundCodeEligibility {
            epoch: 2,
            bindings: vec![BoundBinding {
                repo_id: "repo".to_owned(),
                branch_ref: "refs/heads/main".to_owned(),
                generation_id: "gen-published".to_owned(),
            }],
            lineage: vec![
                DurableLineage {
                    record_id: "chunk-published".to_owned(),
                    repo_id: "repo".to_owned(),
                    generation_id: "gen-published".to_owned(),
                },
                DurableLineage {
                    record_id: "chunk-stale".to_owned(),
                    repo_id: "repo".to_owned(),
                    generation_id: "gen-old".to_owned(),
                },
            ],
        };
        let view = engram_retrieval::BoundCodeView::from_eligibility(&eligibility);
        assert!(view.allows("chunk-published", true));
        assert!(
            !view.allows("chunk-stale", true),
            "lineage on a non-published generation never passes"
        );
        assert!(
            !view.allows("chunk-unknown-code-kind", true),
            "a code-kind record without durable lineage is legacy unbound code"
        );
        assert!(
            view.allows("chunk-ordinary", false),
            "ordinary documents always pass"
        );
    }
}

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
    /// Bound-code eligibility (task_013 seam): when wired, the vector lane
    /// constrains its candidate set to published-generation chunk ids BEFORE
    /// the `LIMIT`, and every fused candidate is re-classified before rerank
    /// input / `compose_context`. `None` keeps the historical lane.
    pub(crate) code_eligibility: Option<Arc<dyn CodeEligibilityResolver>>,
    /// Scoped chunk listing (engine-neutral `KnowledgeQuery` port) backing the
    /// eligible-target set for [`Self::code_eligibility`].
    pub(crate) knowledge_query: Option<Arc<dyn KnowledgeQuery>>,
}

impl PgUnifiedRecall {
    /// The allowed vector-lane target set for a resolved view: every in-scope
    /// chunk whose metadata classifies as non-code or eligible code. The set
    /// is the `id = ANY($n)` constraint fed to
    /// [`PgVectorIndex::search_constrained`], so the SQL ranking never
    /// considers ineligible rows (task_013 seam).
    async fn eligible_vector_targets(
        &self,
        view: &engram_retrieval::BoundCodeView,
        scope: &engram_domain::Scope,
    ) -> CoreResult<std::collections::BTreeSet<String>> {
        let query = self
            .knowledge_query
            .as_ref()
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "bound vector recall requires a scoped knowledge query".into(),
            })?;
        let mut allowed = std::collections::BTreeSet::new();
        for chunk in query.list_chunks(scope).await? {
            // Corrective r2 (finding 4): classification by DURABLE LINEAGE +
            // chunk kind — an unstamped code-kind chunk is legacy unbound
            // code and never enters the allowed set.
            let code_kind = matches!(
                chunk.kind,
                engram_domain::KnowledgeChunkKind::CodeSymbol
                    | engram_domain::KnowledgeChunkKind::CodeBlock
                    | engram_domain::KnowledgeChunkKind::DiffHunk
            );
            if view.allows(chunk.id.as_str(), code_kind) {
                allowed.insert(chunk.id.to_string());
            }
        }
        Ok(allowed)
    }

    /// The vector lane: embed → pgvector search → chunk rehydration. Errors
    /// degrade (the caller records a source failure); an unwired lane or an
    /// empty index is a no-op, not a failure.
    ///
    /// With `code_eligibility` wired (task_013), the search runs
    /// [`PgVectorIndex::search_constrained`] over the eligible chunk-id set:
    /// eligibility applies inside the SQL ranking, before the limit — never a
    /// global top-k that then drops rows.
    async fn vector_lane(&self, request: &RetrievalRequest) -> CoreResult<Vec<RetrievalResult>> {
        let (Some(vectors), Some(embedder)) = (&self.vectors, &self.embedder) else {
            return Ok(Vec::new());
        };
        let query_vector = embedder.embed_query(&request.query)?;
        let view = match &self.code_eligibility {
            Some(resolver) => Some(
                resolve_view(
                    resolver.as_ref(),
                    &request.scope,
                    request
                        .filters
                        .as_ref()
                        .and_then(|filters| filters.code_index_keys.as_deref()),
                )
                .await?,
            ),
            None => None,
        };
        let hits = match &view {
            Some(view) => {
                let allowed = self.eligible_vector_targets(view, &request.scope).await?;
                // No eligible targets at all: the lane contributes nothing
                // rather than widening to an unconstrained page (fail closed,
                // never silently unbound).
                if allowed.is_empty() {
                    return Ok(Vec::new());
                }
                vectors
                    .search_constrained(
                        &embedder.embedding_space(),
                        query_vector,
                        VECTOR_LANE_LIMIT,
                        &allowed,
                    )
                    .await?
            }
            None => {
                let space = embedder.embedding_space();
                vectors
                    .search(&space, query_vector, VECTOR_LANE_LIMIT)
                    .await?
            }
        };
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

        // Post-lane guard (task_013): every fused candidate is re-classified
        // against the published generation set before compose_context, so an
        // ineligible code target cannot enter the context budget even if a
        // lane could not constrain internally. Non-code targets pass.
        if let Some(resolver) = &self.code_eligibility {
            let requested = request
                .filters
                .as_ref()
                .and_then(|filters| filters.code_index_keys.as_deref());
            let view = resolve_view(resolver.as_ref(), &request.scope, requested).await?;
            candidates.retain(|candidate| view.allows(candidate.target_id.as_str(), false));
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
