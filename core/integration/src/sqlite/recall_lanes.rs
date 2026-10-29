//! Knowledge-store-backed target resolvers for the unified-recall retrieval lanes.
//!
//! The lexical (`engram-store-lexical`) and vector (`engram-store-vector`)
//! retrieval lanes return adapter-level hits — `(target_id, score)` and
//! [`VectorSearchResult`] respectively. A [`RetrievalIndex`] lane needs the
//! canonical target content / provenance / policy to shape a portable
//! `RetrievalResult`. These resolvers rehydrate hits from the
//! [`SqlKnowledgeStore`]'s existing chunk reads, so vector + lexical storage
//! stays secondary adapter state (the store remains the source of truth) —
//! mirroring the in-test stub resolvers in the adapter crates.
//!
//! ADR-0022: engine-specific (names `Sql*`, holds the knowledge adapter). The
//! resolvers live under `core/integration/src/sqlite/` behind the `sqlite`
//! feature, exempt from the engine-neutrality gate.
//!
//! # Sync resolvers + async store
//!
//! The lexical resolver is **async** (it awaits the store's `get_chunk`), so it
//! composes safely inside the unified-recall async path. (Earlier it was sync
//! and drove `get_chunk` via `block_on` — but `futures::executor::block_on`
//! *does* panic on re-entry into an already-running `LocalPool`, so any caller
//! that drives recall with its own `block_on` — e.g. the MCP transport — hit a
//! nested-`block_on` panic, observed as the subprocess dying. Awaiting is the
//! fix.) The fastembed-gated vector resolver still uses the sync `resolve_chunk`
//! helper and wants the same treatment.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    AllowedUse, ChunkId, DeleteMode, EntityId, KnowledgeChunk, KnowledgeChunkKind, KnowledgeEntity,
    KnowledgeRelationship, Policy, Retention, RetrievalRequest, RetrievalTargetType, Scope,
    Sensitivity, Visibility,
};
use engram_knowledge::KnowledgeRepository;
use engram_retrieval::{CodeEligibilityResolver, GraphCache, RetrievalIndex};
use engram_runtime::CoreResult;
use engram_store_associative_graph::{AssociativeGraphIndex, GraphRelationshipSource};
use engram_store_community_summary::CommunitySummaryIndex;
use engram_store_lexical::{
    LexicalIndex, LexicalResolvedTarget, LexicalRetrievalIndex, LexicalTargetResolver,
};
use engram_store_sqlite::SqlKnowledgeStore;
use engram_store_sqlite::{VectorResolvedTarget, VectorSearchResult, VectorTargetResolver};

/// Orphan-rule wrapper adapting `SqlKnowledgeStore` to the associative-graph
/// edge source (mirrors `bindings/node/src/knowledge_fusion.rs`). A bare
/// `impl GraphRelationshipSource for SqlKnowledgeStore` is forbidden in this
/// crate (neither the trait nor the store type is local); this newtype is the
/// local type the impl hangs on.
pub(crate) struct KnowledgeRelationshipSource(pub(crate) Arc<SqlKnowledgeStore>);

#[async_trait]
impl GraphRelationshipSource for KnowledgeRelationshipSource {
    async fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        self.0.list_entities(scope).await
    }
    async fn relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.0.list_relationships(scope).await
    }
}

/// Builds the associative-graph retrieval lane over a knowledge store: the
/// `RetrievalIndex` that ranks entities by Personalized PageRank seeded at query
/// entities. Exposed `pub` (its signature names `SqlKnowledgeStore`, an engine
/// type — acceptable because it lives in the engine-specific `sqlite` mod, the
/// ADR-0022 exempt zone) so the conformance tests construct the lane directly
/// and assert lane-level behavior in isolation from `bootstrap_sqlite`; the
/// production bootstrap consumes it via the same call. One orphan-rule newtype,
/// no per-consumer wrapper duplication.
///
/// Pass a shared `cache` so the lane serves the in-scope entities +
/// relationships from a materialized snapshot on a cache hit (the snapshot is
/// shared across the graph lanes — a miss here still benefits the sibling lanes
/// on their next query). Pass `None` for the no-cache / test path.
pub fn associative_recall_lane(
    store: Arc<SqlKnowledgeStore>,
    cache: Option<Arc<dyn GraphCache>>,
) -> Arc<dyn RetrievalIndex> {
    let source: Arc<dyn GraphRelationshipSource> = Arc::new(KnowledgeRelationshipSource(store));
    match cache {
        Some(cache) => Arc::new(AssociativeGraphIndex::with_cache(
            source,
            Default::default(),
            cache,
        )),
        None => Arc::new(AssociativeGraphIndex::new(source)),
    }
}

/// Builds the community-summary retrieval lane (GraphRAG-style) over a
/// knowledge store. Mirrors [`associative_recall_lane`].
///
/// Pass a shared `cache` so the lane serves the in-scope entities +
/// relationships from a materialized snapshot on a cache hit. Pass `None` for
/// the no-cache / test path.
pub fn community_summary_recall_lane(
    store: Arc<SqlKnowledgeStore>,
    cache: Option<Arc<dyn GraphCache>>,
) -> Arc<dyn RetrievalIndex> {
    let source: Arc<dyn GraphRelationshipSource> = Arc::new(KnowledgeRelationshipSource(store));
    match cache {
        Some(cache) => Arc::new(CommunitySummaryIndex::with_cache(source, 20, cache)),
        None => Arc::new(CommunitySummaryIndex::new(source)),
    }
}

/// Builds the REAL SQLite vector retrieval lane with bound-code eligibility
/// wired into its candidate path (corrective r2, finding 5): sqlite-vec
/// selection is constrained to the allowed target set BEFORE the top-K, so
/// an ineligible nearest vector cannot crowd out an eligible one even at
/// top-K = 1. The query embedder stays the injected `VectorQueryProvider`
/// port (the model boundary); the vector cell and the lane are real.
pub fn vector_recall_lane_bound(
    store: Arc<SqlKnowledgeStore>,
    index: engram_store_sqlite::SqliteVectorIndex,
    query_provider: Arc<dyn engram_store_sqlite::VectorQueryProvider>,
    eligibility: Arc<dyn CodeEligibilityResolver>,
) -> Arc<dyn RetrievalIndex> {
    let resolver = KnowledgeVectorResolver::new(store.clone());
    Arc::new(
        engram_store_sqlite::VectorRetrievalIndex::with_default_limit_bound(
            index,
            query_provider,
            Arc::new(resolver),
            20,
            eligibility,
            store,
        ),
    )
}

/// Builds the lexical (BM25) retrieval lane over a shared Tantivy index + the
/// knowledge store's entity-aware target resolver. Exposed `pub` (mirrors
/// [`associative_recall_lane`] / [`community_summary_recall_lane`]) so the
/// conformance tests construct the lane directly over a seeded store — the
/// production bootstrap wiring and the tests exercise the same constructor.
///
/// The lexical index is shared by-value (`Arc`) so a `LexicalFeed` (writes) and
/// this lane (reads) operate over one in-RAM Tantivy index — exactly how
/// `scan_repo` feeds code-symbol names that this lane then ranks.
pub fn lexical_recall_lane(
    store: Arc<SqlKnowledgeStore>,
    lexical_index: Arc<LexicalIndex>,
) -> Arc<dyn RetrievalIndex> {
    let resolver = KnowledgeLexicalResolver::new(store);
    Arc::new(LexicalRetrievalIndex::from_arc(
        lexical_index,
        Arc::new(resolver),
    ))
}

/// [`lexical_recall_lane`] with bound-code eligibility wired end to end
/// (task_013 seam): the lane resolves the scope's published generations once
/// per request, constrains BM25 collection to the eligible contributor set
/// BEFORE the limit, and the resolver denies legacy unbound code symbols at
/// rehydration so they never enter the candidate page or a fallback path.
pub fn lexical_recall_lane_bound(
    store: Arc<SqlKnowledgeStore>,
    lexical_index: Arc<LexicalIndex>,
    eligibility: Arc<dyn CodeEligibilityResolver>,
) -> Arc<dyn RetrievalIndex> {
    let resolver = KnowledgeLexicalResolver::new(store).deny_unbound_symbols();
    Arc::new(LexicalRetrievalIndex::from_arc_with_eligibility(
        lexical_index,
        Arc::new(resolver),
        eligibility,
    ))
}

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

/// Lexical-lane target resolver backed by the knowledge store: rehydrates a
/// BM25 hit's chunk id into its canonical `KnowledgeChunk`.
pub(crate) struct KnowledgeLexicalResolver {
    store: Arc<SqlKnowledgeStore>,
    /// Bound mode (task_013): symbol entities with NO `codeGenerationId` stamp
    /// are legacy unbound code and resolve to `None` — unbound code lineage
    /// yields a denial, it never leaks through a documents fallback.
    deny_unbound_symbols: bool,
}

/// Entity kinds that are code symbols (structural stubs are covered by the
/// existing `is_code_artifact` suppression). Used only for the legacy-unbound
/// denial in bound mode — entities carrying a generation stamp are classified
/// by stamp, not by kind.
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

impl KnowledgeLexicalResolver {
    /// Wraps a shared knowledge-store handle to resolve chunk hits.
    pub(crate) fn new(store: Arc<SqlKnowledgeStore>) -> Self {
        Self {
            store,
            deny_unbound_symbols: false,
        }
    }

    /// Switches the resolver into bound mode: symbol entities without a
    /// generation stamp never rehydrate (they predate the binding system).
    pub(crate) fn deny_unbound_symbols(mut self) -> Self {
        self.deny_unbound_symbols = true;
        self
    }
}

#[async_trait]
impl LexicalTargetResolver for KnowledgeLexicalResolver {
    async fn resolve(
        &self,
        target_id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<LexicalResolvedTarget>> {
        // (1) Chunk resolution — the common case for docs/memories. Preserves
        // the prior behavior exactly: a chunk-id hit rehydrates its canonical
        // `KnowledgeChunk`.
        let chunk_id = ChunkId::from(target_id);
        if let Some(chunk) = self.store.get_chunk(&chunk_id, &request.scope).await? {
            return Ok(Some(chunk_to_lexical(chunk)));
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
            return Ok(Some(entity_to_lexical(entity)));
        }
        Ok(None)
    }
}

/// Vector-lane target resolver backed by the knowledge store: rehydrates a
/// sqlite-vec hit into its canonical `KnowledgeChunk`.
pub(crate) struct KnowledgeVectorResolver {
    store: Arc<SqlKnowledgeStore>,
}

impl KnowledgeVectorResolver {
    /// Wraps a shared knowledge-store handle to resolve vector hits.
    pub(crate) fn new(store: Arc<SqlKnowledgeStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl VectorTargetResolver for KnowledgeVectorResolver {
    async fn resolve(
        &self,
        hit: &VectorSearchResult,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<VectorResolvedTarget>> {
        let chunk = resolve_chunk(&self.store, &hit.target_id, &request.scope).await?;
        Ok(chunk.map(chunk_to_vector))
    }
}

/// Looks up a chunk by id + scope from the knowledge store.
///
/// `target_id` comes from a vector secondary-index hit; the store is the
/// canonical source, so a stale or scope-invisible hit returns `None` (the lane
/// skips it) rather than synthesizing a phantom candidate. (fastembed-only; the
/// lexical resolver awaits `get_chunk` directly — see `KnowledgeLexicalResolver`.)
async fn resolve_chunk(
    store: &Arc<SqlKnowledgeStore>,
    target_id: &str,
    scope: &Scope,
) -> CoreResult<Option<KnowledgeChunk>> {
    let id = ChunkId::from(target_id);
    store.get_chunk(&id, scope).await
}

/// Shapes a resolved chunk as a lexical-lane retrieval target.
fn chunk_to_lexical(chunk: KnowledgeChunk) -> LexicalResolvedTarget {
    let code_kind = matches!(
        chunk.kind,
        KnowledgeChunkKind::CodeSymbol
            | KnowledgeChunkKind::CodeBlock
            | KnowledgeChunkKind::DiffHunk
    );
    LexicalResolvedTarget {
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
fn entity_to_lexical(entity: KnowledgeEntity) -> LexicalResolvedTarget {
    let code_kind = is_code_entity_kind(&entity);
    LexicalResolvedTarget {
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
pub(crate) fn is_code_entity_kind(entity: &KnowledgeEntity) -> bool {
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

/// Shapes a resolved chunk as a vector-lane retrieval target.
fn chunk_to_vector(chunk: KnowledgeChunk) -> VectorResolvedTarget {
    VectorResolvedTarget {
        target_type: RetrievalTargetType::Chunk,
        target_id: chunk.id.to_string(),
        content: chunk.text,
        provenance: chunk.provenance,
        policy: chunk.policy,
        explanation: None,
        metadata: chunk.metadata,
    }
}

/// Adapts the selected inference port to the vector candidate lane.
pub(super) struct EmbeddingQueryProvider(pub Arc<dyn crate::EmbeddingProvider>);
impl engram_store_sqlite::VectorQueryProvider for EmbeddingQueryProvider {
    fn query_vector(&self, request: &engram_domain::RetrievalRequest) -> CoreResult<Vec<f32>> {
        self.0.embed_query(&request.query)
    }
}

#[cfg(test)]
mod tests {
    //! The knowledge-backed resolvers are exercised end-to-end through the
    //! production `bootstrap_sqlite` wiring (see `bootstrap` tests) and the
    //! `SqlUnifiedRecall` integration tests. This module is reserved for any
    //! future resolver-only unit tests that do not require a store.
}
