//! SQLite-backed knowledge, graph, and taxonomy repository.
//!
//! Storage-only: this module persists contract payloads as JSON with scope and
//! lookup indexing. Knowledge orchestration (ingestion, extraction, retrieval
//! fusion) lives elsewhere. Scope visibility mirrors the in-memory knowledge
//! adapter — records that carry scope are filtered directly; chunks, documents,
//! concepts, and relations inherit visibility from their owning source or scheme.

use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    schema::{initialize_schema, json_error, sql_error},
    scope::scope_allows,
};

/// SQLite-backed knowledge, graph, and taxonomy repository.
///
/// Preserves knowledge sources/documents/chunks, entities/relationships/graphs,
/// and concept schemes/concepts/relations as contract JSON while indexing
/// identifiers and scope columns for repository reads.
#[derive(Clone)]
pub struct SqlKnowledgeStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqlKnowledgeStore {
    /// Opens an in-memory SQLite knowledge store and initializes its schema.
    pub fn open_in_memory() -> CoreResult<Self> {
        let connection = Connection::open_in_memory().map_err(sql_error)?;
        Self::from_connection(connection)
    }

    /// Opens a file-backed SQLite knowledge store and initializes its schema.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        let connection = Connection::open(path).map_err(sql_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-knowledge-sqlite".to_owned(),
            message: "connection lock poisoned".to_owned(),
        })
    }

    /// Lists knowledge graphs visible to `scope` (store-specific; not on a port).
    /// Used by the whole-graph explorer to enumerate ingested sources/repos.
    pub async fn list_graphs(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_graphs ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
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

    /// Lists knowledge entities visible to `scope`. Each entity carries its
    /// `graph_id` so the explorer can cluster by source/repo.
    pub async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_entities ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut entities = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let entity = serde_json::from_str::<KnowledgeEntity>(&json).map_err(json_error)?;
            if scope_allows(&entity.scope, scope) {
                entities.push(entity);
            }
        }
        Ok(entities)
    }

    /// Lists knowledge chunks visible to `scope`. Chunks carry the actual
    /// document/code text so Q&A can explain what code does (not just its
    /// call-graph edges).
    pub async fn list_chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_chunks ORDER BY document_id, id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut chunks = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let chunk = serde_json::from_str::<KnowledgeChunk>(&json).map_err(json_error)?;
            // Chunks inherit visibility from their source.
            let source = source_for_chunk(&connection, &chunk)?;
            if source
                .map(|s| scope_allows(&s.scope, scope))
                .unwrap_or(false)
            {
                chunks.push(chunk);
            }
        }
        Ok(chunks)
    }

    /// Lists knowledge sources (repos) visible to `scope`. One record per scan.
    /// Much cheaper than loading all entities to compute per-repo stats.
    pub async fn list_sources(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeSource>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_sources ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut sources = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let source = serde_json::from_str::<KnowledgeSource>(&json).map_err(json_error)?;
            if scope_allows(&source.scope, scope) {
                sources.push(source);
            }
        }
        Ok(sources)
    }

    /// Lists `KnowledgeEntity` records belonging to a specific repository (via
    /// `graph_id IN (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?)`),
    /// visible to `scope`. The per-source `EntityKind::Repository` node has
    /// `graph_id = None` and is NOT included in this result set; reach it via its
    /// `belongs_to` edges returned by `list_relationships_by_source`.
    pub async fn list_entities_by_source(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_entities \
                 WHERE graph_id IN \
                     (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?1) \
                 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![stable_source_key], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut entities = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let entity = serde_json::from_str::<KnowledgeEntity>(&json).map_err(json_error)?;
            if scope_allows(&entity.scope, scope) {
                entities.push(entity);
            }
        }
        Ok(entities)
    }

    /// Lists `KnowledgeRelationship` records belonging to a specific repository
    /// (via `graph_id IN (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?)`),
    /// visible to `scope`. This includes the `belongs_to` edges that link document
    /// graphs to the per-source `EntityKind::Repository` node (those edges carry
    /// the document graph's `graph_id`).
    pub async fn list_relationships_by_source(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_relationships \
                 WHERE graph_id IN \
                     (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?1) \
                 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![stable_source_key], |row| row.get::<_, String>(0))
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
        Ok(relationships)
    }

    /// Lists knowledge relationships visible to `scope`.
    pub async fn list_relationships(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_relationships ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
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
        Ok(relationships)
    }
}

#[async_trait]
impl KnowledgeRepository for SqlKnowledgeStore {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        let json = serde_json::to_string(&source).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_sources
                    (id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    source.id.to_string(),
                    source.scope.tenant,
                    source.scope.subject,
                    source.scope.workspace,
                    source.scope.session,
                    source.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(source)
    }

    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        let json = serde_json::to_string(&document).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_documents (id, source_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    source_id = excluded.source_id,
                    record_json = excluded.record_json
                "#,
                params![
                    document.id.to_string(),
                    document.source_id.to_string(),
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(document)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let json = serde_json::to_string(&chunk).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_chunks (id, document_id, source_id, record_json)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(id) DO UPDATE SET
                    document_id = excluded.document_id,
                    source_id = excluded.source_id,
                    record_json = excluded.record_json
                "#,
                params![
                    chunk.id.to_string(),
                    chunk.document_id.to_string(),
                    chunk.source_id.to_string(),
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(chunk)
    }

    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        let connection = self.lock()?;
        let chunk = connection
            .query_row(
                "SELECT record_json FROM knowledge_chunks WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeChunk>(&json).map_err(json_error))
            .transpose()?;
        let Some(chunk) = chunk else {
            return Ok(None);
        };
        // Chunks inherit visibility from their source.
        let source = source_for_chunk(&connection, &chunk)?;
        Ok(source
            .filter(|source| scope_allows(&source.scope, scope))
            .map(|_| chunk))
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        let json = serde_json::to_string(&entity).map_err(json_error)?;
        // Lift graph_id into an indexed column for the graph-id join queries
        // (list_entities_by_source). The Repository entity has graph_id = None.
        let lifted_graph_id = entity.graph_id.as_ref().map(|id| id.to_string());
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_entities
                    (id, tenant, subject, workspace, session, environment,
                     graph_id, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    graph_id = excluded.graph_id,
                    record_json = excluded.record_json
                "#,
                params![
                    entity.id.to_string(),
                    entity.scope.tenant,
                    entity.scope.subject,
                    entity.scope.workspace,
                    entity.scope.session,
                    entity.scope.environment,
                    lifted_graph_id,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(entity)
    }

    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        let json = serde_json::to_string(&relationship).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_relationships
                    (id, graph_id, subject_id, tenant, subject, workspace, session,
                     environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(id) DO UPDATE SET
                    graph_id = excluded.graph_id,
                    subject_id = excluded.subject_id,
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    relationship.id.to_string(),
                    relationship.graph_id.as_ref().map(ToString::to_string),
                    relationship.subject.id.as_ref().map(ToString::to_string),
                    relationship.scope.tenant,
                    relationship.scope.subject,
                    relationship.scope.workspace,
                    relationship.scope.session,
                    relationship.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(relationship)
    }

    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        let connection = self.lock()?;
        let entity = connection
            .query_row(
                "SELECT record_json FROM knowledge_entities WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeEntity>(&json).map_err(json_error))
            .transpose()?;
        Ok(entity.filter(|entity| scope_allows(&entity.scope, scope)))
    }

    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let relationship = connection
            .query_row(
                "SELECT record_json FROM knowledge_relationships WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeRelationship>(&json).map_err(json_error))
            .transpose()?;
        Ok(relationship.filter(|relationship| scope_allows(&relationship.scope, scope)))
    }

    async fn delete_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        // Read the row first to scope-check before deleting.
        let entity = connection
            .query_row(
                "SELECT record_json FROM knowledge_entities WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeEntity>(&json).map_err(json_error))
            .transpose()?;
        let Some(entity) = entity else {
            return Ok(false);
        };
        if !scope_allows(&entity.scope, scope) {
            return Ok(false);
        }
        let deleted = connection
            .execute(
                "DELETE FROM knowledge_entities WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(deleted > 0)
    }

    async fn delete_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        // Read the row first to scope-check before deleting.
        let relationship = connection
            .query_row(
                "SELECT record_json FROM knowledge_relationships WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<KnowledgeRelationship>(&json).map_err(json_error))
            .transpose()?;
        let Some(relationship) = relationship else {
            return Ok(false);
        };
        if !scope_allows(&relationship.scope, scope) {
            return Ok(false);
        }
        let deleted = connection
            .execute(
                "DELETE FROM knowledge_relationships WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(deleted > 0)
    }
}

#[async_trait]
impl KnowledgeGraphRepository for SqlKnowledgeStore {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        let json = serde_json::to_string(&graph).map_err(json_error)?;
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
                params![
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
                params![id.to_string()],
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
                params![id.to_string()],
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
            params![id.to_string()],
        )
        .map_err(sql_error)?;
        tx.execute(
            "DELETE FROM knowledge_relationships WHERE graph_id = ?1",
            params![id.to_string()],
        )
        .map_err(sql_error)?;
        tx.execute(
            "DELETE FROM knowledge_graphs WHERE id = ?1",
            params![id.to_string()],
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
            .query_map(params![stable_source_key], |row| row.get::<_, String>(0))
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
                params![graph_id.to_string()],
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
            .query_map(params![graph_id.to_string(), node_id.to_string()], |row| {
                row.get::<_, String>(0)
            })
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

#[async_trait]
impl TaxonomyRepository for SqlKnowledgeStore {
    async fn put_concept_scheme(&self, scheme: ConceptScheme) -> CoreResult<ConceptScheme> {
        let json = serde_json::to_string(&scheme).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO concept_schemes
                    (id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    scheme.id.to_string(),
                    scheme.scope.tenant,
                    scheme.scope.subject,
                    scheme.scope.workspace,
                    scheme.scope.session,
                    scheme.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(scheme)
    }

    async fn get_concept_scheme(
        &self,
        id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Option<ConceptScheme>> {
        let connection = self.lock()?;
        let scheme = connection
            .query_row(
                "SELECT record_json FROM concept_schemes WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<ConceptScheme>(&json).map_err(json_error))
            .transpose()?;
        Ok(scheme.filter(|scheme| scope_allows(&scheme.scope, scope)))
    }

    async fn put_concept(&self, concept: Concept) -> CoreResult<Concept> {
        let json = serde_json::to_string(&concept).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO concepts (id, scheme_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    scheme_id = excluded.scheme_id,
                    record_json = excluded.record_json
                "#,
                params![concept.id.to_string(), concept.scheme_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(concept)
    }

    async fn put_concept_relation(&self, relation: ConceptRelation) -> CoreResult<ConceptRelation> {
        let json = serde_json::to_string(&relation).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO concept_relations (id, scheme_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    scheme_id = excluded.scheme_id,
                    record_json = excluded.record_json
                "#,
                params![relation.id, relation.scheme_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(relation)
    }

    async fn list_concepts(
        &self,
        scheme_id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Vec<Concept>> {
        let connection = self.lock()?;
        let scheme = connection
            .query_row(
                "SELECT record_json FROM concept_schemes WHERE id = ?1",
                params![scheme_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<ConceptScheme>(&json).map_err(json_error))
            .transpose()?;
        let Some(scheme) = scheme else {
            return Ok(Vec::new());
        };
        if !scope_allows(&scheme.scope, scope) {
            return Ok(Vec::new());
        }

        let mut statement = connection
            .prepare("SELECT record_json FROM concepts WHERE scheme_id = ?1")
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![scheme_id.to_string()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        let mut concepts = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            concepts.push(serde_json::from_str::<Concept>(&json).map_err(json_error)?);
        }
        concepts.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
        Ok(concepts)
    }
}

#[async_trait]
impl OntologyRepository for SqlKnowledgeStore {
    async fn put_ontology(&self, ontology: Ontology) -> CoreResult<Ontology> {
        let json = serde_json::to_string(&ontology).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontologies
                    (id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    ontology.id.to_string(),
                    ontology.scope.tenant,
                    ontology.scope.subject,
                    ontology.scope.workspace,
                    ontology.scope.session,
                    ontology.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(ontology)
    }

    async fn get_ontology(&self, id: &OntologyId, scope: &Scope) -> CoreResult<Option<Ontology>> {
        let connection = self.lock()?;
        let ontology = connection
            .query_row(
                "SELECT record_json FROM ontologies WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Ontology>(&json).map_err(json_error))
            .transpose()?;
        Ok(ontology.filter(|ontology| scope_allows(&ontology.scope, scope)))
    }

    // Classes, properties, and axioms carry no scope of their own — they inherit
    // visibility from their owning ontology (mirroring concepts ↔ concept
    // scheme). `put_*` does not re-verify the caller owns `ontology_id`; reads
    // (`get_ontology`, `validate_graph`) enforce scope on the parent ontology.
    async fn put_class(&self, class: OntologyClass) -> CoreResult<OntologyClass> {
        let json = serde_json::to_string(&class).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontology_classes (id, ontology_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    ontology_id = excluded.ontology_id,
                    record_json = excluded.record_json
                "#,
                params![class.id.to_string(), class.ontology_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(class)
    }

    async fn put_property(&self, property: OntologyProperty) -> CoreResult<OntologyProperty> {
        let json = serde_json::to_string(&property).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontology_properties (id, ontology_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    ontology_id = excluded.ontology_id,
                    record_json = excluded.record_json
                "#,
                params![
                    property.id.to_string(),
                    property.ontology_id.to_string(),
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(property)
    }

    async fn put_axiom(&self, axiom: OntologyAxiom) -> CoreResult<OntologyAxiom> {
        let json = serde_json::to_string(&axiom).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontology_axioms (id, ontology_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    ontology_id = excluded.ontology_id,
                    record_json = excluded.record_json
                "#,
                params![axiom.id.to_string(), axiom.ontology_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(axiom)
    }

    /// Advisory validation: warns on relationships whose predicate is not declared
    /// as an ontology property (by label or URI). It never rejects writes — the
    /// port is advisory by contract. A missing or scope-hidden ontology, or a
    /// scope-hidden relationship, contributes no findings.
    async fn validate_graph(
        &self,
        graph_id: &KnowledgeGraphId,
        ontology_id: &OntologyId,
        scope: &Scope,
    ) -> CoreResult<Vec<OntologyValidationFinding>> {
        let connection = self.lock()?;
        let ontology = connection
            .query_row(
                "SELECT record_json FROM ontologies WHERE id = ?1",
                params![ontology_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Ontology>(&json).map_err(json_error))
            .transpose()?;
        let Some(ontology) = ontology else {
            return Ok(Vec::new());
        };
        if !scope_allows(&ontology.scope, scope) {
            return Ok(Vec::new());
        }

        // Declared vocabulary = property labels + URIs (lowercased).
        let mut declared: HashSet<String> = HashSet::new();
        let mut property_statement = connection
            .prepare("SELECT record_json FROM ontology_properties WHERE ontology_id = ?1")
            .map_err(sql_error)?;
        let property_rows = property_statement
            .query_map(params![ontology_id.to_string()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        for row in property_rows {
            let json = row.map_err(sql_error)?;
            let property = serde_json::from_str::<OntologyProperty>(&json).map_err(json_error)?;
            declared.insert(property.label.to_lowercase());
            declared.insert(property.uri.to_lowercase());
        }
        drop(property_statement);

        let now = Utc::now();
        // Bound the scan so an oversized graph cannot make advisory validation
        // unbounded: read LIMIT+1 rows, and if that many come back, report
        // truncation instead of scanning further. `drop` the statement before
        // reusing the connection (rusqlite borrow lifetime).
        let mut relationship_statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_relationships \
                 WHERE graph_id = ?1 ORDER BY id LIMIT ?2",
            )
            .map_err(sql_error)?;
        let relationship_rows: Vec<String> = relationship_statement
            .query_map(
                params![
                    graph_id.to_string(),
                    (VALIDATE_RELATIONSHIP_LIMIT + 1) as i64
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?
            .collect::<Result<_, _>>()
            .map_err(sql_error)?;
        drop(relationship_statement);
        let truncated = relationship_rows.len() > VALIDATE_RELATIONSHIP_LIMIT;

        let mut findings = Vec::new();
        for json in relationship_rows
            .into_iter()
            .take(VALIDATE_RELATIONSHIP_LIMIT)
        {
            let relationship =
                serde_json::from_str::<KnowledgeRelationship>(&json).map_err(json_error)?;
            if !scope_allows(&relationship.scope, scope) {
                continue;
            }
            if declared.contains(&relationship.predicate.to_lowercase()) {
                continue;
            }
            findings.push(OntologyValidationFinding {
                id: format!("finding-{ontology_id}-{}", relationship.id),
                ontology_id: ontology_id.clone(),
                severity: OntologyValidationSeverity::Warning,
                code: "undeclared_predicate".to_owned(),
                message: format!(
                    "relationship predicate `{}` is not declared by ontology `{ontology_id}`",
                    relationship.predicate
                ),
                target: Some(relationship.subject.clone()),
                axiom_id: None,
                provenance: validation_provenance(ontology_id, now),
                detected_at: now,
            });
        }
        if truncated {
            findings.push(OntologyValidationFinding {
                id: format!("finding-{ontology_id}-truncated"),
                ontology_id: ontology_id.clone(),
                severity: OntologyValidationSeverity::Info,
                code: "validation_truncated".to_owned(),
                message: format!(
                    "graph has more than {VALIDATE_RELATIONSHIP_LIMIT} relationships; validation truncated"
                ),
                target: None,
                axiom_id: None,
                provenance: validation_provenance(ontology_id, now),
                detected_at: now,
            });
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(findings)
    }
}

/// Caps the number of relationships `validate_graph` scans, so advisory
/// validation over an oversized graph stays bounded.
const VALIDATE_RELATIONSHIP_LIMIT: usize = 5_000;

/// Builds the advisory provenance stamped on every validation finding. Validation
/// is deterministic and carries no input evidence, so a fixed system actor is used.
fn validation_provenance(ontology_id: &OntologyId, now: chrono::DateTime<Utc>) -> Provenance {
    Provenance {
        source: format!("ontology:{ontology_id}"),
        actor: Actor {
            id: Id::from("engram-ontology-validator"),
            kind: ActorKind::System,
            display_name: Some("Ontology validator".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("ontology_validation".to_owned()),
    }
}

/// Loads the `KnowledgeSource` that owns a chunk (chunk -> document -> source).
fn source_for_chunk(
    connection: &Connection,
    chunk: &KnowledgeChunk,
) -> CoreResult<Option<KnowledgeSource>> {
    let document = connection
        .query_row(
            "SELECT record_json FROM knowledge_documents WHERE id = ?1",
            params![chunk.document_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|json| serde_json::from_str::<SourceDocument>(&json).map_err(json_error))
        .transpose()?;
    let Some(document) = document else {
        return Ok(None);
    };
    connection
        .query_row(
            "SELECT record_json FROM knowledge_sources WHERE id = ?1",
            params![document.source_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|json| serde_json::from_str::<KnowledgeSource>(&json).map_err(json_error))
        .transpose()
}
