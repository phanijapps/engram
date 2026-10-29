//! Engine-neutral query port for listing knowledge-graph records in a scope.
//!
//! [`KnowledgeRepository`](engram_knowledge::KnowledgeRepository) and
//! [`KnowledgeGraphRepository`](engram_knowledge::KnowledgeGraphRepository) are
//! write/lookup ports (put / get / delete / neighbors); they intentionally
//! expose no "list everything in scope" method. Code-intelligence composition —
//! and any caller that needs the full entity/edge set for a project — goes
//! through this port so it can route through the
//! [`EngramProvider`](crate::EngramProvider) instead of reaching into a concrete
//! store (the old `codegraph/mcp-server` reached into the concrete store
//! directly; this port removes that need).

use async_trait::async_trait;
use engram_domain::{
    ChunkId, DocumentId, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph, KnowledgeRelationship,
    Scope,
};
use engram_runtime::CoreResult;
use std::sync::Arc;

/// Read port: list the entities / relationships / chunks visible to a scope.
/// Lean chunk reference for embed/index paths (see
/// [`KnowledgeQuery::list_chunk_refs`]).
#[derive(Debug, Clone)]
pub struct ChunkRef {
    pub id: ChunkId,
    /// The chunk's provenance source string (scan delta filtering).
    pub source: String,
    /// `false` for empty-text chunks — excluded from embedding without
    /// needing the text itself.
    pub has_text: bool,
    /// The chunk's text content hash (scanner-stamped, full-text domain) —
    /// the durable-dedup key: two chunks with identical text share it.
    pub content_hash: String,
}

#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait KnowledgeQuery: Send + Sync {
    /// All entities in `scope`.
    async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>>;

    /// All relationships in `scope`.
    async fn list_relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>>;

    /// All chunks in `scope` (for embedding/indexing). Default: empty (not supported).
    async fn list_chunks(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        Ok(Vec::new())
    }

    /// Lean chunk listing for embed/index paths: id + provenance source +
    /// text-nonemptiness per chunk in scope — no text transfer, no full
    /// record deserialization. The embed pending-set computation needs
    /// exactly this; materializing full chunk records (text included) for
    /// 90k+ chunks cost ~1.4s and hundreds of MB per call. Default: derived
    /// from [`Self::list_chunks`] (correct everywhere, efficient where the
    /// store overrides it).
    async fn list_chunk_refs(&self, scope: &Scope) -> CoreResult<Vec<ChunkRef>> {
        Ok(self
            .list_chunks(scope)
            .await?
            .into_iter()
            .map(|c| ChunkRef {
                id: c.id,
                source: c.provenance.source,
                has_text: !c.text.is_empty(),
                content_hash: c.content_hash,
            })
            .collect())
    }

    /// All graphs in `scope`. Default: empty (not supported).
    async fn list_graphs(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        Ok(Vec::new())
    }

    /// The chunks of one `document_id` within `scope` (bounded per-document read,
    /// used by the extract-knowledge op). Default: empty (not supported).
    async fn list_chunks_by_document(
        &self,
        _document_id: &DocumentId,
        _scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        Ok(Vec::new())
    }
}

/// Shared scoped authorization for explicit-ID code reads, wildcard listing
/// and maintenance mutations (task_016, engine-neutral).
///
/// One seam wraps one [`KnowledgeQuery`] and one cell-backed
/// [`CodeEligibilityGate`] — the same authorization view serves graph
/// traversal, explicit-ID fetches and every maintenance operation, with no
/// second scope hierarchy:
/// - **Explicit ids** must resolve to lineage of a PUBLISHED generation in
///   the request scope; a foreign, staged or unknown id denies the whole
///   request (fail closed — never a silent partial result).
/// - **Wildcard / empty request** resolves to exactly the published set — it
///   can never widen to staged, foreign or unbound repositories.
/// - **Maintenance** requires explicit authorized ids; there is no
///   scope-wide maintenance shortcut.
pub struct AuthorizedKnowledgeQuery {
    query: Arc<dyn KnowledgeQuery>,
    gate: Arc<dyn engram_knowledge::CodeEligibilityGate>,
}

impl AuthorizedKnowledgeQuery {
    /// Wraps a knowledge read surface and the durable eligibility gate.
    pub fn new(
        query: Arc<dyn KnowledgeQuery>,
        gate: Arc<dyn engram_knowledge::CodeEligibilityGate>,
    ) -> Self {
        Self { query, gate }
    }

    /// Authorizes an explicit-id (or wildcard, when `requested` is empty)
    /// code read and returns the constrained target set.
    pub async fn authorize_read(
        &self,
        scope: &Scope,
        requested: &[String],
        limit: usize,
    ) -> CoreResult<Vec<String>> {
        let constrained = self.gate.authorize_query(scope, requested, limit).await?;
        Ok(constrained.target_ids)
    }

    /// Authorizes a maintenance mutation over explicit target ids — the same
    /// authorization view, never a wider scope.
    pub async fn authorize_maintenance(
        &self,
        scope: &Scope,
        requested: &[String],
    ) -> CoreResult<Vec<String>> {
        self.gate.authorize_maintenance(scope, requested).await
    }

    /// Entities visible to `scope`, with code-derived entities constrained to
    /// the authorized target set when one is requested: non-code entities
    /// always flow (AC-007 keeps ordinary knowledge usable).
    pub async fn list_entities_authorized(
        &self,
        scope: &Scope,
        requested: &[String],
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let entities = self.query.list_entities(scope).await?;
        if requested.is_empty() {
            // Wildcard listing: everything EXCEPT code-derived entities not in
            // the published set (constrain, never widen).
            let authorized: std::collections::HashSet<String> = self
                .authorize_read(scope, &[], usize::MAX)
                .await?
                .into_iter()
                .collect();
            return Ok(entities
                .into_iter()
                .filter(|entity| {
                    let stamped = entity
                        .metadata
                        .as_ref()
                        .and_then(|meta| {
                            meta.get(engram_retrieval::bound_filter::CODE_GENERATION_KEY)
                        })
                        .is_some();
                    !stamped || authorized.contains(&entity.id.to_string())
                })
                .collect());
        }
        let authorized: std::collections::HashSet<String> = self
            .authorize_read(scope, requested, requested.len())
            .await?
            .into_iter()
            .collect();
        Ok(entities
            .into_iter()
            .filter(|entity| {
                !is_code_derived(entity) || authorized.contains(&entity.id.to_string())
            })
            .collect())
    }

    /// The wrapped read surface (for unconstrained non-code reads).
    pub fn query(&self) -> &Arc<dyn KnowledgeQuery> {
        &self.query
    }
}

/// True when an entity carries a bound-code generation stamp.
fn is_code_derived(entity: &KnowledgeEntity) -> bool {
    entity
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(engram_retrieval::bound_filter::CODE_GENERATION_KEY))
        .is_some()
}
