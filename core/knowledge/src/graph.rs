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
    /// query fails loudly rather than silently reconciling nothing.
    /// `SqlKnowledgeStore` overrides this — no behavior change on the real path.
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
}
