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
use engram_runtime::{CoreError, CoreResult};
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
/// and maintenance mutations (task_016 + corrective r2 findings 3/4,
/// engine-neutral).
///
/// One seam wraps one [`KnowledgeQuery`] and the binding COORDINATOR — so
/// every read/maintenance call authenticates the caller through the shared
/// `CodeBindingAuthorizer` (a denying authorizer denies even for fully
/// populated foreign scopes) and classifies targets by DURABLE LINEAGE +
/// record kind (metadata is never authority):
/// - **Explicit code ids** must resolve to lineage of a PUBLISHED generation
///   in the request scope; a foreign, staged or unknown code id denies the
///   whole request (fail closed — never a silent partial result).
/// - **Explicit ordinary non-code ids** stay readable: an authorized concept
///   or document entity does not need code lineage (finding 4).
/// - **Wildcard / empty request** resolves to exactly the published set —
///   never staged, foreign or unbound repositories, and unstamped legacy
///   CODE kinds are excluded from wildcard listings.
/// - **Maintenance** requires explicit authorized CODE ids; there is no
///   scope-wide maintenance shortcut.
pub struct AuthorizedKnowledgeQuery {
    query: Arc<dyn KnowledgeQuery>,
    coordinator: Arc<crate::code_binding::CodeBindingCoordinator>,
}

impl AuthorizedKnowledgeQuery {
    /// Wraps a knowledge read surface and the binding coordinator (the
    /// coordinator owns both the authorizer and the durable store).
    pub fn new(
        query: Arc<dyn KnowledgeQuery>,
        coordinator: Arc<crate::code_binding::CodeBindingCoordinator>,
    ) -> Self {
        Self { query, coordinator }
    }

    /// Authorizes the caller for every repository in the scope's published
    /// set (finding 3): a denying authorizer denies even for populated
    /// foreign scopes.
    async fn authorize_caller(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
    ) -> CoreResult<()> {
        let view = self.coordinator.eligibility_view(scope, None).await?;
        for binding in view.eligible_bindings() {
            self.coordinator
                .authorize(
                    actor,
                    scope,
                    &engram_domain::RepoId(binding.repo_id.clone()),
                )
                .await?;
        }
        Ok(())
    }

    /// Authorizes an explicit-id (or wildcard, when `requested` is empty)
    /// read and returns the constrained target set: published code lineage
    /// targets only for code ids; explicitly requested ordinary non-code
    /// records pass through as authorized non-code (finding 4).
    #[allow(clippy::double_must_use)]
    pub async fn authorize_read(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
        limit: usize,
    ) -> CoreResult<Vec<String>> {
        self.authorize_caller(actor, scope).await?;
        let view = self.coordinator.eligibility_view(scope, None).await?;
        if requested.is_empty() {
            // Wildcard: exactly the eligible code targets (durable lineage of
            // repo-selected published bindings), bounded by the budget. It
            // can never widen to staged, foreign or unbound repositories.
            return Ok(eligible_lineage_records(&view, limit));
        }
        // Explicit ids: classify each. Code ids (durable lineage) must be
        // eligible; ordinary non-code records are authorized non-code reads;
        // unknown ids deny the whole request.
        let mut authorized = Vec::with_capacity(requested.len());
        for id in requested {
            if view.is_code_record(id) {
                if !view.allows(id, true) {
                    return Err(foreign_target(&[id.clone()]));
                }
                authorized.push(id.clone());
            } else {
                // Ordinary non-code record? It must actually exist as a
                // non-code record in the read surface — otherwise it is an
                // unknown id and denies.
                if await_non_code_record(&self.query, scope, id).await? {
                    authorized.push(id.clone());
                } else {
                    return Err(foreign_target(&[id.clone()]));
                }
            }
        }
        Ok(authorized)
    }

    /// Authorizes a maintenance mutation over explicit target ids — the same
    /// authorization view, never a wider scope. Maintenance targets must be
    /// ELIGIBLE CODE ids (durable lineage of published bindings).
    #[allow(clippy::double_must_use)]
    pub async fn authorize_maintenance(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
    ) -> CoreResult<Vec<String>> {
        if requested.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "maintenance authorization requires explicit target ids".into(),
            });
        }
        self.authorize_caller(actor, scope).await?;
        let view = self.coordinator.eligibility_view(scope, None).await?;
        let mut authorized = Vec::with_capacity(requested.len());
        for id in requested {
            if !view.is_code_record(id) || !view.allows(id, true) {
                return Err(foreign_target(&[id.clone()]));
            }
            authorized.push(id.clone());
        }
        Ok(authorized)
    }

    /// Entities visible to `scope`, with code-derived entities constrained to
    /// the authorized target set: non-code entities always flow (AC-007),
    /// and unstamped legacy CODE kinds never appear in wildcard listings
    /// (finding 4).
    #[allow(clippy::double_must_use)]
    pub async fn list_entities_authorized(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let entities = self.query.list_entities(scope).await?;
        let view = self.coordinator.eligibility_view(scope, None).await?;
        self.authorize_caller(actor, scope).await?;
        if requested.is_empty() {
            return Ok(entities
                .into_iter()
                .filter(|entity| view.allows(entity.id.as_str(), entity_is_code_kind(entity)))
                .collect());
        }
        let authorized: std::collections::HashSet<String> = self
            .authorize_read(actor, scope, requested, requested.len())
            .await?
            .into_iter()
            .collect();
        Ok(entities
            .into_iter()
            .filter(|entity| {
                // Explicit listing: authorized ids (code or ordinary) pass;
                // everything else drops (a narrowed explicit read).
                authorized.contains(&entity.id.to_string())
            })
            .collect())
    }

    /// The wrapped read surface (for unconstrained non-code reads).
    pub fn query(&self) -> &Arc<dyn KnowledgeQuery> {
        &self.query
    }
}

/// Eligible code target ids under a view (durable lineage of repo-selected
/// published bindings), sorted and bounded by the request budget.
fn eligible_lineage_records(view: &engram_retrieval::BoundCodeView, limit: usize) -> Vec<String> {
    let mut eligible: Vec<String> = view
        .code_records()
        .into_iter()
        .filter(|record| view.allows(record, true))
        .map(str::to_owned)
        .collect();
    eligible.sort();
    eligible.truncate(limit);
    eligible
}

/// True when `id` names an existing NON-CODE record in the read surface
/// (an authorized ordinary read — finding 4). Code-kind records without
/// lineage are legacy code and are NOT ordinary reads.
async fn await_non_code_record(
    query: &Arc<dyn KnowledgeQuery>,
    scope: &Scope,
    id: &str,
) -> CoreResult<bool> {
    for entity in query.list_entities(scope).await? {
        if entity.id.as_str() == id {
            return Ok(!entity_is_code_kind(&entity));
        }
    }
    Ok(false)
}

/// True for symbol / structural code-artifact entity kinds.
fn entity_is_code_kind(entity: &KnowledgeEntity) -> bool {
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

/// Typed denial for foreign/unpublished/unknown code targets.
fn foreign_target(ids: &[String]) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!(
            "BindingRequired: code target ids are foreign or unpublished in this scope: {}",
            ids.join(", ")
        ),
    }
}
