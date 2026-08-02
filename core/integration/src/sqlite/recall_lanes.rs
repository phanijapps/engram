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
    AllowedUse, ChunkId, DeleteMode, EntityId, KnowledgeChunk, KnowledgeEntity,
    KnowledgeRelationship, Policy, Retention, RetrievalRequest, RetrievalTargetType, Scope,
    Sensitivity, Visibility,
};
use engram_knowledge::KnowledgeRepository;
use engram_retrieval::{GraphCache, RetrievalIndex};
use engram_runtime::CoreResult;
use engram_store_associative_graph::{AssociativeGraphIndex, GraphRelationshipSource};
use engram_store_community_summary::CommunitySummaryIndex;
use engram_store_lexical::{
    LexicalIndex, LexicalResolvedTarget, LexicalRetrievalIndex, LexicalTargetResolver,
};
use engram_store_sqlite::SqlKnowledgeStore;
#[cfg(feature = "fastembed")]
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

/// Lexical-lane target resolver backed by the knowledge store: rehydrates a
/// BM25 hit's chunk id into its canonical `KnowledgeChunk`.
pub(crate) struct KnowledgeLexicalResolver {
    store: Arc<SqlKnowledgeStore>,
}

impl KnowledgeLexicalResolver {
    /// Wraps a shared knowledge-store handle to resolve chunk hits.
    pub(crate) fn new(store: Arc<SqlKnowledgeStore>) -> Self {
        Self { store }
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
        let entity_id = EntityId::from(target_id);
        if let Some(entity) = self.store.get_entity(&entity_id, &request.scope).await? {
            return Ok(Some(entity_to_lexical(entity)));
        }
        Ok(None)
    }
}

/// Vector-lane target resolver backed by the knowledge store: rehydrates a
/// sqlite-vec hit into its canonical `KnowledgeChunk`.
#[cfg(feature = "fastembed")]
pub(crate) struct KnowledgeVectorResolver {
    store: Arc<SqlKnowledgeStore>,
}

#[cfg(feature = "fastembed")]
impl KnowledgeVectorResolver {
    /// Wraps a shared knowledge-store handle to resolve vector hits.
    pub(crate) fn new(store: Arc<SqlKnowledgeStore>) -> Self {
        Self { store }
    }
}

#[cfg(feature = "fastembed")]
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
#[cfg(feature = "fastembed")]
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
    LexicalResolvedTarget {
        target_type: RetrievalTargetType::Chunk,
        target_id: chunk.id.to_string(),
        content: chunk.text,
        provenance: chunk.provenance,
        policy: chunk.policy,
        explanation: None,
        metadata: chunk.metadata,
    }
}

/// Shapes a resolved entity as a lexical-lane retrieval target (code symbols).
/// The content mirrors what `scan_repo` indexed (`"{name} {kind:?}"`) so the
/// lane's hit text stays consistent with the indexed document.
fn entity_to_lexical(entity: KnowledgeEntity) -> LexicalResolvedTarget {
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
    }
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
#[cfg(feature = "fastembed")]
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

#[cfg(test)]
mod tests {
    //! The knowledge-backed resolvers are exercised end-to-end through the
    //! production `bootstrap_sqlite` wiring (see `bootstrap` tests) and the
    //! `SqlUnifiedRecall` integration tests. This module is reserved for any
    //! future resolver-only unit tests that do not require a store.
}
