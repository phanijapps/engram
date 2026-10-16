//! Attribute known legacy document versions without selecting any as current.
use super::schema::{json_error, sql_error};
use engram_domain::DocumentHeadKey;
use engram_runtime::CoreResult;
use rusqlite::{Connection, params};

pub(super) fn attribute(
    conn: &Connection,
    key: &str,
    identity: &DocumentHeadKey,
) -> CoreResult<()> {
    let scope = serde_json::to_string(&identity.scope).map_err(json_error)?;
    conn.execute("INSERT OR IGNORE INTO document_head_records(head_key,attempt_id,kind,record_id)
      SELECT ?1,'legacy-doc:'||d.id,'document',d.id FROM knowledge_documents d JOIN knowledge_sources s ON s.id=d.source_id
      WHERE json_extract(s.record_json,'$.scope')=?2 AND json_extract(s.record_json,'$.metadata.stableSourceKey')=?3
      AND (json_extract(d.record_json,'$.path')=?4 OR json_extract(d.record_json,'$.uri')=?4)
      AND NOT EXISTS(SELECT 1 FROM document_head_records r WHERE r.kind='document' AND r.record_id=d.id)",params![key,scope,identity.repository,identity.source_key]).map_err(sql_error)?;
    conn.execute("INSERT OR IGNORE INTO document_head_records(head_key,attempt_id,kind,record_id)
      SELECT d.head_key,d.attempt_id,'source',s.id FROM document_head_records d JOIN knowledge_documents doc ON doc.id=d.record_id JOIN knowledge_sources s ON s.id=doc.source_id
      WHERE d.head_key=?1 AND d.kind='document' AND d.attempt_id LIKE 'legacy-doc:%'
      AND NOT EXISTS(SELECT 1 FROM code_source_contributions c WHERE c.kind='source' AND c.record_id=s.id)
      AND NOT EXISTS(SELECT 1 FROM document_head_records r WHERE r.kind='source' AND r.record_id=s.id)",[key]).map_err(sql_error)?;
    conn.execute("INSERT OR IGNORE INTO document_head_records(head_key,attempt_id,kind,record_id)
      SELECT r.head_key,r.attempt_id,'chunk',c.id FROM document_head_records r JOIN knowledge_chunks_records c ON c.document_id=r.record_id
      WHERE r.head_key=?1 AND r.kind='document' AND r.attempt_id LIKE 'legacy-doc:%'
      AND json_extract(c.record_json,'$.code') IS NULL",[key]).map_err(sql_error)?;
    conn.execute("INSERT OR IGNORE INTO document_head_records(head_key,attempt_id,kind,record_id)
      SELECT d.head_key,d.attempt_id,'graph',g.id FROM document_head_records d JOIN knowledge_graphs_records g ON json_extract(g.record_json,'$.metadata.documentId')=d.record_id
      WHERE d.head_key=?1 AND d.kind='document' AND d.attempt_id LIKE 'legacy-doc:%'
      AND NOT EXISTS(SELECT 1 FROM code_source_contributions c WHERE c.kind='graph' AND c.record_id=g.id)
      AND NOT EXISTS(SELECT 1 FROM document_head_records r WHERE r.kind='graph' AND r.record_id=g.id)", [key]).map_err(sql_error)?;
    for (table, kind) in [
        ("knowledge_entities_records", "entity"),
        ("knowledge_relationships_records", "relationship"),
        ("knowledge_unresolved_refs_records", "unresolved_reference"),
    ] {
        conn.execute(&format!("INSERT OR IGNORE INTO document_head_records(head_key,attempt_id,kind,record_id)
          SELECT g.head_key,g.attempt_id,'{kind}',r.id FROM document_head_records g JOIN {table} r ON r.graph_id=g.record_id
          WHERE g.head_key=?1 AND g.kind='graph' AND g.attempt_id LIKE 'legacy-doc:%'
          AND NOT EXISTS(SELECT 1 FROM code_source_contributions c WHERE c.kind='{kind}' AND c.record_id=r.id)
          AND NOT EXISTS(SELECT 1 FROM document_head_records d WHERE d.kind='{kind}' AND d.record_id=r.id)"), [key]).map_err(sql_error)?;
    }
    Ok(())
}
