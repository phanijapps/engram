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

use crate::knowledge::changes::{append_change_locked, stored_json};
use crate::knowledge::{schema::sql_error, service::SqlKnowledgeStore};

/// Read the persisted `archived_at` for a record (if any), so a re-put preserves a
/// prior maintenance archive instead of silently un-archiving (ingestion always
/// passes `archived_at = None`). Keeps the column and `record_json` in lockstep.
/// Temporal discrimination helper: when the stored entity is semantically
/// identical to the incoming one (same name, kind, graph, source refs, and
/// metadata — ignoring provenance timestamps/created_at/updated_at/validFrom),
/// return the STORED `validFrom` so an unchanged symbol keeps its original
/// validity stamp across re-ingests. `None` means "no prior row, or the
/// entity genuinely changed" — the caller keeps the incoming stamp.
fn persisted_semantically_identical(
    conn: &Connection,
    id: &str,
    incoming: &KnowledgeEntity,
) -> CoreResult<Option<Timestamp>> {
    let sql = "SELECT record_json FROM knowledge_entities WHERE id = ?1";
    let stored: Option<String> = conn
        .query_row(sql, rusqlite::params![id], |row| row.get(0))
        .optional()
        .map_err(sql_error)?;
    let Some(json) = stored else {
        return Ok(None);
    };
    let Ok(prior) = serde_json::from_str::<KnowledgeEntity>(&json) else {
        return Ok(None);
    };
    let unchanged = prior.name == incoming.name
        && prior.kind == incoming.kind
        && prior.graph_id == incoming.graph_id
        && prior.source_refs == incoming.source_refs
        && prior.metadata == incoming.metadata
        && prior.aliases == incoming.aliases;
    if unchanged {
        Ok(prior.valid_from)
    } else {
        Ok(None)
    }
}

fn persisted_archived_at(
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

    async fn get_source(
        &self,
        id: &SourceId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeSource>> {
        let source = self.get_source_by_id(id).await?;
        Ok(source.filter(|source| crate::knowledge::scope::scope_allows(&source.scope, scope)))
    }

    async fn list_sources(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeSource>> {
        SqlKnowledgeStore::list_sources(self, scope).await
    }

    async fn list_documents(&self, scope: &Scope) -> CoreResult<Vec<SourceDocument>> {
        SqlKnowledgeStore::list_documents(self, scope).await
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

    /// Lists a document's chunks within `scope` (scope resolved via the source
    /// join, mirroring `get_chunk`). Used by retraction to find prior chunks.
    async fn list_entities_by_graph(
        &self,
        graph_id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let connection = self.lock()?;
        let mut stmt = connection
            .prepare(
                r#"
                SELECT record_json FROM knowledge_entities
                WHERE graph_id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                "#,
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![
                    graph_id.to_string(),
                    scope.tenant,
                    scope.subject,
                    scope.workspace,
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let mut entities = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            entities
                .push(serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?);
        }
        Ok(entities)
    }

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
    async fn delete_document(&self, id: &DocumentId, _scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        let n = connection
            .execute(
                "DELETE FROM knowledge_documents WHERE id = ?1",
                rusqlite::params![id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(n > 0)
    }

    /// Hard-deletes a single chunk by id (scope-bounded by the reconcile caller).
    async fn delete_chunk(&self, id: &ChunkId, _scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        let n = connection
            .execute(
                "DELETE FROM knowledge_chunks WHERE id = ?1",
                rusqlite::params![id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(n > 0)
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        let connection = self.lock()?;
        let graph_id = entity
            .graph_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        // Preserve a prior maintenance archive across re-put (ingestion passes
        // archived_at = None); keep the column and record_json in lockstep.
        let preserved =
            persisted_archived_at(&connection, "knowledge_entities", &entity.id.to_string())?;
        let mut stored = entity;
        if let Some(ts) = preserved {
            stored.archived_at = Some(ts);
        }
        // Temporal discrimination (whats-changed-temporal-flat): re-ingest
        // re-derives every symbol in a changed file, and a wholesale
        // validFrom re-stamp made `recent` uniformly score the whole repo.
        // When the incoming entity is semantically identical to the stored
        // one, keep the stored validFrom — the stamp moves only on a real
        // change (kind, graph, source-refs, or metadata).
        stored.valid_from =
            match persisted_semantically_identical(&connection, &stored.id.to_string(), &stored)? {
                Some(prior_valid_from) => Some(prior_valid_from),
                None => stored.valid_from,
            };
        // Prior content read BEFORE the write (change-log no-op detection).
        let prior_json = stored_json(&connection, "knowledge_entities", &stored.id.to_string())?;
        let json = serde_json::to_string(&stored).map_err(crate::knowledge::schema::json_error)?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_entities
                    (id, graph_id, tenant, subject, workspace, session, environment, record_json, archived_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(id) DO UPDATE SET
                    graph_id = excluded.graph_id,
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json,
                    archived_at = excluded.archived_at
                "#,
                rusqlite::params![
                    stored.id.to_string(),
                    graph_id,
                    stored.scope.tenant,
                    stored.scope.subject,
                    stored.scope.workspace,
                    stored.scope.session,
                    stored.scope.environment,
                    json,
                    stored.archived_at.map(|t| t.to_rfc3339())
                ],
            )
            .map_err(sql_error)?;
        // Change log (slice 3): upsert event only when the row content
        // actually changed — identical re-puts log nothing (no-op rule).
        let new_json =
            serde_json::to_string(&stored).map_err(crate::knowledge::schema::json_error)?;
        if prior_json.as_deref() != Some(new_json.as_str()) {
            append_change_locked(
                &connection,
                &stored.scope,
                stored.graph_id.as_ref().map(ToString::to_string).as_deref(),
                engram_domain::KnowledgeChangeTarget::Entity,
                &stored.id.to_string(),
                engram_domain::KnowledgeChangeOp::Upsert,
            )?;
        }
        Ok(stored)
    }

    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
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
        // Preserve a prior maintenance archive across re-put.
        let preserved = persisted_archived_at(
            &connection,
            "knowledge_relationships",
            &relationship.id.to_string(),
        )?;
        let mut stored = relationship;
        if let Some(ts) = preserved {
            stored.archived_at = Some(ts);
        }
        // Prior content read BEFORE the write (change-log no-op detection).
        let prior_json = stored_json(
            &connection,
            "knowledge_relationships",
            &stored.id.to_string(),
        )?;
        let json = serde_json::to_string(&stored).map_err(crate::knowledge::schema::json_error)?;
        connection
            .execute(
                r#"
                INSERT INTO knowledge_relationships
                    (id, graph_id, subject_id, tenant, subject, workspace, session, environment, record_json, archived_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(id) DO UPDATE SET
                    graph_id = excluded.graph_id,
                    subject_id = excluded.subject_id,
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json,
                    archived_at = excluded.archived_at
                "#,
                rusqlite::params![
                    stored.id.to_string(),
                    graph_id,
                    subject_id,
                    stored.scope.tenant,
                    stored.scope.subject,
                    stored.scope.workspace,
                    stored.scope.session,
                    stored.scope.environment,
                    json,
                    stored.archived_at.map(|t| t.to_rfc3339())
                ],
            )
            .map_err(sql_error)?;
        // Change log: upsert event only on content change (no-op rule).
        let new_json =
            serde_json::to_string(&stored).map_err(crate::knowledge::schema::json_error)?;
        if prior_json.as_deref() != Some(new_json.as_str()) {
            append_change_locked(
                &connection,
                &stored.scope,
                stored.graph_id.as_ref().map(ToString::to_string).as_deref(),
                engram_domain::KnowledgeChangeTarget::Relationship,
                &stored.id.to_string(),
                engram_domain::KnowledgeChangeOp::Upsert,
            )?;
        }
        Ok(stored)
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
        if affected > 0 {
            append_change_locked(
                &connection,
                scope,
                None,
                engram_domain::KnowledgeChangeTarget::Entity,
                &id.to_string(),
                engram_domain::KnowledgeChangeOp::Remove,
            )?;
        }
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

    /// Lists entities whose `source_refs` carry a source ref targeting
    /// `source_id`, within `scope` (queryable-by-source surface for sourced
    /// distill batches). Scope-filtered scan with the ref membership checked in
    /// Rust over the parsed record — the same shape `list_entities_by_graph`
    /// uses for un-indexed record fields. The scope predicate is the canonical
    /// rule (`scope_allows`): a `None` request field is unconstrained, a
    /// `Some` field requires record equality — a NULL record column is NOT a
    /// wildcard, and session/environment are constrained like subject/workspace
    /// (FIND-security-002 / FIND-data-001).
    async fn list_entities_by_source_ref(
        &self,
        source_id: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let connection = self.lock()?;
        let mut stmt = connection
            .prepare(
                r#"
                SELECT record_json FROM knowledge_entities
                WHERE tenant = ?1
                    AND (?2 IS NULL OR subject = ?2)
                    AND (?3 IS NULL OR workspace = ?3)
                    AND (?4 IS NULL OR session = ?4)
                    AND (?5 IS NULL OR environment = ?5)
                    AND archived_at IS NULL
                ORDER BY id
                "#,
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![
                    scope.tenant,
                    scope.subject,
                    scope.workspace,
                    scope.session,
                    scope.environment
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let mut entities = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let entity: KnowledgeEntity =
                serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?;
            if has_source_ref(&entity.source_refs, source_id) {
                entities.push(entity);
            }
        }
        Ok(entities)
    }

    /// Lists relationships whose `evidence` refs target `source_id`, within
    /// `scope`. Same shape and semantics as
    /// [`Self::list_entities_by_source_ref`] (canonical scope predicate).
    async fn list_relationships_by_source_ref(
        &self,
        source_id: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let mut stmt = connection
            .prepare(
                r#"
                SELECT record_json FROM knowledge_relationships
                WHERE tenant = ?1
                    AND (?2 IS NULL OR subject = ?2)
                    AND (?3 IS NULL OR workspace = ?3)
                    AND (?4 IS NULL OR session = ?4)
                    AND (?5 IS NULL OR environment = ?5)
                    AND archived_at IS NULL
                ORDER BY id
                "#,
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![
                    scope.tenant,
                    scope.subject,
                    scope.workspace,
                    scope.session,
                    scope.environment
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let mut relationships = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let relationship: KnowledgeRelationship =
                serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?;
            if has_source_ref(&relationship.evidence, source_id) {
                relationships.push(relationship);
            }
        }
        Ok(relationships)
    }

    /// Clears `archived_at` on one entity (single-record restore, no incident
    /// cascade — supersession archives per record by source-ref count). Mirrors
    /// the maintenance restore's provenance re-stamp without the cascade.
    async fn restore_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        let json: Option<String> = connection
            .query_row(
                r#"
                SELECT record_json FROM knowledge_entities
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                    AND archived_at IS NOT NULL
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(json) = json else {
            return Ok(false);
        };
        let mut entity: KnowledgeEntity =
            serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?;
        if !crate::knowledge::scope::scope_allows(&entity.scope, scope) {
            return Ok(false);
        }
        let now = Utc::now();
        entity.archived_at = None;
        entity.updated_at = Some(now);
        entity.provenance.observed_at = now;
        entity.provenance.method = Some("restore".to_owned());
        let new_json =
            serde_json::to_string(&entity).map_err(crate::knowledge::schema::json_error)?;
        connection
            .execute(
                "UPDATE knowledge_entities SET archived_at = NULL, record_json = ?1 WHERE id = ?2",
                rusqlite::params![new_json, id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(true)
    }

    /// Clears `archived_at` on one relationship. Same semantics as
    /// [`Self::restore_entity`].
    async fn restore_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock()?;
        let json: Option<String> = connection
            .query_row(
                r#"
                SELECT record_json FROM knowledge_relationships
                WHERE id = ?1
                    AND tenant = ?2
                    AND (subject IS NULL OR subject = ?3)
                    AND (workspace IS NULL OR workspace = ?4)
                    AND archived_at IS NOT NULL
                "#,
                rusqlite::params![id.to_string(), scope.tenant, scope.subject, scope.workspace],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(json) = json else {
            return Ok(false);
        };
        let mut relationship: KnowledgeRelationship =
            serde_json::from_str(&json).map_err(crate::knowledge::schema::json_error)?;
        if !crate::knowledge::scope::scope_allows(&relationship.scope, scope) {
            return Ok(false);
        }
        let now = Utc::now();
        relationship.archived_at = None;
        relationship.updated_at = Some(now);
        relationship.provenance.observed_at = now;
        relationship.provenance.method = Some("restore".to_owned());
        let new_json =
            serde_json::to_string(&relationship).map_err(crate::knowledge::schema::json_error)?;
        connection
            .execute(
                "UPDATE knowledge_relationships SET archived_at = NULL, record_json = ?1 WHERE id = ?2",
                rusqlite::params![new_json, id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(true)
    }
}

/// True when the ref list carries a source ref targeting `source_id` — the
/// supersession membership guard (records without the ref are immune, e.g.
/// code-scan-derived records attributed by `graph_id`/`stable_source_key`).
fn has_source_ref(refs: &[EvidenceRef], source_id: &str) -> bool {
    refs.iter().any(|r| {
        r.target_type == EvidenceTargetType::Source && r.target_id.as_deref() == Some(source_id)
    })
}
