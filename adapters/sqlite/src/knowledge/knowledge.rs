//! Knowledge repository implementation for SQLite adapter.
//!
//! Handles CRUD operations for knowledge sources, documents, chunks, entities,
//! and relationships. This module focuses on storage-neutral knowledge operations
//! while leaving graph traversal, ontology, and taxonomy operations to their
//! respective modules.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use engram_domain::*;
use engram_knowledge::KnowledgeRepository;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension};

use crate::knowledge::{schema::sql_error, service::SqlKnowledgeStore};

/// Read the persisted `archived_at` for a record (if any), so a re-put preserves a
/// prior maintenance archive instead of silently un-archiving (ingestion always
/// passes `archived_at = None`). Keeps the column and `record_json` in lockstep.
pub(super) fn persisted_archived_at(
    conn: &Connection,
    table: &str,
    id: &str,
) -> CoreResult<Option<Timestamp>> {
    let sql = format!("SELECT archived_at FROM {table} WHERE id = ?1");
    // The column is nullable: read NULL as None inside the row closure —
    // inferring `String` there throws "Invalid column type Null" on any
    // re-put of an un-archived row (the multi-file/second-scan upsert bug).
    let stored: Option<Option<String>> = conn
        .query_row(&sql, rusqlite::params![id], |row| row.get(0))
        .optional()
        .map_err(sql_error)?;
    stored
        .flatten()
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-knowledge-sqlite".to_owned(),
                    message: format!("invalid archived_at timestamp: {e}"),
                })
        })
        .transpose()
}

#[async_trait]
impl KnowledgeRepository for SqlKnowledgeStore {
    fn code_sources(&self) -> Option<&dyn engram_knowledge::CodeSourceRepository> {
        Some(self)
    }
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::reject_unfenced_write(
            &connection,
            CodeSourceRecordKind::Source,
            source.id.as_str(),
        )?;
        let result = super::record_writes::put_source(&connection, source)?;
        connection.commit().map_err(sql_error)?;
        Ok(result)
    }

    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::reject_unfenced_write(
            &connection,
            CodeSourceRecordKind::Document,
            document.id.as_str(),
        )?;
        super::guarded_source_write::reject_unfenced_write(
            &connection,
            CodeSourceRecordKind::Source,
            document.source_id.as_str(),
        )?;
        let result = super::record_writes::put_document(&connection, document)?;
        connection.commit().map_err(sql_error)?;
        Ok(result)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        if chunk.code.is_some() {
            return Err(CoreError::Conflict {
                reason: "typed code writes require a source attempt".into(),
            });
        }
        super::guarded_source_write::reject_unfenced_write(
            &connection,
            CodeSourceRecordKind::Chunk,
            chunk.id.as_str(),
        )?;
        super::guarded_source_write::reject_unfenced_write(
            &connection,
            CodeSourceRecordKind::Document,
            chunk.document_id.as_str(),
        )?;
        let result = super::record_writes::put_chunk(&connection, chunk)?;
        connection.commit().map_err(sql_error)?;
        Ok(result)
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

    /// Lists a document's chunks within `scope` (scope resolved via the source
    /// join, mirroring `get_chunk`). Used by retraction to find prior chunks.
    async fn list_chunks_by_document(
        &self,
        document_id: &DocumentId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        let connection = self.lock()?;
        let mut stmt = connection
            .prepare(
                r#"
                SELECT c.record_json
                FROM knowledge_chunks c
                JOIN knowledge_sources s ON c.source_id = s.id
                WHERE c.document_id = ?1
                    AND s.tenant = ?2
                    AND (s.subject IS NULL OR s.subject = ?3)
                    AND (s.workspace IS NULL OR s.workspace = ?4)
                "#,
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![
                    document_id.to_string(),
                    scope.tenant,
                    scope.subject,
                    scope.workspace,
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let mut chunks = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            chunks.push(serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?);
        }
        Ok(chunks)
    }

    /// Hard-deletes a document by id. The reconcile path is scope-bounded
    /// (the document id was read from a scope-filtered prior graph), so this
    /// deletes by id directly.
    async fn delete_document(&self, id: &DocumentId, scope: &Scope) -> CoreResult<bool> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::authorize_owned_mutation(
            &connection,
            CodeSourceRecordKind::Document,
            id.as_str(),
            scope,
        )?;
        let n = connection
            .execute(
                "DELETE FROM knowledge_documents WHERE id = ?1",
                rusqlite::params![id.to_string()],
            )
            .map_err(sql_error)?;
        connection.commit().map_err(sql_error)?;
        Ok(n > 0)
    }

    /// Hard-deletes a single chunk by id (scope-bounded by the reconcile caller).
    async fn delete_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<bool> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::authorize_owned_mutation(
            &connection,
            CodeSourceRecordKind::Chunk,
            id.as_str(),
            scope,
        )?;
        let n = connection
            .execute(
                "DELETE FROM knowledge_chunks_records WHERE id = ?1",
                rusqlite::params![id.to_string()],
            )
            .map_err(sql_error)?;
        connection.commit().map_err(sql_error)?;
        Ok(n > 0)
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::reject_unfenced_write(
            &connection,
            CodeSourceRecordKind::Entity,
            entity.id.as_str(),
        )?;
        if let Some(graph) = &entity.graph_id {
            super::guarded_source_write::reject_unfenced_write(
                &connection,
                CodeSourceRecordKind::Graph,
                graph.as_str(),
            )?;
        }
        let result = super::record_writes::put_entity(&connection, entity)?;
        connection.commit().map_err(sql_error)?;
        Ok(result)
    }

    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::reject_unfenced_write(
            &connection,
            CodeSourceRecordKind::Relationship,
            relationship.id.as_str(),
        )?;
        if let Some(graph) = &relationship.graph_id {
            super::guarded_source_write::reject_unfenced_write(
                &connection,
                CodeSourceRecordKind::Graph,
                graph.as_str(),
            )?;
        }
        let result = super::record_writes::put_relationship(&connection, relationship)?;
        connection.commit().map_err(sql_error)?;
        Ok(result)
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
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::authorize_owned_mutation(
            &connection,
            CodeSourceRecordKind::Entity,
            id.as_str(),
            scope,
        )?;
        let affected = connection
            .execute(
                r#"
                DELETE FROM knowledge_entities_records
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace,],
            )
            .map_err(sql_error)?;
        connection.commit().map_err(sql_error)?;
        Ok(affected > 0)
    }

    async fn delete_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        let mut guard = self.lock()?;
        let connection = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        super::guarded_source_write::authorize_owned_mutation(
            &connection,
            CodeSourceRecordKind::Relationship,
            id.as_str(),
            scope,
        )?;
        let affected = connection
            .execute(
                r#"
                DELETE FROM knowledge_relationships_records
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace,],
            )
            .map_err(sql_error)?;
        connection.commit().map_err(sql_error)?;
        Ok(affected > 0)
    }
}
