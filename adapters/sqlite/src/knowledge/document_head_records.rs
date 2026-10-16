//! Validation and canonical writes within the owning head transaction.
use super::{
    document_head_state::{conflict, invalid},
    schema::{json_error, sql_error},
};
use engram_domain::*;
use engram_knowledge::DocumentHeadWrite;
use engram_runtime::CoreResult;
use rusqlite::{Connection, OptionalExtension, params};

fn record<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    table: &str,
    id: &str,
) -> CoreResult<T> {
    // table is always a literal from this adapter, never caller input.
    conn.query_row(
        &format!("SELECT record_json FROM {table} WHERE id=?1"),
        [id],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .map_err(sql_error)?
    .ok_or_else(|| invalid("document head canonical inventory is missing"))
    .and_then(|v| serde_json::from_str(&v).map_err(json_error))
}
fn policy(h: &DocumentHead, p: &Policy, v: &Provenance) -> CoreResult<()> {
    if p != &h.policy || v != &h.provenance {
        return Err(invalid(
            "document inventory policy/provenance differs from head",
        ));
    }
    Ok(())
}
fn source(conn: &Connection, h: &DocumentHead, id: &SourceId) -> CoreResult<KnowledgeSource> {
    let s: KnowledgeSource = record(conn, "knowledge_sources", id.as_str())?;
    if s.scope != h.key.scope {
        return Err(invalid(
            "document source scope differs from exact head scope",
        ));
    }
    let repository = s
        .metadata
        .as_ref()
        .and_then(|m| m.get("stableSourceKey"))
        .and_then(|v| v.as_str())
        .unwrap_or(&s.name);
    if repository != h.key.repository {
        return Err(invalid("document source repository differs from head"));
    }
    policy(h, &s.policy, &s.provenance)?;
    Ok(s)
}
fn document(conn: &Connection, h: &DocumentHead, d: &SourceDocument) -> CoreResult<()> {
    if h.document_id.as_ref() != Some(&d.id) || d.content_hash != h.source_hash {
        return Err(invalid("document identity/hash differs from head"));
    }
    policy(h, &d.policy, &d.provenance)?;
    let s = source(conn, h, &d.source_id)?;
    if d.path.as_deref().or(d.uri.as_deref()).unwrap_or(&s.name) != h.key.source_key {
        return Err(invalid("document source key differs from head"));
    }
    Ok(())
}
fn chunk(conn: &Connection, h: &DocumentHead, c: &KnowledgeChunk) -> CoreResult<()> {
    if !h.chunk_ids.contains(&c.id)
        || h.document_id.as_ref() != Some(&c.document_id)
        || c.code.is_some()
    {
        return Err(invalid(
            "document chunk is not in declared inventory or needs code-source fencing",
        ));
    }
    policy(h, &c.policy, &c.provenance)?;
    let d: SourceDocument = record(conn, "knowledge_documents", c.document_id.as_str())?;
    document(conn, h, &d)?;
    if c.source_id != d.source_id {
        return Err(invalid(
            "document chunk source differs from parent document",
        ));
    }
    Ok(())
}

pub(super) fn write(
    conn: &Connection,
    key: &str,
    h: &DocumentHead,
    payload: DocumentHeadWrite,
) -> CoreResult<()> {
    let (kind, id) = match &payload {
        DocumentHeadWrite::Source(v) => ("source", v.id.as_str()),
        DocumentHeadWrite::Document(v) => ("document", v.id.as_str()),
        DocumentHeadWrite::Chunk(v) => ("chunk", v.id.as_str()),
        DocumentHeadWrite::Graph(v) => ("graph", v.id.as_str()),
        DocumentHeadWrite::Entity(v) => ("entity", v.id.as_str()),
        DocumentHeadWrite::Relationship(v) => ("relationship", v.id.as_str()),
        DocumentHeadWrite::UnresolvedReference(v) => ("unresolved_reference", v.id.as_str()),
    };
    let record_kind = match kind {
        "source" => CodeSourceRecordKind::Source,
        "document" => CodeSourceRecordKind::Document,
        "graph" => CodeSourceRecordKind::Graph,
        "entity" => CodeSourceRecordKind::Entity,
        "relationship" => CodeSourceRecordKind::Relationship,
        "unresolved_reference" => CodeSourceRecordKind::UnresolvedReference,
        _ => CodeSourceRecordKind::Chunk,
    };
    super::guarded_source_write::reject_foreign_record_scope(conn, record_kind, id, &h.key.scope)?;
    let code_owned: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE kind=?1 AND record_id=?2)",
            params![kind, id],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    if code_owned {
        return Err(conflict(
            "code-source-owned canonical record requires its code writer",
        ));
    }
    let foreign:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM document_head_records WHERE kind=?1 AND record_id=?2 AND head_key!=?3)",params![kind,id,key],|r|r.get(0)).map_err(sql_error)?;
    if foreign {
        return Err(conflict(
            "canonical document record belongs to another head",
        ));
    }
    let id = id.to_string();
    match payload {
        DocumentHeadWrite::Source(v) => {
            if v.scope != h.key.scope {
                return Err(invalid("document source scope differs from head"));
            }
            policy(h, &v.policy, &v.provenance)?;
            let repo = v
                .metadata
                .as_ref()
                .and_then(|m| m.get("stableSourceKey"))
                .and_then(|v| v.as_str())
                .unwrap_or(&v.name);
            if repo != h.key.repository {
                return Err(invalid("document source repository differs from head"));
            }
            // Also protect unowned legacy identities from a foreign scope.
            let prior: Option<String> = conn
                .query_row(
                    "SELECT record_json FROM knowledge_sources WHERE id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            if let Some(prior) = prior {
                let s: KnowledgeSource = serde_json::from_str(&prior).map_err(json_error)?;
                if s.scope != v.scope {
                    return Err(conflict("cannot overwrite source in another scope"));
                }
            }
            super::record_writes::put_source(conn, v)?;
        }
        DocumentHeadWrite::Document(v) => {
            document(conn, h, &v)?;
            super::record_writes::put_document(conn, v)?;
        }
        DocumentHeadWrite::Chunk(v) => {
            chunk(conn, h, &v)?;
            super::document_derived_write::immutable_chunk(conn, &v)?;
            super::record_writes::put_chunk(conn, v)?;
        }
        derived => super::document_derived_write::write(conn, key, h, derived)?,
    }
    conn.execute("INSERT OR IGNORE INTO document_head_records(head_key,attempt_id,kind,record_id) VALUES(?1,?2,?3,?4)",params![key,h.attempt_id,kind,id]).map_err(sql_error)?;
    Ok(())
}
pub(super) fn verify(conn: &Connection, key: &str, h: &DocumentHead) -> CoreResult<()> {
    let id = h
        .document_id
        .as_ref()
        .ok_or_else(|| invalid("active document head has no document"))?;
    let d: SourceDocument = record(conn, "knowledge_documents", id.as_str())?;
    document(conn, h, &d)?;
    let mut required = vec![("source", d.source_id.as_str()), ("document", id.as_str())];
    for id in &h.chunk_ids {
        let c: KnowledgeChunk = record(conn, "knowledge_chunks_records", id.as_str())?;
        chunk(conn, h, &c)?;
        required.push(("chunk", id.as_str()));
    }
    for (kind, id) in required {
        let present:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM document_head_records WHERE head_key=?1 AND attempt_id=?2 AND kind=?3 AND record_id=?4)",params![key,h.attempt_id,kind,id],|r|r.get(0)).map_err(sql_error)?;
        if !present {
            return Err(invalid(
                "document inventory was not written under the current lease",
            ));
        }
    }
    super::document_derived_write::verify(conn, key, h)?;
    Ok(())
}
