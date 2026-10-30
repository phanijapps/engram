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
use engram_integration::{EmbeddingProvider, KnowledgeQuery, UnifiedRecall};
use engram_knowledge::KnowledgeRepository as _;
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
                    retired: false,
                    source_path: String::new(),
                    span: None,
                },
                DurableLineage {
                    record_id: "chunk-stale".to_owned(),
                    repo_id: "repo".to_owned(),
                    generation_id: "gen-old".to_owned(),
                    retired: false,
                    source_path: String::new(),
                    span: None,
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
    /// Canonical candidate evidence for degraded code-authority handling
    /// (corrective r3, finding 6): when the eligibility authority ERRORS,
    /// positively authorized ordinary records (documents, concept entities,
    /// memories, beliefs) survive through canonical repository evidence —
    /// no target-kind proxy denies every chunk/entity wholesale.
    pub(crate) candidate_evidence: Option<Arc<dyn engram_retrieval::CandidateEvidenceResolver>>,
    /// Graph entity+chunk lane over the engine-neutral `KnowledgeQuery` port
    /// (corrective r4: the recipe carries ENTITY candidates too, so positive
    /// ordinary entities are provable through the composed recall — the
    /// SQLite recipe's graph lane mirrored at the same port boundary).
    pub(crate) graph_lane: Option<Arc<dyn engram_retrieval::RetrievalIndex>>,
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

        // Graph lane: entity + chunk candidates over the knowledge port.
        if let Some(graph_lane) = &self.graph_lane {
            match graph_lane.retrieve_candidates(&request).await {
                Ok(graph_results) => candidates.extend(graph_results),
                Err(e) => source_failures.push(lane("graph", e)),
            }
        }

        // Post-lane guard (task_013): authority resolution is itself a
        // degradable source.  A failed authority read must not abort ordinary
        // memory/belief recall, but it must deny code-capable chunk/entity
        // candidates rather than silently widening the page.
        let mut authority_failed = false;
        let code_view = match &self.code_eligibility {
            Some(resolver) => {
                let requested = request
                    .filters
                    .as_ref()
                    .and_then(|filters| filters.code_index_keys.as_deref());
                match resolve_view(resolver.as_ref(), &request.scope, requested).await {
                    Ok(view) => Some(view),
                    Err(error) => {
                        source_failures.push(lane("code_eligibility", error));
                        authority_failed = true;
                        None
                    }
                }
            }
            None => None,
        };
        if let Some(view) = &code_view {
            // Corrective r4 (canonical 6c): with canonical evidence
            // configured, a SUCCESSFUL view validates every candidate's
            // canonical CODE DEPENDENCIES against that view — derived
            // memories/beliefs built from stale code deny, positively
            // authorized independent ordinary records keep flowing, Unknown
            // fails closed.
            //
            // Work package A3 (r5): acceptance requires `complete` — a fully
            // validated dependency set; the aggregate-only trait default can
            // never pass. A code-dependent acceptance additionally requires
            // a NONEMPTY code set (never a vacuous `.all` over a
            // default-empty vector), and a MISSING evidence resolver is not
            // derived-record independence: without it the guard fails closed
            // identically to the authority-failure branch (identical
            // semantics to the SQLite recipe).
            if let Some(evidence) = &self.candidate_evidence {
                let mut allowed = Vec::with_capacity(candidates.len());
                for candidate in candidates {
                    match evidence
                        .classify_with_dependencies(&candidate, &request)
                        .await
                    {
                        Ok(classification) => {
                            let dependency_eligible = classification.complete
                                && !classification.code_dependencies.is_empty()
                                && classification
                                    .code_dependencies
                                    .iter()
                                    .all(|dependency| view.allows(dependency.as_str(), true));
                            if classification.complete
                                && (classification.evidence
                                    == engram_retrieval::CandidateEvidence::Ordinary
                                    || (classification.evidence
                                        == engram_retrieval::CandidateEvidence::CodeDependent
                                        && dependency_eligible))
                            {
                                allowed.push(candidate);
                            }
                        }
                        Err(error) => source_failures.push(lane("candidate_evidence", error)),
                    }
                }
                candidates = allowed;
            } else {
                // Missing evidence resolver ≠ derived-record independence
                // (A3): own-id lineage cannot prove a memory/belief free of
                // stale code dependencies, so every candidate fails closed
                // while code bindings exist and the denial is surfaced.
                source_failures.push(lane(
                    "candidate_evidence",
                    CoreError::Adapter {
                        adapter: "candidate-evidence".to_string(),
                        message: "evidence resolver not configured; candidates fail \
                                 closed while code bindings exist"
                            .to_string(),
                    },
                ));
                candidates.clear();
            }
        } else if authority_failed {
            // Corrective r3 (finding 6) + review r5 (finding 6): degrade
            // through CANONICAL EVIDENCE with the SAME completeness contract
            // as the successful-view path — `classify_with_dependencies`,
            // accepting ONLY complete, positively ordinary evidence. An
            // aggregate-only resolver cannot pass (its classification is
            // explicitly incomplete). Without an evidence resolver the
            // recipe fails closed entirely (corrective r4, canonical 6c).
            match &self.candidate_evidence {
                Some(evidence) => {
                    let mut allowed = Vec::with_capacity(candidates.len());
                    for candidate in candidates {
                        match evidence
                            .classify_with_dependencies(&candidate, &request)
                            .await
                        {
                            Ok(classification) => {
                                if classification.complete
                                    && classification.evidence
                                        == engram_retrieval::CandidateEvidence::Ordinary
                                {
                                    allowed.push(candidate)
                                }
                            }
                            Err(error) => source_failures.push(lane("candidate_evidence", error)),
                        }
                    }
                    candidates = allowed;
                }
                // Corrective r4 (canonical 6c): without canonical evidence
                // NO category is exempt — Memory/Belief included. Positive
                // ordinary evidence cannot be established while the code
                // authority is unavailable, so every code-capable candidate
                // denies (the composed payload then carries only facts-lane
                // degradation already filtered above and the surfaced
                // failures explain the emptiness).
                None => candidates.clear(),
            }
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

/// Graph entity+chunk lane over the engine-neutral `KnowledgeQuery` port:
/// token-rank entities by name and chunks by text (the SQLite recipe's graph
/// lane mirrored; entities carry the graph-lane default policy shape).
pub(crate) struct PgGraphLane {
    query: Arc<dyn KnowledgeQuery>,
    default_limit: u32,
    eligibility: Option<Arc<dyn CodeEligibilityResolver>>,
}

impl PgGraphLane {
    pub(crate) fn new(query: Arc<dyn KnowledgeQuery>) -> Self {
        Self {
            query,
            default_limit: 20,
            eligibility: None,
        }
    }

    pub(crate) fn with_eligibility(
        query: Arc<dyn KnowledgeQuery>,
        eligibility: Arc<dyn CodeEligibilityResolver>,
    ) -> Self {
        Self {
            query,
            default_limit: 20,
            eligibility: Some(eligibility),
        }
    }
}

#[async_trait::async_trait]
impl engram_retrieval::RetrievalIndex for PgGraphLane {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = request
            .limit
            .or_else(|| request.budget.as_ref().and_then(|budget| budget.max_items))
            .unwrap_or(self.default_limit) as usize;
        let entities = self.query.list_entities(&request.scope).await?;
        let chunks = self.query.list_chunks(&request.scope).await?;
        let view = match &self.eligibility {
            Some(eligibility) => Some(engram_retrieval::BoundCodeView::from_eligibility(
                &eligibility.resolve(&request.scope).await?,
            )),
            None => None,
        };
        let tokens: Vec<String> = request
            .query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|token| token.len() > 2)
            .map(str::to_owned)
            .collect();
        let mut scored: Vec<(f32, RetrievalResult)> = Vec::new();
        for entity in entities {
            let hay = entity.name.to_lowercase();
            let score: f32 = tokens
                .iter()
                .map(|token| {
                    if hay == *token {
                        3.0
                    } else if hay.starts_with(token.as_str()) || hay.ends_with(token.as_str()) {
                        2.0
                    } else if hay.contains(token.as_str()) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .sum();
            if score <= 0.0 {
                continue;
            }
            scored.push((
                score,
                RetrievalResult {
                    id: format!("graph-entity-{}", entity.id),
                    target_type: RetrievalTargetType::Entity,
                    target_id: entity.id.to_string(),
                    content: entity.name.clone(),
                    score: RetrievalScore {
                        total: score,
                        relevance: Some(score),
                        recency: None,
                        confidence: None,
                        cue_match: None,
                        hierarchical_fit: None,
                        policy_fit: Some(1.0),
                    },
                    provenance: entity.provenance.clone(),
                    policy: graph_default_policy(),
                    explanation: None,
                    fusion_trace: None,
                    metadata: entity.metadata.clone(),
                },
            ));
        }
        for chunk in chunks {
            let code_kind = matches!(
                chunk.kind,
                engram_domain::KnowledgeChunkKind::CodeSymbol
                    | engram_domain::KnowledgeChunkKind::CodeBlock
                    | engram_domain::KnowledgeChunkKind::DiffHunk
            );
            if view
                .as_ref()
                .is_some_and(|view| !view.allows(chunk.id.as_str(), code_kind))
            {
                continue;
            }
            let hay = chunk.text.to_lowercase();
            let score: f32 = tokens
                .iter()
                .map(|token| {
                    if hay.contains(token.as_str()) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .sum();
            if score <= 0.0 {
                continue;
            }
            let text: String = chunk.text.chars().take(2000).collect();
            scored.push((
                score,
                RetrievalResult {
                    id: format!("graph-chunk-{}", chunk.id),
                    target_type: RetrievalTargetType::Chunk,
                    target_id: chunk.id.to_string(),
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
                    provenance: chunk.provenance.clone(),
                    policy: chunk.policy.clone(),
                    explanation: None,
                    fusion_trace: None,
                    metadata: chunk.metadata.clone(),
                },
            ));
        }
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.target_id.cmp(&b.1.target_id))
        });
        Ok(scored
            .into_iter()
            .take(limit)
            .map(|(_, result)| result)
            .collect())
    }
}

/// Entities carry no canonical policy field; the graph-lane default is the
/// ONE domain constructor — graphless entity/relationship policy is governed
/// by [`engram_domain::Policy::graph_lane_default`] everywhere, with no
/// per-adapter synthetic duplicate.
fn graph_default_policy() -> engram_domain::Policy {
    engram_domain::Policy::graph_lane_default()
}

#[cfg(test)]
mod authority_tests {
    //! Corrective r3 (finding 6) Pg-recipe proof: an eligibility authority
    //! ERROR degrades the recall through CANONICAL EVIDENCE — positively
    //! authorized ordinary records survive, code-dependent/unknown records
    //! deny, and the failures surface as source failures. Runs ONLY against
    //! the approved marked disposable fixture (marker asserted inside), gated
    //! behind `--ignored` + `ENGRAM_TEST_PG_URL` like the accepted PG suites.

    use super::*;
    use async_trait::async_trait;
    use engram_domain::{
        Actor, ActorKind, AllowedUse, Belief, BeliefId, BeliefStatus, BeliefSubject, DeleteMode,
        EntityKind, EvidenceRef, EvidenceTargetType, Id, KnowledgeChunk, KnowledgeChunkKind,
        KnowledgeEntity, KnowledgeSource, MemoryContent, MemoryKind, MemoryRecord, MemoryStatus,
        Policy, Provenance, Requester, Retention, Sensitivity, SourceKind, Visibility,
    };
    use engram_knowledge::CodeBindingRepository;
    use engram_retrieval::{CandidateEvidence, CandidateEvidenceResolver, VectorIndex};
    use engram_store_pgvector::{
        PgBeliefStore, PgCodeBindingRepository, PgConnection, PgKnowledgeStore,
    };
    use futures::executor::block_on;

    fn url() -> String {
        std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required")
    }

    /// The approved disposable-fixture marker: refuse any other database.
    fn marker_assert(conn: &PgConnection) {
        let marker = conn
            .block_on(async {
                conn.client
                    .query_one(
                        "SELECT shobj_description(oid,'pg_database') FROM pg_database \
                         WHERE datname=current_database()",
                        &[],
                    )
                    .await
            })
            .expect("marker query");
        assert_eq!(
            marker.get::<_, Option<String>>(0).as_deref(),
            Some("engram.factory.job=010-project-bound-branch-indexing; disposable=true"),
            "disposable fixture marker mismatch — refusing to touch any other database"
        );
    }

    fn scope(run: &str) -> engram_domain::Scope {
        engram_domain::Scope {
            tenant: "factory-test".into(),
            subject: None,
            workspace: Some(format!("r3-pg-recall-{run}")),
            session: None,
            environment: None,
        }
    }

    fn actor() -> Actor {
        Actor {
            id: Id::from("r3-pg-recall"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn requester() -> Requester {
        Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        }
    }

    fn policy() -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "r3-pg-recall".to_string(),
            actor: actor(),
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_string()),
        }
    }

    fn provenance_with(refs: Vec<EvidenceRef>) -> Provenance {
        let mut provenance = provenance();
        provenance.evidence = refs;
        provenance
    }

    fn evidence_ref(target_type: EvidenceTargetType, id: &str) -> EvidenceRef {
        EvidenceRef {
            target_type,
            target_id: Some(id.to_string()),
            uri: None,
            quote: None,
            location: None,
        }
    }

    /// Deterministic embedder at the model boundary (the Pg vector CELL and
    /// the lane are real; only the embedding model is a port double).
    struct FixedEmbedder;

    impl EmbeddingProvider for FixedEmbedder {
        fn provider_id(&self) -> &str {
            "test"
        }
        fn model_id(&self) -> &str {
            "fixed"
        }
        fn dimensions(&self) -> u32 {
            2
        }
        fn embedding_space(&self) -> engram_domain::EmbeddingSpace {
            engram_domain::EmbeddingSpace::new("test", "fixed", 2, "query", None::<String>)
        }
        fn embed_query(&self, _query: &str) -> CoreResult<Vec<f32>> {
            Ok(vec![0.0, 1.0])
        }
        fn embed_passage(&self, _text: &str) -> CoreResult<Vec<f32>> {
            Ok(vec![0.0, 1.0])
        }
    }

    struct ErroringResolver;
    #[async_trait::async_trait]
    impl CodeEligibilityResolver for ErroringResolver {
        async fn resolve(
            &self,
            _scope: &engram_domain::Scope,
        ) -> CoreResult<engram_retrieval::BoundCodeEligibility> {
            Err(CoreError::Adapter {
                adapter: "test-authority".to_string(),
                message: "authority unavailable".to_string(),
            })
        }
    }

    fn chunk(run: &str, id: &str, text: &str, kind: KnowledgeChunkKind) -> KnowledgeChunk {
        KnowledgeChunk {
            id: Id::from(id),
            document_id: Id::from(format!("{run}-doc")),
            source_id: Id::from(run_source_id(run)),
            kind,
            text: text.to_string(),
            summary: None,
            location: None,
            entities: Vec::new(),
            concepts: Vec::new(),
            embedding_refs: Vec::new(),
            content_hash: format!("sha256:{id}"),
            provenance: provenance(),
            policy: policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }
    }

    fn entity(run: &str, id: &str, name: &str, kind: EntityKind) -> KnowledgeEntity {
        KnowledgeEntity {
            id: Id::from(id),
            graph_id: None,
            kind,
            name: name.to_string(),
            aliases: Vec::new(),
            scope: scope(run),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            archived_at: None,
            metadata: None,
        }
    }

    fn run_source_id(run: &str) -> Id {
        Id::from(format!("{run}-source"))
    }

    fn scope_current() -> engram_domain::Scope {
        scope(&scope_run())
    }

    fn scope_run() -> String {
        format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    /// Corrective r4 (canonical 2, direct cell proof): the Pg
    /// `validate_historical_read_lease` contract — presented-live lease
    /// validates; released, expired, foreign-scope, wrong-generation,
    /// wrong-repo and record-mismatched leases all deny. Generation
    /// ownership alone is not this proof. The fixture takes real durable
    /// custody of gen-old, then a real publication advances the binding to
    /// gen-cur, so the validated generation is genuinely historical.
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL against the approved marked disposable fixture"]
    fn pg_historical_lease_validator_direct_cases() {
        use engram_knowledge::CodeEligibility;
        let run = scope_run();
        let conn = PgConnection::connect(&url()).expect("connect disposable fixture");
        marker_assert(&conn);
        conn.block_on(async {
            conn.client
                .batch_execute(&engram_store_pgvector::schema::schema_sql(2))
                .await
                .expect("apply schema")
        });
        let test_scope = scope(&run);
        let repo = engram_domain::RepoId("repo-lease".to_string());
        let bindings = PgCodeBindingRepository::new(PgConnection::connect(&url()).expect("b"))
            .expect("binding repository");
        block_on(bindings.bind(engram_domain::CodeBinding {
            scope: test_scope.clone(),
            repo_id: repo.clone(),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: Some("gen-old".to_string()),
            epoch: 0,
        }))
        .expect("bind");
        // Fixture-truth repair (parent decision Option 1): the lease is
        // acquired against the DURABLE eligibility of this moment — exactly
        // like every green lease suite — never a hand-stamped snapshot, so
        // the scope really owns gen-old when custody is taken.
        let eligibility = block_on(bindings.eligibility(&test_scope)).expect("durable eligibility");
        let lease = |id: &str,
                     eligibility: CodeEligibility,
                     expires: engram_domain::Timestamp|
         -> engram_knowledge::CodeReadLease {
            engram_knowledge::CodeReadLease {
                id: format!("lease-{run}-{id}"),
                eligibility,
                fencing_token: 1,
                expires_at: expires,
            }
        };
        let now = chrono::Utc::now();
        let live = block_on(engram_knowledge::CodeReadLeases::acquire_read_lease(
            &bindings,
            lease(
                "live",
                eligibility.clone(),
                now + chrono::Duration::minutes(10),
            ),
            now,
        ))
        .expect("acquire live lease");
        // All three leases take custody BEFORE the world advances, each
        // under its own id (a released id is tombstoned forever, so ids are
        // never reused).
        let short = block_on(engram_knowledge::CodeReadLeases::acquire_read_lease(
            &bindings,
            lease(
                "short",
                eligibility.clone(),
                now + chrono::Duration::milliseconds(1),
            ),
            now,
        ))
        .expect("acquire short lease");
        let again = block_on(engram_knowledge::CodeReadLeases::acquire_read_lease(
            &bindings,
            lease(
                "again",
                eligibility.clone(),
                now + chrono::Duration::minutes(10),
            ),
            now,
        ))
        .expect("acquire again");
        // A REAL publication advances the binding from gen-old to gen-cur
        // (begin_attempt → receipts → ready → publish), making gen-old a
        // genuinely historical generation for the assertions below.
        let attempt_id = format!("attempt-{run}");
        block_on(bindings.begin_attempt(
            &test_scope,
            &repo,
            engram_domain::CodeGeneration {
                id: "gen-cur".to_string(),
                binding_version: 1,
                commit_oid: "a".repeat(40),
                snapshot_digest: "b".repeat(64),
                parser_fingerprint: "parser".to_string(),
                filter_fingerprint: "filter".to_string(),
            },
            engram_domain::CodeAttempt {
                id: attempt_id.clone(),
                idempotency_key: format!("idem-{run}"),
                binding_version: 1,
                expected_epoch: 0,
                expected_active_generation: Some("gen-old".to_string()),
                fencing_token: 0,
                state: engram_domain::CodeAttemptState::Created,
                receipt_kinds: vec![
                    engram_domain::CodeReceiptKind::Source,
                    engram_domain::CodeReceiptKind::Knowledge,
                ],
            },
        ))
        .expect("begin attempt");
        for lane in [
            engram_domain::CodeReceiptKind::Source,
            engram_domain::CodeReceiptKind::Knowledge,
        ] {
            block_on(
                bindings.record_receipt(engram_knowledge::StageReceipt {
                    attempt_id: attempt_id.clone(),
                    generation_id: "gen-cur".to_string(),
                    lane: serde_json::to_value(&lane)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    digest: format!("digest-{run}-{lane:?}"),
                }),
            )
            .expect("receipt");
        }
        let (_, stored) = block_on(bindings.get_attempt(&test_scope, &repo, &attempt_id))
            .expect("get attempt")
            .expect("attempt exists");
        block_on(bindings.mark_ready(&test_scope, &repo, &attempt_id, stored.fencing_token))
            .expect("mark ready");
        block_on(bindings.publish(&test_scope, &repo, &attempt_id, 0)).expect("publish");

        // (1) Presented LIVE lease: validates for the pinned generation.
        assert!(
            block_on(
                engram_knowledge::CodeReadLeases::validate_historical_read_lease(
                    &bindings,
                    &live,
                    &test_scope,
                    &repo.0,
                    "gen-old",
                    chrono::Utc::now(),
                )
            )
            .expect("validate")
        );

        // (2) RELEASED lease denies.
        block_on(engram_knowledge::CodeReadLeases::release_read_lease(
            &bindings, &live,
        ))
        .expect("release");
        assert!(
            !block_on(
                engram_knowledge::CodeReadLeases::validate_historical_read_lease(
                    &bindings,
                    &live,
                    &test_scope,
                    &repo.0,
                    "gen-old",
                    chrono::Utc::now(),
                )
            )
            .expect("validate released")
        );

        // (3) EXPIRED lease denies.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !block_on(
                engram_knowledge::CodeReadLeases::validate_historical_read_lease(
                    &bindings,
                    &short,
                    &test_scope,
                    &repo.0,
                    "gen-old",
                    chrono::Utc::now(),
                )
            )
            .expect("validate expired")
        );

        // (4) Canonical-equality: a MODIFIED copy of a live durable lease
        // (record mismatch) denies.
        let mut tampered = again.clone();
        tampered.expires_at += chrono::Duration::minutes(5);
        assert!(
            !block_on(
                engram_knowledge::CodeReadLeases::validate_historical_read_lease(
                    &bindings,
                    &tampered,
                    &test_scope,
                    &repo.0,
                    "gen-old",
                    chrono::Utc::now(),
                )
            )
            .expect("validate tampered")
        );

        // (5) Foreign scope / wrong generation / wrong repo deny.
        assert!(
            !block_on(
                engram_knowledge::CodeReadLeases::validate_historical_read_lease(
                    &bindings,
                    &again,
                    &scope(&format!("{run}-foreign")),
                    &repo.0,
                    "gen-old",
                    chrono::Utc::now(),
                )
            )
            .expect("validate foreign scope")
        );
        assert!(
            !block_on(
                engram_knowledge::CodeReadLeases::validate_historical_read_lease(
                    &bindings,
                    &again,
                    &test_scope,
                    &repo.0,
                    "gen-never",
                    chrono::Utc::now(),
                )
            )
            .expect("validate wrong generation")
        );
        assert!(
            !block_on(
                engram_knowledge::CodeReadLeases::validate_historical_read_lease(
                    &bindings,
                    &again,
                    &test_scope,
                    "repo-other",
                    "gen-old",
                    chrono::Utc::now(),
                )
            )
            .expect("validate wrong repo")
        );
    }

    /// The authority-error degradation proof over the real Pg recipe.
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL against the approved marked disposable fixture"]
    fn pg_authority_failure_preserves_ordinary_and_denies_code_dependent() {
        let run = scope_run();
        let conn = PgConnection::connect(&url()).expect("connect disposable fixture");
        marker_assert(&conn);
        conn.block_on(async {
            conn.client
                .batch_execute(&engram_store_pgvector::schema::schema_sql(2))
                .await
                .expect("apply schema")
        });
        let test_scope = scope(&run);

        let memory = Arc::new(PgMemoryService::new(
            PgConnection::connect(&url()).expect("m"),
        ));
        let beliefs = Arc::new(PgBeliefStore::new(
            PgConnection::connect(&url()).expect("b"),
        ));
        let knowledge = Arc::new(PgKnowledgeStore::new(
            PgConnection::connect(&url()).expect("k"),
        ));
        let ownership = Arc::new(
            PgCodeBindingRepository::new(PgConnection::connect(&url()).expect("o"))
                .expect("binding repository"),
        );

        // Canonical seed: an ordinary source + document chunk + code chunk,
        // ordinary/code entities (with real durable lineage on the code one),
        // an ordinary memory, a code-DERIVED memory and an ordinary belief.
        let source = KnowledgeSource {
            id: Id::from(run_source_id(&run)),
            kind: SourceKind::Upload,
            scope: test_scope.clone(),
            name: "r3 pg source".to_string(),
            uri: None,
            version: None,
            repository_key: None,
            git_ref: None,
            policy: policy(),
            provenance: provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        };
        block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::put_source(
                &knowledge, source,
            ),
        )
        .expect("put source");
        block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::put_document(
                &knowledge,
                engram_domain::SourceDocument {
                    id: Id::from(format!("{run}-doc")),
                    source_id: Id::from(run_source_id(&run)),
                    kind: engram_domain::SourceDocumentKind::Text,
                    uri: None,
                    path: Some("fixture.rs".to_string()),
                    title: None,
                    mime_type: None,
                    language: Some("rust".to_string()),
                    version: None,
                    content_hash: "sha256:r3-pg-doc".to_string(),
                    provenance: provenance(),
                    policy: policy(),
                    created_at: chrono::Utc::now(),
                    updated_at: None,
                    metadata: None,
                },
            ),
        )
        .expect("put document");
        block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::put_chunk(
                &knowledge,
                chunk(
                    &run,
                    &format!("{run}-chunk-doc"),
                    "parse ordinary note",
                    KnowledgeChunkKind::Paragraph,
                ),
            ),
        )
        .expect("put doc chunk");
        block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::put_chunk(
                &knowledge,
                chunk(
                    &run,
                    &format!("{run}-chunk-code"),
                    "parse code symbol",
                    KnowledgeChunkKind::CodeSymbol,
                ),
            ),
        )
        .expect("put code chunk");
        block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::put_entity(
                &knowledge,
                entity(
                    &run,
                    &format!("{run}-entity-concept"),
                    "parseConcept",
                    EntityKind::Concept,
                ),
            ),
        )
        .expect("put concept entity");
        block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::put_entity(
                &knowledge,
                entity(
                    &run,
                    &format!("{run}-entity-symbol"),
                    "parseSymbol",
                    EntityKind::Function,
                ),
            ),
        )
        .expect("put symbol entity");
        block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::put_entity(
                &knowledge,
                entity(
                    &run,
                    &format!("{run}-entity-plain"),
                    "parsePlain",
                    EntityKind::Concept,
                ),
            ),
        )
        .expect("put positive ordinary entity");
        // Real durable lineage: the CONCEPT entity is code-owned by lineage.
        block_on(engram_knowledge::CodeBindingRepository::put_lineage(
            &*ownership,
            &test_scope,
            &format!("{run}-entity-concept"),
            engram_domain::CodeLineage {
                repo_id: engram_domain::RepoId("repo-a".to_string()),
                generation_id: "gen-a".to_string(),
                source_path: "src/x.rs".to_string(),
                contributors: vec![format!("{run}-entity-concept")],
                span: None,
            },
        ))
        .expect("put lineage");

        let ordinary_memory = block_on(engram_memory::MemoryRepository::put_memory(
            &*memory,
            MemoryRecord {
                id: Id::from(format!("{run}-memory-ordinary")),
                kind: MemoryKind::Observation,
                content: MemoryContent {
                    text: "parse ordinary memory".to_string(),
                    summary: None,
                    entities: Vec::new(),
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                status: MemoryStatus::Active,
                scope: test_scope.clone(),
                policy: policy(),
                provenance: provenance(),
                links: Vec::new(),
                verdict: None,
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
                assertions: Vec::new(),
            },
        ))
        .expect("put ordinary memory");
        let code_derived_memory = block_on(engram_memory::MemoryRepository::put_memory(
            &*memory,
            MemoryRecord {
                id: Id::from(format!("{run}-memory-code")),
                kind: MemoryKind::Observation,
                content: MemoryContent {
                    text: "parse derived from code".to_string(),
                    summary: None,
                    entities: Vec::new(),
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                status: MemoryStatus::Active,
                scope: test_scope.clone(),
                policy: policy(),
                provenance: provenance_with(vec![evidence_ref(
                    EvidenceTargetType::Chunk,
                    &format!("{run}-chunk-code"),
                )]),
                links: Vec::new(),
                verdict: None,
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
                assertions: Vec::new(),
            },
        ))
        .expect("put code-derived memory");
        block_on(engram_belief::BeliefRepository::put_belief(
            &*beliefs,
            Belief {
                id: BeliefId::from(format!("{run}-belief-ordinary")),
                scope: test_scope.clone(),
                subject: BeliefSubject {
                    key: "parse".to_string(),
                    entity_ref: None,
                    concept_ref: None,
                    aliases: Vec::new(),
                },
                content: "parse ordinary belief".to_string(),
                status: BeliefStatus::Active,
                confidence: 0.9,
                sources: Vec::new(),
                valid_from: None,
                valid_until: None,
                superseded_by: None,
                stale: None,
                synthesizer: None,
                reasoning: None,
                embedding_refs: Vec::new(),
                policy: policy(),
                provenance: provenance(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            },
        ))
        .expect("put ordinary belief");

        // Real vector cell rows so the (failing) vector lane is genuinely
        // wired over the real Pg recipe.
        let vectors = Arc::new(
            PgVectorIndex::new_checked(
                PgConnection::connect(&url()).expect("v"),
                FixedEmbedder.embedding_space(),
            )
            .expect("vector index"),
        );
        for id in [format!("{run}-chunk-doc"), format!("{run}-chunk-code")] {
            block_on(vectors.insert(
                &Id::from(id),
                &FixedEmbedder.embedding_space(),
                vec![0.0, 1.0],
            ))
            .expect("insert vector");
        }

        let erroring: Arc<dyn CodeEligibilityResolver> = Arc::new(ErroringResolver);
        let recall = PgUnifiedRecall {
            memory: memory.clone(),
            beliefs: beliefs.clone(),
            knowledge: knowledge.clone(),
            vectors: Some(vectors),
            embedder: Some(Arc::new(FixedEmbedder)),
            code_eligibility: Some(erroring),
            knowledge_query: None,
            graph_lane: Some(Arc::new(super::PgGraphLane::new(Arc::new(
                crate::query::PgKnowledgeQuery::new(
                    PgConnection::connect(&url()).expect("graph lane"),
                ),
            )))),
            candidate_evidence: Some(Arc::new(
                engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                    memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                    beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                    knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                    memory.retrieval_authorizer(),
                    Some(ownership.clone() as Arc<dyn engram_knowledge::CodeBindingRepository>),
                    Some(knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
                ),
            )),
        };

        let request = RetrievalRequest {
            query: "parse".to_string(),
            scope: test_scope.clone(),
            requester: requester(),
            modes: Vec::new(),
            filters: None,
            cues: Vec::new(),
            limit: Some(10),
            budget: None,
            include_explanations: None,
        };
        let payload = block_on(engram_integration::UnifiedRecall::recall(
            &recall,
            request.clone(),
        ))
        .expect("degraded recall composes");
        let ids: Vec<String> = payload.items.iter().map(|i| i.target_id.clone()).collect();
        assert!(
            ids.contains(&format!("{run}-memory-ordinary")),
            "the positively authorized ordinary memory survives the authority error: {ids:?}"
        );
        assert!(
            ids.contains(&format!("{run}-belief-ordinary")),
            "the positively authorized ordinary belief survives: {ids:?}"
        );
        assert!(
            !ids.contains(&format!("{run}-memory-code")),
            "the code-derived memory is denied through canonical evidence: {ids:?}"
        );
        // Corrective r4 (four kinds through the REAL composed recipe,
        // authority-error path): positive ordinary Chunk AND Entity survive
        // alongside Memory/Belief; the code counterparts deny.
        assert!(
            ids.contains(&format!("{run}-chunk-doc")),
            "positive ordinary CHUNK survives through the composed recipe: {ids:?}"
        );
        assert!(
            ids.contains(&format!("{run}-entity-plain")),
            "positive ordinary ENTITY survives through the composed recipe: {ids:?}"
        );
        assert!(
            !ids.contains(&format!("{run}-chunk-code"))
                && !ids.contains(&format!("{run}-entity-symbol")),
            "code chunk/entity deny through the composed recipe: {ids:?}"
        );
        let failures: Vec<String> = payload
            .source_failures
            .iter()
            .map(|f| f.source.clone())
            .collect();
        assert!(
            failures.contains(&"code_eligibility".to_string()),
            "the authority error is surfaced: {failures:?}"
        );
        assert!(
            failures.contains(&"vector".to_string()),
            "the failing vector lane degrades: {failures:?}"
        );

        // Corrective r4 (canonical 6c): a SUCCESSFUL view with evidence
        // validates code DEPENDENCIES — the stale-derived memory denies
        // while ALL FOUR ordinary kinds keep flowing through the recipe.
        let working: Arc<dyn CodeEligibilityResolver> = Arc::new(
            engram_integration::code_binding::RepositoryEligibilitySource::new(ownership.clone()),
        );
        let working_recall = PgUnifiedRecall {
            memory: memory.clone(),
            beliefs: beliefs.clone(),
            knowledge: knowledge.clone(),
            vectors: None,
            embedder: None,
            code_eligibility: Some(working),
            knowledge_query: None,
            graph_lane: Some(Arc::new(super::PgGraphLane::new(Arc::new(
                crate::query::PgKnowledgeQuery::new(
                    PgConnection::connect(&url()).expect("working graph lane"),
                ),
            )))),
            candidate_evidence: Some(Arc::new(
                engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                    memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                    beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                    knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                    memory.retrieval_authorizer(),
                    Some(ownership.clone() as Arc<dyn engram_knowledge::CodeBindingRepository>),
                    Some(knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
                ),
            )),
        };
        let working_payload = block_on(engram_integration::UnifiedRecall::recall(
            &working_recall,
            request,
        ))
        .expect("successful-view recall composes");
        let working_ids: Vec<String> = working_payload
            .items
            .iter()
            .map(|i| i.target_id.clone())
            .collect();
        assert!(
            working_ids.contains(&format!("{run}-chunk-doc"))
                && working_ids.contains(&format!("{run}-entity-plain"))
                && working_ids.contains(&format!("{run}-memory-ordinary"))
                && working_ids.contains(&format!("{run}-belief-ordinary")),
            "ALL FOUR ordinary kinds flow on the successful path: {working_ids:?}"
        );
        assert!(
            !working_ids.contains(&format!("{run}-memory-code")),
            "the stale-code-derived memory denies on the successful path: {working_ids:?}"
        );

        // The same owned seam classifies every category over the REAL Pg
        // stores + real durable lineage: ordinary chunk/entity positively
        // ordinary; code chunk, code-kind entity and lineage-owned entity
        // code-dependent.
        let evidence = engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
            memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
            beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
            knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
            memory.retrieval_authorizer(),
            Some(ownership.clone() as Arc<dyn engram_knowledge::CodeBindingRepository>),
            // Real graph repository, mirroring the composed views above: the
            // graphless fixture records classify through graph_lane_default
            // via the configured authorizer, while a graph-owned record
            // would resolve its canonical graph policy instead of denying.
            Some(knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
        );
        let request = RetrievalRequest {
            query: "parse".to_string(),
            scope: test_scope.clone(),
            requester: requester(),
            modes: Vec::new(),
            filters: None,
            cues: Vec::new(),
            limit: Some(10),
            budget: None,
            include_explanations: None,
        };
        let doc_chunk = block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::get_chunk(
                &knowledge,
                &Id::from(format!("{run}-chunk-doc")),
                &test_scope,
            ),
        )
        .expect("get doc chunk")
        .expect("doc chunk exists");
        let classify = |result: &RetrievalResult| {
            block_on(evidence.evidence(result, &request)).expect("evidence resolution")
        };
        let chunk_result = |id: &str, text: &str, provenance: &Provenance| RetrievalResult {
            id: format!("candidate-{id}"),
            target_type: RetrievalTargetType::Chunk,
            target_id: id.to_string(),
            content: text.to_string(),
            score: RetrievalScore {
                total: 1.0,
                relevance: Some(1.0),
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: Some(1.0),
            },
            provenance: provenance.clone(),
            policy: policy(),
            explanation: None,
            fusion_trace: None,
            metadata: None,
        };
        assert_eq!(
            classify(&chunk_result(
                &format!("{run}-chunk-doc"),
                &doc_chunk.text.clone(),
                &doc_chunk.provenance
            )),
            CandidateEvidence::Ordinary,
            "ordinary document chunk is positively ordinary over the Pg stores"
        );
        let code_chunk = block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::get_chunk(
                &knowledge,
                &Id::from(format!("{run}-chunk-code")),
                &test_scope,
            ),
        )
        .expect("get code chunk")
        .expect("code chunk exists");
        assert_eq!(
            classify(&chunk_result(
                &format!("{run}-chunk-code"),
                &code_chunk.text.clone(),
                &code_chunk.provenance
            )),
            CandidateEvidence::CodeDependent
        );
        let entity_result = |id: &str, name: &str, provenance: &Provenance| RetrievalResult {
            id: format!("candidate-{id}"),
            target_type: RetrievalTargetType::Entity,
            target_id: id.to_string(),
            content: name.to_string(),
            score: RetrievalScore {
                total: 1.0,
                relevance: Some(1.0),
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: Some(1.0),
            },
            provenance: provenance.clone(),
            policy: policy(),
            explanation: None,
            fusion_trace: None,
            metadata: None,
        };
        let symbol = block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::get_entity(
                &knowledge,
                &Id::from(format!("{run}-entity-symbol")),
                &test_scope,
            ),
        )
        .expect("get symbol entity")
        .expect("symbol entity exists");
        assert_eq!(
            classify(&entity_result(
                &format!("{run}-entity-symbol"),
                &symbol.name.clone(),
                &symbol.provenance
            )),
            CandidateEvidence::CodeDependent,
            "code-kind entities are legacy/code dependent"
        );
        let lineage_owned = block_on(
            <PgKnowledgeStore as engram_knowledge::KnowledgeRepository>::get_entity(
                &knowledge,
                &Id::from(format!("{run}-entity-concept")),
                &test_scope,
            ),
        )
        .expect("get concept entity")
        .expect("concept entity exists");
        assert_eq!(
            classify(&entity_result(
                &format!("{run}-entity-concept"),
                &lineage_owned.name.clone(),
                &lineage_owned.provenance
            )),
            CandidateEvidence::CodeDependent,
            "durable code lineage dominates entity kind over the real Pg catalog"
        );
        let _ = ordinary_memory;
        let _ = code_derived_memory;
    }
}
