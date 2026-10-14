//! Validated record dispatch inside the knowledge cell's source transaction.
use super::{
    record_writes,
    schema::sql_error,
    source_inventory::{invalid, record_kind},
};
use engram_domain::*;
use engram_knowledge::CodeSourceWrite;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, params};

pub(super) fn write(
    conn: &Connection,
    key: &str,
    m: &CodeSourceGeneration,
    payload: CodeSourceWrite,
) -> CoreResult<()> {
    let record = payload.record();
    reject_foreign_record_scope(conn, record.kind, &record.id, &m.file.scope)?;
    let staged:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE file_key=?1 AND attempt_id=?2 AND kind=?3 AND record_id=?4)",params![key,m.attempt_id,record_kind(record.kind),record.id],|r|r.get(0)).map_err(sql_error)?;
    if !staged {
        return Err(CoreError::Conflict {
            reason: "source write is not in staged inventory".into(),
        });
    }
    match payload {
        CodeSourceWrite::UnresolvedReference(v) => {
            check_scope(&v.scope, m)?;
            require_staged(
                conn,
                key,
                m,
                CodeSourceRecordKind::Entity,
                v.from_entity_id.as_str(),
            )?;
            if let Some(graph) = &v.graph_id {
                require_staged(conn, key, m, CodeSourceRecordKind::Graph, graph.as_str())?;
            }
            if !preserve_independent_owner(
                conn,
                key,
                m,
                CodeSourceRecordKind::UnresolvedReference,
                v.id.as_str(),
                &serde_json::to_value(&v).map_err(super::schema::json_error)?,
            )? {
                record_writes::put_unresolved_reference(conn, v)?;
            }
        }
        CodeSourceWrite::Source(v) => {
            check_scope(&v.scope, m)?;
            check_policy(&v.policy, &v.provenance, m)?;
            if v.metadata
                .as_ref()
                .and_then(|v| v.get("stableSourceKey"))
                .and_then(|v| v.as_str())
                != Some(m.file.repository.as_str())
            {
                return Err(invalid("source repository identity mismatch"));
            }
            if !preserve_independent_owner(
                conn,
                key,
                m,
                CodeSourceRecordKind::Source,
                v.id.as_str(),
                &serde_json::to_value(&v).map_err(super::schema::json_error)?,
            )? {
                record_writes::put_source(conn, v)?;
            }
        }
        CodeSourceWrite::Document(v) => {
            check_policy(&v.policy, &v.provenance, m)?;
            if v.kind != SourceDocumentKind::Code
                || v.path.as_deref() != Some(&m.file.path)
                || v.content_hash != m.source_hash
            {
                return Err(invalid("code document does not match source file"));
            }
            require_staged(
                conn,
                key,
                m,
                CodeSourceRecordKind::Source,
                v.source_id.as_str(),
            )?;
            if !preserve_independent_owner(
                conn,
                key,
                m,
                CodeSourceRecordKind::Document,
                v.id.as_str(),
                &serde_json::to_value(&v).map_err(super::schema::json_error)?,
            )? {
                record_writes::put_document(conn, v)?;
            }
        }
        CodeSourceWrite::Chunk(v) => {
            check_policy(&v.policy, &v.provenance, m)?;
            let code = v
                .code
                .as_ref()
                .ok_or_else(|| invalid("source chunk needs typed coverage"))?;
            if code.file != m.file
                || code.generation != m.generation
                || code.source_hash != m.source_hash
                || code.fingerprint != m.fingerprint
                || v.document_kind != Some(SourceDocumentKind::Code)
            {
                return Err(invalid("chunk is from another source generation"));
            }
            require_staged(
                conn,
                key,
                m,
                CodeSourceRecordKind::Document,
                v.document_id.as_str(),
            )?;
            require_staged(
                conn,
                key,
                m,
                CodeSourceRecordKind::Source,
                v.source_id.as_str(),
            )?;
            if code.ranges.is_empty()
                || code
                    .ranges
                    .iter()
                    .any(|r| r.end < r.start || r.end > m.byte_length)
            {
                return Err(invalid("chunk range is outside the source"));
            }
            if !preserve_independent_owner(
                conn,
                key,
                m,
                CodeSourceRecordKind::Chunk,
                v.id.as_str(),
                &serde_json::to_value(&v).map_err(super::schema::json_error)?,
            )? {
                record_writes::put_chunk(conn, v)?;
            }
        }
        CodeSourceWrite::Graph(v) => {
            check_scope(&v.scope, m)?;
            check_policy(&v.policy, &v.provenance, m)?;
            let doc = v
                .metadata
                .as_ref()
                .and_then(|v| v.get("documentId"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid("code graph has no document association"))?;
            require_staged(conn, key, m, CodeSourceRecordKind::Document, doc)?;
            if v.metadata
                .as_ref()
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str())
                != Some(&m.file.path)
            {
                return Err(invalid("code graph belongs to another file"));
            }
            if !preserve_independent_owner(
                conn,
                key,
                m,
                CodeSourceRecordKind::Graph,
                v.id.as_str(),
                &serde_json::to_value(&v).map_err(super::schema::json_error)?,
            )? {
                record_writes::put_graph(conn, v)?;
            }
        }
        CodeSourceWrite::Entity(v) => {
            check_scope(&v.scope, m)?;
            check_actor(&v.provenance, m)?;
            if let Some(graph) = &v.graph_id {
                require_staged(conn, key, m, CodeSourceRecordKind::Graph, graph.as_str())?;
            }
            if !preserve_independent_owner(
                conn,
                key,
                m,
                CodeSourceRecordKind::Entity,
                v.id.as_str(),
                &serde_json::to_value(&v).map_err(super::schema::json_error)?,
            )? {
                record_writes::put_entity(conn, v)?;
            }
        }
        CodeSourceWrite::Relationship(v) => {
            check_scope(&v.scope, m)?;
            check_actor(&v.provenance, m)?;
            if let Some(graph) = &v.graph_id {
                require_staged(conn, key, m, CodeSourceRecordKind::Graph, graph.as_str())?;
            }
            if !preserve_independent_owner(
                conn,
                key,
                m,
                CodeSourceRecordKind::Relationship,
                v.id.as_str(),
                &serde_json::to_value(&v).map_err(super::schema::json_error)?,
            )? {
                record_writes::put_relationship(conn, v)?;
            }
        }
    }
    Ok(())
}
fn check_scope(scope: &Scope, m: &CodeSourceGeneration) -> CoreResult<()> {
    if scope != &m.file.scope {
        return Err(CoreError::PolicyDenied {
            reason: "source write scope mismatch".into(),
        });
    }
    Ok(())
}
fn check_actor(provenance: &Provenance, m: &CodeSourceGeneration) -> CoreResult<()> {
    if provenance.actor != m.provenance.actor {
        return Err(CoreError::PolicyDenied {
            reason: "source writer actor mismatch".into(),
        });
    }
    Ok(())
}
fn check_policy(
    policy: &Policy,
    provenance: &Provenance,
    m: &CodeSourceGeneration,
) -> CoreResult<()> {
    check_actor(provenance, m)?;
    if policy != &m.policy {
        return Err(CoreError::PolicyDenied {
            reason: "source write policy mismatch".into(),
        });
    }
    Ok(())
}
fn require_staged(
    conn: &Connection,
    key: &str,
    m: &CodeSourceGeneration,
    kind: CodeSourceRecordKind,
    id: &str,
) -> CoreResult<()> {
    let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE file_key=?1 AND attempt_id=?2 AND kind=?3 AND record_id=?4)",params![key,m.attempt_id,record_kind(kind),id],|r|r.get(0)).map_err(sql_error)?;
    if !exists {
        return Err(invalid(
            "source write association is not staged for this file",
        ));
    }
    Ok(())
}

/// Ordinary CRUD cannot silently overwrite a record after it enters the
/// source ledger. The fenced path calls shared write helpers directly.
pub(super) fn reject_unfenced_write(
    conn: &Connection,
    kind: CodeSourceRecordKind,
    id: &str,
) -> CoreResult<()> {
    let owned: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE kind=?1 AND record_id=?2)",
            params![record_kind(kind), id],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    if owned {
        return Err(CoreError::Conflict {
            reason: "source-owned record requires an updating attempt".into(),
        });
    }
    Ok(())
}

/// A shared physical record cannot be changed beneath another active file view.
fn preserve_independent_owner(
    conn: &Connection,
    key: &str,
    m: &CodeSourceGeneration,
    kind: CodeSourceRecordKind,
    id: &str,
    value: &serde_json::Value,
) -> CoreResult<bool> {
    let shared:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM code_source_contributions c JOIN code_source_generations g ON g.file_key=c.file_key AND g.attempt_id=c.attempt_id WHERE c.kind=?1 AND c.record_id=?2 AND g.state='active' AND c.file_key<>?3)",params![record_kind(kind),id,key],|r|r.get(0)).map_err(sql_error)?;
    if !shared {
        return Ok(false);
    }
    let json: String = conn
        .query_row(
            &format!(
                "SELECT record_json FROM {} WHERE id=?1",
                super::source_inventory::table(kind)
            ),
            [id],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    let stored: serde_json::Value =
        serde_json::from_str(&json).map_err(super::schema::json_error)?;
    let incoming = value;
    if &stored != incoming {
        return Err(CoreError::Conflict {
            reason: format!(
                "source {} would overwrite an independent active contribution",
                m.file.path
            ),
        });
    }
    Ok(true)
}

/// Generic mutations must preserve every exact source scope restriction.
pub(super) fn authorize_owned_mutation(
    conn: &Connection,
    kind: CodeSourceRecordKind,
    id: &str,
    scope: &Scope,
) -> CoreResult<()> {
    let mut stmt=conn.prepare("SELECT g.record_json FROM code_source_contributions c JOIN code_source_generations g ON g.file_key=c.file_key WHERE c.kind=?1 AND c.record_id=?2").map_err(sql_error)?;
    let rows = stmt
        .query_map(params![record_kind(kind), id], |r| r.get::<_, String>(0))
        .map_err(sql_error)?;
    for row in rows {
        let m: CodeSourceGeneration =
            serde_json::from_str(&row.map_err(sql_error)?).map_err(super::schema::json_error)?;
        if &m.file.scope != scope {
            return Err(CoreError::PolicyDenied {
                reason: "source mutation requires its exact scope".into(),
            });
        }
    }
    Ok(())
}

/// Restoring supplied record IDs cannot overwrite an unrelated scope's data.
fn reject_foreign_record_scope(
    conn: &Connection,
    kind: CodeSourceRecordKind,
    id: &str,
    scope: &Scope,
) -> CoreResult<()> {
    use rusqlite::OptionalExtension;
    let sql=match kind {
  CodeSourceRecordKind::Chunk=>"SELECT s.record_json FROM knowledge_chunks_records c JOIN knowledge_sources s ON s.id=c.source_id WHERE c.id=?1".to_owned(),
  CodeSourceRecordKind::Document=>"SELECT s.record_json FROM knowledge_documents d JOIN knowledge_sources s ON s.id=d.source_id WHERE d.id=?1".to_owned(),
  _=>format!("SELECT record_json FROM {} WHERE id=?1",super::source_inventory::table(kind)),
 };
    if let Some(json) = conn
        .query_row(&sql, [id], |r| r.get::<_, String>(0))
        .optional()
        .map_err(sql_error)?
    {
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(super::schema::json_error)?;
        let existing: Scope = serde_json::from_value(
            value
                .get("scope")
                .cloned()
                .ok_or_else(|| invalid("stored contribution has no scope"))?,
        )
        .map_err(super::schema::json_error)?;
        if existing != *scope {
            return Err(CoreError::PolicyDenied {
                reason: "source record ID collides with another exact scope".into(),
            });
        }
    }
    Ok(())
}
