//! Derived graph validation and immutable canonical chunk content.
use super::{
    document_head_state::invalid,
    schema::{json_error, sql_error},
};
use engram_domain::*;
use engram_knowledge::DocumentHeadWrite;
use engram_runtime::CoreResult;
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn immutable_chunk(conn: &Connection, incoming: &KnowledgeChunk) -> CoreResult<()> {
    let prior: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
            [incoming.id.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some(prior) = prior {
        let c: KnowledgeChunk = serde_json::from_str(&prior).map_err(json_error)?;
        if c.text != incoming.text
            || c.content_hash != incoming.content_hash
            || c.source_id != incoming.source_id
            || c.document_id != incoming.document_id
            || c.kind != incoming.kind
            || c.document_kind != incoming.document_kind
        {
            return Err(invalid(
                "canonical chunk content is immutable per ID; changed content requires a new ID",
            ));
        }
    }
    Ok(())
}
fn association(
    conn: &Connection,
    key: &str,
    h: &DocumentHead,
    scope: &Scope,
    graph: Option<&KnowledgeGraphId>,
) -> CoreResult<()> {
    if scope != &h.key.scope {
        return Err(invalid("derived document record requires exact head scope"));
    }
    let graph = graph.ok_or_else(|| invalid("derived document record requires a graph owner"))?;
    let registered: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM document_head_records WHERE head_key=?1 AND attempt_id=?2 AND kind='graph' AND record_id=?3)", params![key,h.attempt_id,graph.as_str()], |r| r.get(0)).map_err(sql_error)?;
    if !registered {
        return Err(invalid(
            "derived graph is not owned by this document attempt",
        ));
    }
    Ok(())
}
fn validate(
    conn: &Connection,
    key: &str,
    h: &DocumentHead,
    payload: &DocumentHeadWrite,
) -> CoreResult<()> {
    match payload {
        DocumentHeadWrite::Graph(g) => {
            if g.scope != h.key.scope
                || g.policy != h.policy
                || g.provenance != h.provenance
                || g.metadata
                    .as_ref()
                    .and_then(|v| v.get("documentId"))
                    .and_then(|v| v.as_str())
                    != h.document_id.as_ref().map(|v| v.as_str())
            {
                return Err(invalid(
                    "derived graph scope/document/policy/provenance differs from head",
                ));
            }
        }
        DocumentHeadWrite::Entity(e) => association(conn, key, h, &e.scope, e.graph_id.as_ref())?,
        DocumentHeadWrite::Relationship(r) => {
            association(conn, key, h, &r.scope, r.graph_id.as_ref())?
        }
        DocumentHeadWrite::UnresolvedReference(r) => {
            association(conn, key, h, &r.scope, r.graph_id.as_ref())?
        }
        _ => return Err(invalid("expected a derived graph record")),
    }
    Ok(())
}

pub(super) fn write(
    conn: &Connection,
    key: &str,
    h: &DocumentHead,
    payload: DocumentHeadWrite,
) -> CoreResult<()> {
    validate(conn, key, h, &payload)?;
    match payload {
        DocumentHeadWrite::Graph(v) => {
            super::record_writes::put_graph(conn, v)?;
        }
        DocumentHeadWrite::Entity(v) => {
            super::record_writes::put_entity(conn, v)?;
        }
        DocumentHeadWrite::Relationship(v) => {
            super::record_writes::put_relationship(conn, v)?;
        }
        DocumentHeadWrite::UnresolvedReference(v) => {
            super::record_writes::put_unresolved_reference(conn, v)?;
        }
        _ => return Err(invalid("expected a derived graph record")),
    }
    Ok(())
}

pub(super) fn verify(conn: &Connection, key: &str, h: &DocumentHead) -> CoreResult<()> {
    for (table, kind) in [
        ("knowledge_graphs_records", "graph"),
        ("knowledge_entities_records", "entity"),
        ("knowledge_relationships_records", "relationship"),
        ("knowledge_unresolved_refs_records", "unresolved_reference"),
    ] {
        let mut stmt = conn.prepare(&format!("SELECT r.record_json FROM document_head_records d LEFT JOIN {table} r ON r.id=d.record_id WHERE d.head_key=?1 AND d.attempt_id=?2 AND d.kind=?3")).map_err(sql_error)?;
        for row in stmt
            .query_map(params![key, h.attempt_id, kind], |r| {
                r.get::<_, Option<String>>(0)
            })
            .map_err(sql_error)?
        {
            let raw = row
                .map_err(sql_error)?
                .ok_or_else(|| invalid("derived inventory record is missing"))?;
            let payload = match kind {
                "graph" => {
                    DocumentHeadWrite::Graph(serde_json::from_str(&raw).map_err(json_error)?)
                }
                "entity" => {
                    DocumentHeadWrite::Entity(serde_json::from_str(&raw).map_err(json_error)?)
                }
                "relationship" => {
                    DocumentHeadWrite::Relationship(serde_json::from_str(&raw).map_err(json_error)?)
                }
                _ => DocumentHeadWrite::UnresolvedReference(
                    serde_json::from_str(&raw).map_err(json_error)?,
                ),
            };
            validate(conn, key, h, &payload)?;
        }
    }
    Ok(())
}
