//! Additive derived-index schema; canonical source revision stays independent.
use super::schema::sql_error;
use engram_runtime::CoreResult;
use rusqlite::Connection;

pub(super) fn initialize(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS code_segment_inventories(file_key TEXT NOT NULL,representation_id TEXT NOT NULL,record_json TEXT NOT NULL,lease_revision INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(file_key,representation_id));
      CREATE TABLE IF NOT EXISTS code_embedding_segments(id TEXT PRIMARY KEY,file_key TEXT NOT NULL,representation_id TEXT NOT NULL,record_json TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS knowledge_code_unit_idx ON knowledge_chunks_records(json_extract(record_json,'$.code.unit')) WHERE json_extract(record_json,'$.code.unit') IS NOT NULL;
      CREATE INDEX IF NOT EXISTS code_segments_file_representation ON code_embedding_segments(file_key,representation_id);
      CREATE TABLE IF NOT EXISTS code_index_revision(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL);
      INSERT OR IGNORE INTO code_index_revision VALUES(1,0);
      CREATE TRIGGER IF NOT EXISTS code_index_source_revision AFTER UPDATE ON code_source_revision BEGIN UPDATE code_index_revision SET revision=revision+1 WHERE singleton=1; END;").map_err(sql_error)?;
    for (table, invalidation) in [
        (
            "code_embedding_segments",
            "UPDATE code_segment_inventories SET record_json=json_set(record_json,'$.state','incomplete') WHERE file_key=OLD.file_key AND representation_id=OLD.representation_id AND json_extract(record_json,'$.state')='active' AND EXISTS(SELECT 1 FROM json_each(record_json,'$.segmentIds') WHERE value=OLD.id);",
        ),
        ("code_segment_inventories", ""),
    ] {
        for event in ["UPDATE", "DELETE"] {
            let changed = if event == "UPDATE" {
                if table == "code_embedding_segments" {
                    "WHEN OLD.record_json IS NOT NEW.record_json OR OLD.id IS NOT NEW.id OR OLD.file_key IS NOT NEW.file_key OR OLD.representation_id IS NOT NEW.representation_id"
                } else {
                    "WHEN OLD.record_json IS NOT NEW.record_json OR OLD.file_key IS NOT NEW.file_key OR OLD.representation_id IS NOT NEW.representation_id OR OLD.lease_revision IS NOT NEW.lease_revision"
                }
            } else {
                ""
            };
            conn.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS code_index_{table}_{event} BEFORE {event} ON {table} {changed} BEGIN {invalidation} UPDATE code_index_revision SET revision=revision+1 WHERE singleton=1; END;")).map_err(sql_error)?;
        }
    }
    Ok(())
}
