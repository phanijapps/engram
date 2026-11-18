//! Shared procedure retrieval, canonical target rehydration and code eligibility.
//! Concrete secondary-index construction and hit conversions remain adapter contributions.
use crate::storage_composition::CompositionRead;
use engram_domain::*;
use engram_runtime::CoreResult;
use std::sync::Arc;

/// Procedures recall lane (RFC-0016 Layer 6 + memory-recall triage):
/// replayable runbooks never surfaced in `recall` — the `procedures` tool
/// listed them but no lane fused them. Query-gated like the temporal lane:
/// a runbook survives only when its name/trigger/steps text shares a
/// meaningful query token; ranked by match count (then name). Engine-
/// neutral — built directly on the `ProcedureRepository` port.
pub fn procedures_recall_lane(
    repo: Arc<dyn engram_procedures::ProcedureRepository>,
) -> Arc<dyn engram_retrieval::RetrievalIndex> {
    Arc::new(ProceduresRetrievalIndex {
        repo,
        default_limit: 10,
    })
}

struct ProceduresRetrievalIndex {
    repo: Arc<dyn engram_procedures::ProcedureRepository>,
    default_limit: u32,
}

#[async_trait::async_trait]
impl engram_retrieval::RetrievalIndex for ProceduresRetrievalIndex {
    async fn retrieve_candidates(
        &self,
        request: &engram_domain::RetrievalRequest,
    ) -> engram_runtime::CoreResult<Vec<engram_domain::RetrievalResult>> {
        use engram_domain::{FusionStrategy, FusionTrace, RetrievalScore, RetrievalTargetType};
        let limit = request
            .limit
            .or_else(|| request.budget.as_ref().and_then(|b| b.max_items))
            .unwrap_or(self.default_limit);
        let tokens = engram_retrieval::query_tokens(request.query.as_str());
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let procedures = self.repo.list_procedures(&request.scope).await?;
        let mut scored: Vec<(f32, engram_domain::Procedure)> = procedures
            .into_iter()
            .filter_map(|p| {
                let hay = format!(
                    "{} {} {}",
                    p.name,
                    p.trigger.as_deref().unwrap_or(""),
                    p.steps.join(" ")
                )
                .to_lowercase();
                let hits = tokens.iter().filter(|t| hay.contains(t.as_str())).count() as f32;
                if hits == 0.0 {
                    None
                } else {
                    Some((hits / tokens.len() as f32, p))
                }
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.name.cmp(&b.1.name))
        });
        Ok(scored
            .into_iter()
            .take(limit.max(1) as usize)
            .map(|(score, p)| engram_domain::RetrievalResult {
                id: format!("result-{}", p.id),
                target_type: RetrievalTargetType::Rule,
                target_id: p.id.to_string(),
                content: format!(
                    "[procedure] {} — trigger: {}. Steps: {}. (successes: {}, failures: {})",
                    p.name,
                    p.trigger.as_deref().unwrap_or("-"),
                    p.steps.join(" → "),
                    p.success_count,
                    p.failure_count
                ),
                score: RetrievalScore {
                    total: score,
                    relevance: Some(score),
                    recency: None,
                    confidence: None,
                    cue_match: None,
                    hierarchical_fit: None,
                    policy_fit: Some(1.0),
                },
                provenance: p.provenance.clone(),
                policy: p.policy.clone(),
                explanation: None,
                metadata: None,
                fusion_trace: Some(FusionTrace {
                    query_id: None,
                    vector_index: None,
                    embedding_time_ms: None,
                    search_time_ms: None,
                    source: "procedures".to_owned(),
                    source_rank: None,
                    source_score: Some(score),
                    score: None,
                    rank: None,
                    fusion_strategy: Some(FusionStrategy::None),
                    fusion_score: Some(score),
                    rerank_strategy: Some(engram_domain::RerankStrategy::None),
                    rerank_score: None,
                    discard_reason: None,
                    deduplicated_with: Vec::new(),
                }),
            })
            .collect())
    }
}

/// Canonical target fields before an adapter applies its index score.
pub struct ResolvedKnowledgeTarget {
    pub target_type: RetrievalTargetType,
    pub target_id: String,
    pub content: String,
    pub provenance: Provenance,
    pub policy: Policy,
    pub explanation: Option<RetrievalExplanation>,
    pub metadata: Option<Metadata>,
    pub code_kind: bool,
}
/// Rehydrates secondary-index identities from scope-bounded canonical records.
pub struct KnowledgeTargetResolver {
    store: Arc<dyn CompositionRead>,
    deny_unbound_symbols: bool,
}
fn is_symbol_kind(entity: &KnowledgeEntity) -> bool {
    matches!(
        entity.kind,
        engram_domain::EntityKind::Class
            | engram_domain::EntityKind::Function
            | engram_domain::EntityKind::Method
            | engram_domain::EntityKind::Struct
            | engram_domain::EntityKind::Interface
            | engram_domain::EntityKind::Trait
            | engram_domain::EntityKind::TypeAlias
            | engram_domain::EntityKind::Enum
    )
}

impl KnowledgeTargetResolver {
    pub fn new(store: Arc<dyn CompositionRead>) -> Self {
        Self {
            store,
            deny_unbound_symbols: false,
        }
    }
    pub fn deny_unbound_symbols(mut self) -> Self {
        self.deny_unbound_symbols = true;
        self
    }
    pub async fn resolve(
        &self,
        target_id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<ResolvedKnowledgeTarget>> {
        // (1) Chunk resolution — the common case for docs/memories. Preserves
        // the prior behavior exactly: a chunk-id hit rehydrates its canonical
        // `KnowledgeChunk`.
        let chunk_id = ChunkId::from(target_id);
        if let Some(chunk) = self.store.get_chunk(&chunk_id, &request.scope).await? {
            return Ok(Some(chunk_to_target(chunk)));
        }
        // (2) Entity resolution — code symbols. `scan_repo` feeds the lexical
        // index keyed by *entity id* (one entry per code symbol, text
        // `"{name} {kind}"`), so a BM25 hit on an entity id must resolve to its
        // entity. Before this branch such hits resolved to `Ok(None)` and were
        // silently dropped by the lane (`let Some(..) else continue`), so
        // multi-term symbol queries returned nothing. Trying chunk-first keeps
        // chunk resolution unchanged; the entity branch is purely additive —
        // entity ids that previously dropped now resolve instead.
        //
        // Code-artifact eligibility (2026-10-20): when the request suppresses
        // code artifacts (default), an entity-id hit resolving to a suppressed
        // kind (Module/File/Repository/Variable) resolves to None — the lane's
        // over-fetch refills the page. This is THE eligibility seam for the
        // lexical lane; the chunk branch is unaffected.
        let entity_id = EntityId::from(target_id);
        if let Some(entity) = self.store.get_entity(&entity_id, &request.scope).await? {
            if engram_retrieval::suppresses_code_artifacts(request)
                && engram_retrieval::is_code_artifact(&entity)
            {
                return Ok(None);
            }
            // Bound mode (task_013): a symbol entity with repository
            // provenance but no generation stamp is legacy unbound code — deny
            // at rehydration. Entities WITH a stamp flow to the lane's
            // view classification (stale generations drop there).
            if self.deny_unbound_symbols
                && is_symbol_kind(&entity)
                && !entity
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get(engram_retrieval::bound_filter::CODE_GENERATION_KEY))
                    .is_some()
            {
                return Ok(None);
            }
            return Ok(Some(entity_to_target(entity)));
        }
        Ok(None)
    }

    pub async fn eligible_target_ids(
        &self,
        request: &RetrievalRequest,
        view: &engram_retrieval::BoundCodeView,
    ) -> CoreResult<std::collections::BTreeSet<String>> {
        // Resolve canonical records before Tantivy collection.  A record with
        // durable eligible lineage is allowed, as is a positively typed
        // ordinary non-code chunk/entity.  Code kinds with absent, forged, or
        // mismatched metadata remain code and cannot enter via `doc`.
        let mut ids = std::collections::BTreeSet::new();
        for chunk in self.store.list_chunks(&request.scope).await? {
            if view.allows(
                chunk.id.as_str(),
                matches!(
                    chunk.kind,
                    KnowledgeChunkKind::CodeSymbol
                        | KnowledgeChunkKind::CodeBlock
                        | KnowledgeChunkKind::DiffHunk
                ),
            ) {
                ids.insert(chunk.id.to_string());
            }
        }
        for entity in self.store.list_entities(&request.scope).await? {
            if view.allows(entity.id.as_str(), is_code_entity_kind(&entity)) {
                ids.insert(entity.id.to_string());
            }
        }
        Ok(ids)
    }
}

/// Shapes a resolved chunk as a lexical-lane retrieval target.
fn chunk_to_target(chunk: KnowledgeChunk) -> ResolvedKnowledgeTarget {
    let code_kind = matches!(
        chunk.kind,
        KnowledgeChunkKind::CodeSymbol
            | KnowledgeChunkKind::CodeBlock
            | KnowledgeChunkKind::DiffHunk
    );
    ResolvedKnowledgeTarget {
        target_type: RetrievalTargetType::Chunk,
        target_id: chunk.id.to_string(),
        content: chunk.text,
        provenance: chunk.provenance,
        policy: chunk.policy,
        explanation: None,
        metadata: chunk.metadata,
        // Corrective r2 (finding 4): the record's KIND is classification
        // input; durable lineage is the code authority.
        code_kind,
    }
}

/// Shapes a resolved entity as a lexical-lane retrieval target (code symbols).
/// The content mirrors what `scan_repo` indexed (`"{name} {kind:?}"`) so the
/// lane's hit text stays consistent with the indexed document.
fn entity_to_target(entity: KnowledgeEntity) -> ResolvedKnowledgeTarget {
    let code_kind = is_code_entity_kind(&entity);
    ResolvedKnowledgeTarget {
        target_type: RetrievalTargetType::Entity,
        target_id: entity.id.to_string(),
        content: format!("{} {:?}", entity.name, entity.kind),
        provenance: entity.provenance,
        // `KnowledgeEntity` carries no policy field; code symbols index
        // workspace-visible + durable + retrieval-allowed, mirroring the policy
        // the graph / associative-graph lanes assign their entity candidates.
        policy: entity_symbol_policy(),
        explanation: None,
        metadata: entity.metadata,
        // Corrective r2 (finding 4): symbol + structural-artifact entity kinds
        // are code kinds; durable lineage is the code authority.
        code_kind,
    }
}

/// True for symbol and structural code-artifact entity kinds (the kinds the
/// scanner produces for code). Shared by every resolver/lane shape here so
/// classification inputs agree.
pub fn is_code_entity_kind(entity: &KnowledgeEntity) -> bool {
    engram_retrieval::is_code_artifact(entity)
        || matches!(
            entity.kind,
            engram_domain::EntityKind::Class
                | engram_domain::EntityKind::Function
                | engram_domain::EntityKind::Method
                | engram_domain::EntityKind::Struct
                | engram_domain::EntityKind::Interface
                | engram_domain::EntityKind::Trait
                | engram_domain::EntityKind::TypeAlias
                | engram_domain::EntityKind::Enum
        )
}

/// Default retrieval policy for indexed code-symbol entities (entities carry no
/// policy field). Mirrors the graph / associative-graph lanes' entity policy.
fn entity_symbol_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}
