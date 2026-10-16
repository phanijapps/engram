//! Synchronous knowledge record writes shared by ordinary CRUD and fenced source writes.
use super::schema::sql_error;
use engram_domain::*;
use engram_runtime::CoreResult;
use rusqlite::Connection;
pub(super) fn put_source(
    connection: &Connection,
    source: KnowledgeSource,
) -> CoreResult<KnowledgeSource> {
    let json = serde_json::to_string(&source).map_err(crate::knowledge::schema::json_error)?;
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
pub(super) fn put_document(
    connection: &Connection,
    document: SourceDocument,
) -> CoreResult<SourceDocument> {
    let json = serde_json::to_string(&document).map_err(crate::knowledge::schema::json_error)?;
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
pub(super) fn put_chunk(
    connection: &Connection,
    chunk: KnowledgeChunk,
) -> CoreResult<KnowledgeChunk> {
    let json = serde_json::to_string(&chunk).map_err(crate::knowledge::schema::json_error)?;
    connection
        .execute(
            r#"
                INSERT INTO knowledge_chunks_records
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
pub(super) fn put_entity(
    connection: &Connection,
    entity: KnowledgeEntity,
) -> CoreResult<KnowledgeEntity> {
    let graph_id = entity
        .graph_id
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    // Preserve a prior maintenance archive across re-put (ingestion passes
    // archived_at = None); keep the column and record_json in lockstep.
    let preserved = super::knowledge::persisted_archived_at(
        connection,
        "knowledge_entities_records",
        &entity.id.to_string(),
    )?;
    let mut stored = entity;
    if let Some(ts) = preserved {
        stored.archived_at = Some(ts);
    }
    let json = serde_json::to_string(&stored).map_err(crate::knowledge::schema::json_error)?;
    connection
            .execute(
                r#"
                INSERT INTO knowledge_entities_records
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
    Ok(stored)
}
pub(super) fn put_relationship(
    connection: &Connection,
    relationship: KnowledgeRelationship,
) -> CoreResult<KnowledgeRelationship> {
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
    let preserved = super::knowledge::persisted_archived_at(
        connection,
        "knowledge_relationships_records",
        &relationship.id.to_string(),
    )?;
    let mut stored = relationship;
    if let Some(ts) = preserved {
        stored.archived_at = Some(ts);
    }
    let json = serde_json::to_string(&stored).map_err(crate::knowledge::schema::json_error)?;
    connection
            .execute(
                r#"
                INSERT INTO knowledge_relationships_records
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
    Ok(stored)
}
pub(super) fn put_graph(
    connection: &Connection,
    graph: KnowledgeGraph,
) -> CoreResult<KnowledgeGraph> {
    let json = serde_json::to_string(&graph).map_err(crate::knowledge::schema::json_error)?;
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
    connection
        .execute(
            r#"
                INSERT INTO knowledge_graphs_records
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

pub(super) fn put_unresolved_reference(
    connection: &Connection,
    reference: UnresolvedReference,
) -> CoreResult<()> {
    let json = serde_json::to_string(&reference).map_err(crate::knowledge::schema::json_error)?;
    connection
        .execute(
            "INSERT INTO knowledge_unresolved_refs_records
                       (id, graph_id, tenant, subject, workspace, session,
                        environment, status, record_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(id) DO UPDATE SET
                       status = excluded.status,
                       record_json = excluded.record_json",
            rusqlite::params![
                reference.id.to_string(),
                reference.graph_id.as_ref().map(|g| g.to_string()),
                reference.scope.tenant,
                reference.scope.subject,
                reference.scope.workspace,
                reference.scope.session,
                reference.scope.environment,
                serde_json::to_string(&reference.status)
                    .map_err(crate::knowledge::schema::json_error)?,
                json,
            ],
        )
        .map_err(sql_error)?;

    Ok(())
}
