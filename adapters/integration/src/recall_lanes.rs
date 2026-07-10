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
//! resolvers live in the adapters layer, not the engine-neutral port crate.
//!
//! # Sync resolvers + async store
//!
//! Both resolver traits are synchronous (the adapter lanes call them inline),
//! while `KnowledgeRepository::get_chunk` is async-by-convention — its body is
//! pure synchronous rusqlite (mutex lock + query + deserialize) with no async
//! I/O. `block_on` therefore polls the future to completion in a single step
//! without yielding, so re-entry from within the unified-recall async path is
//! safe (unlike a tokio runtime, `futures::executor::block_on` does not panic on
//! re-entry, and the polled future never needs the outer executor to progress).

use std::sync::Arc;

use engram_domain::{ChunkId, KnowledgeChunk, RetrievalRequest, RetrievalTargetType, Scope};
use engram_knowledge::KnowledgeRepository;
use engram_runtime::CoreResult;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_lexical::{LexicalResolvedTarget, LexicalTargetResolver};
#[cfg(feature = "fastembed")]
use engram_store_vector::{VectorResolvedTarget, VectorSearchResult, VectorTargetResolver};
use futures::executor::block_on;

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

impl LexicalTargetResolver for KnowledgeLexicalResolver {
    fn resolve(
        &self,
        target_id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<LexicalResolvedTarget>> {
        let chunk = resolve_chunk(&self.store, target_id, &request.scope)?;
        Ok(chunk.map(chunk_to_lexical))
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
impl VectorTargetResolver for KnowledgeVectorResolver {
    fn resolve(
        &self,
        hit: &VectorSearchResult,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<VectorResolvedTarget>> {
        let chunk = resolve_chunk(&self.store, &hit.target_id, &request.scope)?;
        Ok(chunk.map(chunk_to_vector))
    }
}

/// Looks up a chunk by id + scope from the knowledge store.
///
/// `target_id` comes from a secondary index hit (lexical / vector); the store
/// is the canonical source, so a stale or scope-invisible hit returns `None`
/// (the lane skips it) rather than synthesizing a phantom candidate.
fn resolve_chunk(
    store: &Arc<SqlKnowledgeStore>,
    target_id: &str,
    scope: &Scope,
) -> CoreResult<Option<KnowledgeChunk>> {
    let id = ChunkId::from(target_id);
    block_on(store.get_chunk(&id, scope))
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
    //! production `bootstrap_provider` wiring (see `wiring` tests) and the
    //! `SqlUnifiedRecall` integration tests. This module is reserved for any
    //! future resolver-only unit tests that do not require a store.
}
