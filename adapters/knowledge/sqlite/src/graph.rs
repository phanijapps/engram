//! Knowledge graph repository implementation for SQLite adapter.
//!
//! Handles graph operations including storage, retrieval, deletion, and
//! graph traversal (neighbors). This module focuses on graph-specific operations
//! while leaving CRUD knowledge operations to the knowledge module.

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::KnowledgeGraphRepository;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::OptionalExtension;

use crate::{
    schema::json_error, schema::sql_error, scope::scope_allows, service::SqlKnowledgeStore,
};

#[async_trait]
impl KnowledgeGraphRepository for SqlKnowledgeStore {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        let json = serde_json::to_string(&graph).map_err(crate::schema::json_error)?;
        // Lift stable_source_key and path from the graph's metadata into indexed
        // columns so they can be filtered without deserializing record_json.
        //
        // CROSS-CRATE CONTRACT: the literal keys "stableSourceKey" and "path" are
        // the canonical metadata keys defined in `engram-ingest` as
        // `STABLE_SOURCE_KEY` / `SOURCE_PATH_KEY`. This crate intentionally does
        // NOT depend on `engram-ingest`, so the literals must match those constants
        // exactly. The `list_graphs_by_source` integration test in
        // `adapters/ingest/tests/repo_identity.rs` will fail with an empty result
        // if they drift.  If the keys ever change, update both sites together.
        let lifted_key = graph
            .metadata
            .as_ref()
            .and_then(|m| m.get("stableSourceKey")) // must match engram-ingest STABLE_SOURCE_KEY
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let lifted_path = graph
            .metadata
            .as_ref()
            .and_then(|m| m.get("path")) // must match engram-ingest SOURCE_PATH_KEY
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_graphs
                    (id, tenant, subject, workspace, session, environment,
                     stable_source_key, path, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    stable_source_key = excluded.stable_source_key,
                    path = excluded.path,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![
                    graph.id.to_string(),
                    graph.scope.tenant,
                    graph.scope.subject,
                    graph.scope.workspace,
                    graph.scope.session,
                    graph.scope.environment,
                    lifted_key,
                    lifted_path,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(graph)
    }

    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        let connection = self.lock()?;
        let graph = connection
            .query_row(
                "SELECT record_json FROM knowledge_graphs WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeGraph>(&json).map_err(json_error))
            .transpose()?;
        Ok(graph.filter(|graph| scope_allows(&graph.scope, scope)))
    }

    async fn delete_graph(&self, id: &KnowledgeGraphId, scope: &Scope) -> CoreResult<bool> {
        let mut connection = self.lock()?;
        // Read the graph first to scope-check before cascading.
        let graph = connection
            .query_row(
                "SELECT record_json FROM knowledge_graphs WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeGraph>(&json).map_err(json_error))
            .transpose()?;
        let Some(graph) = graph else {
            return Ok(false);
        };
        if !scope_allows(&graph.scope, scope) {
            return Ok(false);
        }
        // Cascade in a single transaction: entities → relationships → graph row.
        // Members are matched by `graph_id` alone, without re-checking scope,
        // because the extractor writes every entity and relationship with the
        // same scope as the graph at ingest time.  A member carrying a
        // different scope would be cascade-deleted here without a scope guard —
        // flag this if the ingestion contract ever allows heterogeneous member
        // scopes within one graph.
        let tx = connection.transaction().map_err(sql_error)?;
        tx.execute(
            "DELETE FROM knowledge_entities WHERE graph_id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(sql_error)?;
        tx.execute(
            "DELETE FROM knowledge_relationships WHERE graph_id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(sql_error)?;
        tx.execute(
            "DELETE FROM knowledge_graphs WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(sql_error)?;
        tx.commit().map_err(sql_error)?;
        Ok(true)
    }

    async fn list_graphs_by_source(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeGraph>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_graphs \
                 WHERE stable_source_key = ?1 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(rusqlite::params![stable_source_key], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        let mut graphs = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let graph = serde_json::from_str::<KnowledgeGraph>(&json).map_err(json_error)?;
            if scope_allows(&graph.scope, scope) {
                graphs.push(graph);
            }
        }
        Ok(graphs)
    }

    async fn neighbors(
        &self,
        graph_id: &KnowledgeGraphId,
        node_id: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let graph = connection
            .query_row(
                "SELECT record_json FROM knowledge_graphs WHERE id = ?1",
                rusqlite::params![graph_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeGraph>(&json).map_err(json_error))
            .transpose()?;
        let Some(graph) = graph else {
            return Err(CoreError::NotFound {
                target_type: "knowledge_graph",
                target_id: graph_id.to_string(),
            });
        };
        if !scope_allows(&graph.scope, scope) {
            return Ok(Vec::new());
        }

        let mut statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_relationships \
                 WHERE graph_id = ?1 AND subject_id = ?2",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(
                rusqlite::params![graph_id.to_string(), node_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let mut relationships = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let relationship =
                serde_json::from_str::<KnowledgeRelationship>(&json).map_err(json_error)?;
            if scope_allows(&relationship.scope, scope) {
                relationships.push(relationship);
            }
        }
        relationships.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
        if let Some(limit) = limit {
            relationships.truncate(limit as usize);
        }
        Ok(relationships)
    }
}
