//! Atomic source-owned shared contract projections, independent of KG CRUD.
use super::{
    document_head_state::invalid,
    schema::{json_error, sql_error},
};
use engram_domain::*;
use engram_runtime::CoreResult;
use rusqlite::{Connection, OptionalExtension, params};

fn load(conn: &Connection, table: &str, id: &str) -> CoreResult<Option<String>> {
    conn.query_row(
        &format!("SELECT record_json FROM {table} WHERE id=?1"),
        [id],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql_error)
}
fn member(entity: &KnowledgeEntity, scope: &Scope, repository: &str) -> CoreResult<()> {
    if repository.is_empty()
        || scope.tenant.is_empty()
        || entity.scope != *scope
        || entity.kind != EntityKind::Api
        || entity.graph_id.is_some()
        || !entity.source_refs.iter().any(|r| {
            r.target_type == EvidenceTargetType::Source
                && r.target_id.as_deref() == Some(repository)
        })
    {
        return Err(invalid("repository contract entity identity mismatch"));
    }
    Ok(())
}
fn edge(rel: &KnowledgeRelationship, entity: &KnowledgeEntity, repository: &str) -> CoreResult<()> {
    if rel.scope != entity.scope
        || rel.graph_id.is_some()
        || rel.predicate != "exposes"
        || rel.subject.id.is_none()
        || rel.subject.kind.as_deref() != Some("repository")
        || rel.subject.name.as_deref() != Some(repository)
        || rel.object.id.as_ref() != Some(&entity.id)
        || rel.object.kind.as_deref() != Some("api")
        || rel.object.name.as_deref() != Some(entity.name.as_str())
    {
        return Err(invalid("repository contract edge identity mismatch"));
    }
    Ok(())
}
fn root(
    conn: &Connection,
    rel: &KnowledgeRelationship,
    repository: &str,
    required: bool,
) -> CoreResult<()> {
    let json = load(
        conn,
        "knowledge_entities_records",
        rel.subject.id.as_ref().unwrap().as_str(),
    )?;
    if let Some(json) = json {
        let entity: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
        if entity.scope != rel.scope
            || entity.kind != EntityKind::Repository
            || entity.graph_id.is_some()
            || entity
                .metadata
                .as_ref()
                .and_then(|m| m.get("stableSourceKey"))
                .and_then(|v| v.as_str())
                != Some(repository)
        {
            return Err(invalid("repository contract root identity mismatch"));
        }
    } else if required {
        return Err(invalid("repository contract root missing"));
    }
    Ok(())
}
fn retain(conn: &Connection, kind: CodeSourceRecordKind, id: &str, json: &str) -> CoreResult<()> {
    super::guarded_source_write::reject_unfenced_write(conn, kind, id)?;
    let kind = super::source_inventory::record_kind(kind);
    // Existing owner triggers archive exactly once on the subsequent mutation.
    conn.execute(
        "INSERT INTO canonical_document_history(kind,record_id,record_json) SELECT ?1,?2,?3
      WHERE NOT EXISTS(SELECT 1 FROM document_head_records WHERE kind=?1 AND record_id=?2)
      AND NOT EXISTS(SELECT 1 FROM code_source_contributions WHERE kind=?1 AND record_id=?2)",
        params![kind, id, json],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn put(
    conn: &Connection,
    scope: &Scope,
    repository: &str,
    mut entity: KnowledgeEntity,
    mut rel: KnowledgeRelationship,
) -> CoreResult<()> {
    member(&entity, scope, repository)?;
    if entity.source_refs.len() != 1
        || entity.source_refs[0].uri.is_some()
        || entity.source_refs[0].quote.is_some()
        || entity.source_refs[0].location.is_some()
    {
        return Err(invalid(
            "repository contract write must add only its own source reference",
        ));
    }
    edge(&rel, &entity, repository)?;
    root(conn, &rel, repository, true)?;
    if let Some(json) = load(conn, "knowledge_entities_records", entity.id.as_str())? {
        let mut prior: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
        if prior.scope != *scope
            || prior.kind != entity.kind
            || prior.graph_id != entity.graph_id
            || prior.name != entity.name
        {
            return Err(invalid(
                "repository contract entity collides with existing identity",
            ));
        }
        if !prior.source_refs.iter().any(|r| {
            r.target_type == EvidenceTargetType::Source
                && r.target_id.as_deref() == Some(repository)
        }) {
            prior.source_refs.extend(entity.source_refs);
        }
        prior.metadata = entity.metadata;
        entity = prior;
        if serde_json::to_string(&entity).map_err(json_error)? != json {
            retain(
                conn,
                CodeSourceRecordKind::Entity,
                entity.id.as_str(),
                &json,
            )?;
        }
    } else {
        super::guarded_source_write::reject_unfenced_write(
            conn,
            CodeSourceRecordKind::Entity,
            entity.id.as_str(),
        )?;
    }
    if let Some(json) = load(conn, "knowledge_relationships_records", rel.id.as_str())? {
        let prior: KnowledgeRelationship = serde_json::from_str(&json).map_err(json_error)?;
        edge(&prior, &entity, repository)?;
        if prior.subject != rel.subject || prior.object != rel.object {
            return Err(invalid(
                "repository contract edge collides with existing identity",
            ));
        }
        // Observation refresh alone is idempotent; semantic changes retain old JSON.
        rel.created_at = prior.created_at;
        rel.updated_at = prior.updated_at;
        rel.provenance = prior.provenance;
        rel.archived_at = prior.archived_at;
        if serde_json::to_string(&rel).map_err(json_error)? != json {
            retain(
                conn,
                CodeSourceRecordKind::Relationship,
                rel.id.as_str(),
                &json,
            )?;
        }
    } else {
        super::guarded_source_write::reject_unfenced_write(
            conn,
            CodeSourceRecordKind::Relationship,
            rel.id.as_str(),
        )?;
    }
    super::record_writes::put_entity(conn, entity)?;
    super::record_writes::put_relationship(conn, rel)?;
    Ok(())
}

pub(super) fn retract(
    conn: &Connection,
    scope: &Scope,
    repository: &str,
    entity_id: &EntityId,
    rel_id: &RelationshipId,
) -> CoreResult<()> {
    let entity = load(conn, "knowledge_entities_records", entity_id.as_str())?;
    let rel = load(conn, "knowledge_relationships_records", rel_id.as_str())?;
    let Some(json) = entity else {
        if rel.is_some() {
            return Err(invalid("repository contract edge has no API entity"));
        }
        return Ok(());
    };
    let mut entity: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
    if rel.is_none()
        && entity.scope == *scope
        && entity.kind == EntityKind::Api
        && entity.graph_id.is_none()
        && !entity.source_refs.iter().any(|r| {
            r.target_type == EvidenceTargetType::Source
                && r.target_id.as_deref() == Some(repository)
        })
    {
        return Ok(());
    }
    member(&entity, scope, repository)?;
    if let Some(old) = rel {
        let relationship: KnowledgeRelationship = serde_json::from_str(&old).map_err(json_error)?;
        edge(&relationship, &entity, repository)?;
        // Last-file cleanup can already have pruned the repository root.
        root(conn, &relationship, repository, false)?;
        retain(
            conn,
            CodeSourceRecordKind::Relationship,
            rel_id.as_str(),
            &old,
        )?;
        conn.execute(
            "DELETE FROM knowledge_relationships_records WHERE id=?1",
            [rel_id.as_str()],
        )
        .map_err(sql_error)?;
    }
    retain(
        conn,
        CodeSourceRecordKind::Entity,
        entity_id.as_str(),
        &json,
    )?;
    entity.source_refs.retain(|r| {
        !(r.target_type == EvidenceTargetType::Source && r.target_id.as_deref() == Some(repository))
    });
    if entity.source_refs.is_empty() {
        conn.execute(
            "DELETE FROM knowledge_entities_records WHERE id=?1",
            [entity_id.as_str()],
        )
        .map_err(sql_error)?;
    } else {
        super::record_writes::put_entity(conn, entity)?;
    }
    Ok(())
}
