//! Adapter-local head identity, leases and persisted transition history.
use super::schema::{json_error, sql_error};
use engram_domain::{CodeSourceState, DocumentHead, DocumentHeadKey};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn conflict(reason: &str) -> CoreError {
    CoreError::Conflict {
        reason: reason.into(),
    }
}
pub(super) fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
pub(super) fn key(key: &DocumentHeadKey) -> CoreResult<String> {
    if key.scope.tenant.is_empty() || key.repository.is_empty() || key.source_key.is_empty() {
        return Err(invalid(
            "document head requires exact nonempty source identity",
        ));
    }
    serde_json::to_string(key).map_err(json_error)
}
pub(super) fn load(conn: &Connection, key: &str) -> CoreResult<Option<DocumentHead>> {
    conn.query_row(
        "SELECT record_json FROM document_heads WHERE head_key=?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .map_err(sql_error)?
    .map(|v| serde_json::from_str(&v).map_err(json_error))
    .transpose()
}
pub(super) fn save(conn: &Connection, key: &str, head: &DocumentHead) -> CoreResult<()> {
    let state = match head.state {
        CodeSourceState::Active => "active",
        CodeSourceState::Updating => "updating",
        CodeSourceState::Incomplete => "incomplete",
    };
    conn.execute("INSERT INTO document_head_history(head_key,attempt_id,record_json) SELECT head_key,attempt_id,record_json FROM document_heads WHERE head_key=?1",[key]).map_err(sql_error)?;
    conn.execute("INSERT INTO document_heads(head_key,attempt_id,state,record_json) VALUES(?1,?2,?3,?4) ON CONFLICT(head_key) DO UPDATE SET attempt_id=excluded.attempt_id,state=excluded.state,record_json=excluded.record_json",params![key,head.attempt_id,state,serde_json::to_string(head).map_err(json_error)?]).map_err(sql_error)?;
    conn.execute(
        "UPDATE code_source_revision SET revision=revision+1 WHERE singleton=1",
        [],
    )
    .map_err(sql_error)?;
    Ok(())
}
pub(super) fn lease(conn: &Connection, key: &str, attempt: &str) -> CoreResult<DocumentHead> {
    let h = load(conn, key)?.ok_or_else(|| conflict("document head lease missing"))?;
    if h.attempt_id != attempt || h.state != CodeSourceState::Updating {
        return Err(conflict("document head lease is superseded or unavailable"));
    }
    Ok(h)
}
pub(super) fn initialize(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS document_heads(head_key TEXT PRIMARY KEY,attempt_id TEXT NOT NULL,state TEXT NOT NULL,record_json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS document_head_attempts(head_key TEXT NOT NULL,attempt_id TEXT NOT NULL,PRIMARY KEY(head_key,attempt_id));
      CREATE TABLE IF NOT EXISTS document_head_records(head_key TEXT NOT NULL,attempt_id TEXT NOT NULL,kind TEXT NOT NULL,record_id TEXT NOT NULL,PRIMARY KEY(head_key,attempt_id,kind,record_id));
      CREATE INDEX IF NOT EXISTS document_head_record_identity ON document_head_records(kind,record_id);
      CREATE TABLE IF NOT EXISTS document_head_history(id INTEGER PRIMARY KEY,head_key TEXT NOT NULL,attempt_id TEXT NOT NULL,record_json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS canonical_document_history(id INTEGER PRIMARY KEY,kind TEXT NOT NULL,record_id TEXT NOT NULL,record_json TEXT NOT NULL);").map_err(sql_error)?;
    for (table, kind) in [
        ("knowledge_sources", "source"),
        ("knowledge_documents", "document"),
        ("knowledge_chunks_records", "chunk"),
        ("knowledge_graphs_records", "graph"),
        ("knowledge_entities_records", "entity"),
        ("knowledge_relationships_records", "relationship"),
        ("knowledge_unresolved_refs_records", "unresolved_reference"),
    ] {
        for event in ["UPDATE", "DELETE"] {
            let changed = if event == "UPDATE" {
                "WHEN OLD.record_json IS NOT NEW.record_json"
            } else {
                ""
            };
            conn.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS document_head_{table}_{event} BEFORE {event} ON {table} {changed} BEGIN
              INSERT INTO canonical_document_history(kind,record_id,record_json) SELECT '{kind}',OLD.id,OLD.record_json WHERE EXISTS(SELECT 1 FROM document_head_records r WHERE r.kind='{kind}' AND r.record_id=OLD.id);
              UPDATE document_heads SET state='incomplete',record_json=json_set(record_json,'$.state','incomplete') WHERE state='active' AND head_key IN (SELECT head_key FROM document_head_records r WHERE r.kind='{kind}' AND r.record_id=OLD.id AND r.attempt_id=document_heads.attempt_id);
              UPDATE code_source_revision SET revision=revision+1 WHERE singleton=1;
            END;")).map_err(sql_error)?;
        }
    }
    Ok(())
}
