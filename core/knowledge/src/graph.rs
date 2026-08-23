//! Knowledge graph repository port — graph identity and traversal.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

/// Persistence and traversal port for ontology-backed knowledge graphs.
///
/// This port owns logical graph identity and traversal independent of the
/// physical graph technology. Neo4j labels, RDF triples, SQL joins, or embedded
/// graph indexes are adapter details; callers see scoped graph records and
/// relationship paths with domain provenance.
#[async_trait]
pub trait KnowledgeGraphRepository: Send + Sync {
    /// Stores or updates a graph identity record.
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph>;

    /// Looks up a graph by ID inside the caller-provided scope boundary.
    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>>;

    /// Returns graph neighbors for a node without crossing scope boundaries.
    async fn neighbors(
        &self,
        graph_id: &KnowledgeGraphId,
        node_id: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>>;

    /// Deletes a graph and cascades to every entity and relationship carrying
    /// that `graph_id`, all in a single transaction. Returns `true` if the
    /// graph existed and was deleted. A delete under a non-matching scope is a
    /// no-op returning `false` (hard delete; no tombstone). Default
    /// implementation returns a not-supported error.
    async fn delete_graph(&self, _id: &KnowledgeGraphId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "graph deletes are not supported".to_owned(),
        })
    }

    /// Lists knowledge graphs belonging to `stable_source_key`, visible to
    /// `scope`. Used by the ingest reconciler to find prior graphs for a
    /// `(stable_source_key, path)` pair before writing a replacement.
    ///
    /// Default implementation returns a not-supported error so that a future
    /// adapter that overrides the delete methods but forgets to override this
    /// query fails loudly rather than silently reconciling nothing. The active
    /// knowledge adapter overrides this on its real path (no behavior change).
    async fn list_graphs_by_source(
        &self,
        _scope: &Scope,
        _stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeGraph>> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "list_graphs_by_source is not supported".to_owned(),
        })
    }

    // ── Unresolved-reference ledger (RFC-0020 Phase 2) ─────────────────────
    //
    // The honesty ledger for cross-file code resolution: pending references
    // persist with the same lifecycle as relationships (retracted with their
    // graph; re-attempted by the ingest orphan sweep).

    /// Stores ledger rows (upsert by id; deterministic ids regenerate the
    /// same row on re-ingest).
    async fn put_unresolved_refs(&self, _refs: Vec<UnresolvedReference>) -> CoreResult<()> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "put_unresolved_refs is not supported".to_owned(),
        })
    }

    /// Lists ledger rows by status within the caller's scope.
    async fn list_unresolved_refs(
        &self,
        _scope: &Scope,
        _status: UnresolvedReferenceStatus,
    ) -> CoreResult<Vec<UnresolvedReference>> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "list_unresolved_refs is not supported".to_owned(),
        })
    }

    /// Flips a row's status (`pending → resolved` by the sweep;
    /// `pending → failed` only by explicit operator action).
    async fn update_unresolved_status(
        &self,
        _id: &Id,
        _status: UnresolvedReferenceStatus,
        _scope: &Scope,
    ) -> CoreResult<()> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "update_unresolved_status is not supported".to_owned(),
        })
    }

    /// Deletes ledger rows for a graph — part of the retraction cascade
    /// (`delete_graph` calls this alongside entities and relationships).
    async fn delete_unresolved_for_graph(
        &self,
        _graph_id: &KnowledgeGraphId,
        _scope: &Scope,
    ) -> CoreResult<()> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "delete_unresolved_for_graph is not supported".to_owned(),
        })
    }
}
