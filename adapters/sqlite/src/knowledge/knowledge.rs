//! Knowledge repository implementation for SQLite adapter.
//!
//! Handles CRUD operations for knowledge sources, documents, chunks, entities,
//! and relationships. This module focuses on storage-neutral knowledge operations
//! while leaving graph traversal, ontology, and taxonomy operations to their
//! respective modules.

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::KnowledgeRepository;
use engram_runtime::CoreResult;
use rusqlite::OptionalExtension;

use crate::knowledge::{schema::sql_error, service::SqlKnowledgeStore};

#[async_trait]
impl KnowledgeRepository for SqlKnowledgeStore {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        let json = serde_json::to_string(&source).map_err(crate::knowledge::schema::json_error)?;
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
                rusqlite::params![
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
        let json =
            serde_json::to_string(&document).map_err(crate::knowledge::schema::json_error)?;
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
                rusqlite::params![
                    document.id.to_string(),
                    document.source_id.to_string(),
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(document)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let json = serde_json::to_string(&chunk).map_err(crate::knowledge::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_chunks
                    (id, document_id, source_id, record_json)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(id) DO UPDATE SET
                    document_id = excluded.document_id,
                    source_id = excluded.source_id,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![
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
        // Chunks have no scope of their own; they inherit the source's scope.
        // Resolve visibility by joining chunk → source and filtering on the
        // source's scope columns.
        let connection = self.lock()?;
        let json: Option<String> = connection
            .query_row(
                r#"
                SELECT c.record_json
                FROM knowledge_chunks c
                JOIN knowledge_sources s ON c.source_id = s.id
                WHERE c.id = ?1
                    AND s.tenant = ?2
                    AND (s.subject IS NULL OR s.subject = ?3)
                    AND (s.workspace IS NULL OR s.workspace = ?4)
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace,],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        match json {
            Some(json) => {
                let chunk =
                    serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?;
                Ok(Some(chunk))
            }
            None => Ok(None),
        }
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        let json = serde_json::to_string(&entity).map_err(crate::knowledge::schema::json_error)?;
        let connection = self.lock()?;
        let graph_id = entity
            .graph_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        connection
            .execute(
                r#"
                INSERT INTO knowledge_entities
                    (id, graph_id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(id) DO UPDATE SET
                    graph_id = excluded.graph_id,
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![
                    entity.id.to_string(),
                    graph_id,
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
        let json =
            serde_json::to_string(&relationship).map_err(crate::knowledge::schema::json_error)?;
        let connection = self.lock()?;
        let graph_id = relationship
            .graph_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        let subject_id = relationship
            .subject
            .id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        connection
            .execute(
                r#"
                INSERT INTO knowledge_relationships
                    (id, graph_id, subject_id, tenant, subject, workspace, session, environment, record_json)
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
                rusqlite::params![
                    relationship.id.to_string(),
                    graph_id,
                    subject_id,
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
        let json: Option<String> = connection
            .query_row(
                r#"
                SELECT record_json
                FROM knowledge_entities
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace,],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        match json {
            Some(json) => {
                let entity =
                    serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?;
                Ok(Some(entity))
            }
            None => Ok(None),
        }
    }

    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let json: Option<String> = connection
            .query_row(
                r#"
                SELECT record_json
                FROM knowledge_relationships
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace,],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        match json {
            Some(json) => {
                let relationship =
                    serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?;
                Ok(Some(relationship))
            }
            None => Ok(None),
        }
    }

    async fn delete_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        let affected = connection
            .execute(
                r#"
                DELETE FROM knowledge_entities
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace,],
            )
            .map_err(sql_error)?;
        Ok(affected > 0)
    }

    async fn delete_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        let affected = connection
            .execute(
                r#"
                DELETE FROM knowledge_relationships
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace,],
            )
            .map_err(sql_error)?;
        Ok(affected > 0)
    }
}
