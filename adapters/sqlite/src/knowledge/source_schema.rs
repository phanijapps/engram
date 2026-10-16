//! Local migration and eligibility views for source-aware knowledge readers.
use super::schema::sql_error;
use engram_runtime::CoreResult;
use rusqlite::{Connection, OptionalExtension};

pub(super) const RECORD_TABLES: &[&str] = &[
    "knowledge_chunks",
    "knowledge_entities",
    "knowledge_relationships",
    "knowledge_graphs",
    "knowledge_unresolved_refs",
];

pub(super) fn migrate_record_tables(connection: &Connection) -> CoreResult<()> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='code_source_reader')",
            [],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    if exists {
        let minimum: u32 = connection
            .query_row(
                "SELECT minimum_version FROM code_source_reader WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        if minimum > 1 {
            return Err(engram_runtime::CoreError::CapabilityUnsupported {
                capability: "code_source_preservation".into(),
                reason: format!("minimum source reader {minimum} exceeds supported version 1"),
            });
        }
    }

    for table in RECORD_TABLES {
        let kind: Option<String> = connection
            .query_row(
                "SELECT type FROM sqlite_master WHERE name=?1",
                [table],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if kind.as_deref() == Some("table") {
            connection
                .execute_batch(&format!("ALTER TABLE {table} RENAME TO {table}_records"))
                .map_err(sql_error)?;
        }
    }
    Ok(())
}

pub(super) fn initialize_source_views(connection: &Connection) -> CoreResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS code_source_reader(singleton INTEGER PRIMARY KEY CHECK(singleton=1), minimum_version INTEGER NOT NULL);
      INSERT OR IGNORE INTO code_source_reader VALUES(1,1);
      CREATE TABLE IF NOT EXISTS code_source_generations (
      file_key TEXT PRIMARY KEY, generation TEXT NOT NULL, attempt_id TEXT NOT NULL,
      state TEXT NOT NULL, record_json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS code_source_contributions (
      file_key TEXT NOT NULL, attempt_id TEXT NOT NULL, kind TEXT NOT NULL, record_id TEXT NOT NULL,
      PRIMARY KEY(file_key,attempt_id,kind,record_id));
      CREATE INDEX IF NOT EXISTS idx_code_contribution_record ON code_source_contributions(kind,record_id);
      CREATE TABLE IF NOT EXISTS code_source_attempts(file_key TEXT NOT NULL, attempt_id TEXT NOT NULL, PRIMARY KEY(file_key,attempt_id));
      INSERT OR IGNORE INTO code_source_attempts SELECT file_key,attempt_id FROM code_source_generations;
      INSERT OR IGNORE INTO code_source_attempts SELECT file_key,attempt_id FROM code_source_contributions;
      CREATE TABLE IF NOT EXISTS code_source_revision(singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL);
      INSERT OR IGNORE INTO code_source_revision VALUES(1,0);").map_err(sql_error)?;
    super::document_head_state::initialize(connection)?;
    super::code_segments_schema::initialize(connection)?;
    for (table, kind) in RECORD_TABLES.iter().zip([
        "chunk",
        "entity",
        "relationship",
        "graph",
        "unresolved_reference",
    ]) {
        let source_filter = if kind == "chunk" {
            "AND json_extract(r.record_json,'$.code.role') IS NOT 'source_snapshot'
             AND (json_extract(r.record_json,'$.code') IS NULL OR EXISTS (
               SELECT 1 FROM code_source_generations g
               WHERE g.file_key=json_extract(r.record_json,'$.code.file')
               AND g.generation=json_extract(r.record_json,'$.code.generation') AND g.state='active' AND EXISTS(SELECT 1 FROM json_each(g.record_json,'$.policy.allowedUses') WHERE value='retrieval') AND (json_extract(g.record_json,'$.policy.expiresAt') IS NULL OR julianday(json_extract(g.record_json,'$.policy.expiresAt'))>julianday('now'))
               AND EXISTS(SELECT 1 FROM code_source_contributions own WHERE own.file_key=g.file_key AND own.attempt_id=g.attempt_id AND own.kind='chunk' AND own.record_id=r.id)))"
        } else {
            ""
        };
        let projection = if kind == "chunk" {
            "r.rowid AS rowid,r.id,r.document_id,r.source_id,CASE WHEN json_extract(r.record_json,'$.documentKind') IS NOT NULL THEN r.record_json ELSE json_set(r.record_json,'$.documentKind',json_extract(d.record_json,'$.kind')) END AS record_json"
        } else {
            "r.rowid AS rowid,r.*"
        };
        let join = if kind == "chunk" {
            "LEFT JOIN knowledge_documents d ON d.id=r.document_id"
        } else {
            ""
        };
        let document_filter = if kind == "chunk" {
            "AND (NOT EXISTS(SELECT 1 FROM document_head_records dh WHERE dh.kind='chunk' AND dh.record_id=r.id)
              OR EXISTS(SELECT 1 FROM document_head_records dh JOIN document_heads h ON h.head_key=dh.head_key AND h.attempt_id=dh.attempt_id
              WHERE dh.kind='chunk' AND dh.record_id=r.id AND h.state='active' AND json_extract(h.record_json,'$.documentId')=r.document_id
              AND EXISTS(SELECT 1 FROM json_each(h.record_json,'$.policy.allowedUses') WHERE value='retrieval')
              AND (json_extract(h.record_json,'$.policy.expiresAt') IS NULL OR julianday(json_extract(h.record_json,'$.policy.expiresAt'))>julianday('now'))))".to_owned()
        } else {
            format!("AND (NOT EXISTS(SELECT 1 FROM document_head_records d WHERE d.kind='{kind}' AND d.record_id=r.id)
                OR EXISTS(SELECT 1 FROM document_head_records d JOIN document_heads h ON h.head_key=d.head_key AND h.attempt_id=d.attempt_id
                WHERE d.kind='{kind}' AND d.record_id=r.id AND h.state='active'
                AND EXISTS(SELECT 1 FROM json_each(h.record_json,'$.policy.allowedUses') WHERE value='retrieval')
                AND (json_extract(h.record_json,'$.policy.expiresAt') IS NULL OR julianday(json_extract(h.record_json,'$.policy.expiresAt'))>julianday('now'))))")
        };
        // The outer schema transaction makes replacement atomic for other readers.
        connection
            .execute_batch(&format!("DROP VIEW IF EXISTS {table};"))
            .map_err(sql_error)?;
        connection.execute_batch(&format!("CREATE VIEW IF NOT EXISTS {table} AS
          SELECT {projection} FROM {table}_records r {join}
          WHERE (NOT EXISTS (SELECT 1 FROM code_source_contributions c WHERE c.kind='{kind}' AND c.record_id=r.id)
          OR EXISTS (SELECT 1 FROM code_source_contributions c JOIN code_source_generations g
            ON g.file_key=c.file_key AND g.attempt_id=c.attempt_id
            WHERE c.kind='{kind}' AND c.record_id=r.id AND g.state='active' AND EXISTS(SELECT 1 FROM json_each(g.record_json,'$.policy.allowedUses') WHERE value='retrieval') AND (json_extract(g.record_json,'$.policy.expiresAt') IS NULL OR julianday(json_extract(g.record_json,'$.policy.expiresAt'))>julianday('now')))) {source_filter} {document_filter};")).map_err(sql_error)?;
    }
    for (table, kind) in [
        ("knowledge_sources", "source"),
        ("knowledge_documents", "document"),
        ("knowledge_chunks_records", "chunk"),
        ("knowledge_entities_records", "entity"),
        ("knowledge_relationships_records", "relationship"),
        ("knowledge_graphs_records", "graph"),
        ("knowledge_unresolved_refs_records", "unresolved_reference"),
    ] {
        for event in ["UPDATE", "DELETE"] {
            let condition = if event == "UPDATE" {
                "WHEN OLD.record_json IS NOT NEW.record_json"
            } else {
                ""
            };
            connection.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS source_invalidate_{table}_{event} BEFORE {event} ON {table} {condition} BEGIN
              UPDATE code_source_generations SET state='incomplete',record_json=json_set(record_json,'$.state','incomplete') WHERE state='active' AND file_key IN (SELECT file_key FROM code_source_contributions WHERE kind='{kind}' AND record_id=OLD.id);
              UPDATE code_source_revision SET revision=revision+1 WHERE singleton=1;
            END;")).map_err(sql_error)?;
        }
    }
    Ok(())
}
