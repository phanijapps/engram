//! Concrete retrieval adapter construction and storage-specific hit conversions.
pub use crate::recall_lanes::procedures_recall_lane;
use crate::{
    recall_lanes::{KnowledgeTargetResolver, ResolvedKnowledgeTarget},
    storage_composition::CompositionRead,
};
use async_trait::async_trait;
use engram_domain::{
    ChunkId, KnowledgeChunk, KnowledgeEntity, KnowledgeRelationship, RetrievalRequest,
    RetrievalTargetType, Scope,
};
use engram_knowledge::KnowledgeRepository;
use engram_retrieval::{CodeEligibilityResolver, GraphCache, RetrievalIndex};
use engram_runtime::CoreResult;
use engram_store_associative_graph::{AssociativeGraphIndex, GraphRelationshipSource};
use engram_store_community_summary::CommunitySummaryIndex;
use engram_store_lexical::{
    LexicalIndex, LexicalResolvedTarget, LexicalRetrievalIndex, LexicalTargetResolver,
};
use engram_store_sqlite::{
    SqlKnowledgeStore, VectorResolvedTarget, VectorSearchResult, VectorTargetResolver,
};
use std::sync::Arc;

/// Orphan-rule wrapper adapting `SqlKnowledgeStore` to the associative-graph
/// edge source (mirrors `bindings/node/src/knowledge_fusion.rs`). A bare
/// `impl GraphRelationshipSource for SqlKnowledgeStore` is forbidden in this
/// crate (neither the trait nor the store type is local); this newtype is the
/// local type the impl hangs on.
pub(crate) struct KnowledgeRelationshipSource(pub(crate) Arc<dyn CompositionRead>);

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

/// Compatibility wrapper translating canonical target fields to the lexical adapter.
pub(crate) struct KnowledgeLexicalResolver(KnowledgeTargetResolver);
impl KnowledgeLexicalResolver {
    pub(crate) fn new(store: Arc<SqlKnowledgeStore>) -> Self {
        Self(KnowledgeTargetResolver::new(store))
    }
    pub(crate) fn deny_unbound_symbols(self) -> Self {
        Self(self.0.deny_unbound_symbols())
    }
}
#[async_trait]
impl LexicalTargetResolver for KnowledgeLexicalResolver {
    async fn resolve(
        &self,
        target_id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<LexicalResolvedTarget>> {
        Ok(self
            .0
            .resolve(target_id, request)
            .await?
            .map(lexical_target))
    }
    async fn eligible_target_ids(
        &self,
        request: &RetrievalRequest,
        view: &engram_retrieval::BoundCodeView,
    ) -> CoreResult<std::collections::BTreeSet<String>> {
        self.0.eligible_target_ids(request, view).await
    }
}
fn lexical_target(target: ResolvedKnowledgeTarget) -> LexicalResolvedTarget {
    LexicalResolvedTarget {
        target_type: target.target_type,
        target_id: target.target_id,
        content: target.content,
        provenance: target.provenance,
        policy: target.policy,
        explanation: target.explanation,
        metadata: target.metadata,
        code_kind: target.code_kind,
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
