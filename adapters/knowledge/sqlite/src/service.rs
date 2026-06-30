//! SQLite-backed knowledge, graph, and taxonomy repository.
//!
//! Storage-only: this module persists contract payloads as JSON with scope and
//! lookup indexing. Knowledge orchestration (ingestion, extraction, retrieval
//! fusion) lives elsewhere. Scope visibility mirrors the in-memory knowledge
//! adapter — records that carry scope are filtered directly; chunks, documents,
//! concepts, and relations inherit visibility from their owning source or scheme.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository, TaxonomyRepository};
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
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_entities
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
                    entity.id.to_string(),
                    entity.scope.tenant,
                    entity.scope.subject,
                    entity.scope.workspace,
                    entity.scope.session,
                    entity.scope.environment,
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
}

#[async_trait]
impl KnowledgeGraphRepository for SqlKnowledgeStore {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        let json = serde_json::to_string(&graph).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_graphs
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
                    graph.id.to_string(),
                    graph.scope.tenant,
                    graph.scope.subject,
                    graph.scope.workspace,
                    graph.scope.session,
                    graph.scope.environment,
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
