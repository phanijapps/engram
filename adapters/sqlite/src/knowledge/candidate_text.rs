//! Canonical source-eligible text for a local lexical-index write.
use super::{
    schema::{json_error, sql_error},
    service::SqlKnowledgeStore,
};
use engram_domain::{KnowledgeChunk, KnowledgeEntity};
use engram_runtime::CoreResult;
use rusqlite::OptionalExtension;

impl SqlKnowledgeStore {
    /// Local index migration reads only eligible views, never source snapshots.
    pub fn all_semantic_lexical_entries(&self) -> CoreResult<Vec<(String, String)>> {
        let conn = self.lock()?;
        let mut entries = Vec::new();
        let mut chunks = conn
            .prepare("SELECT id,record_json FROM knowledge_chunks")
            .map_err(sql_error)?;
        let rows = chunks
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql_error)?;
        for row in rows {
            let (id, json) = row.map_err(sql_error)?;
            let chunk: KnowledgeChunk = serde_json::from_str(&json).map_err(json_error)?;
            entries.push((id, chunk.text));
        }
        let mut entities = conn
            .prepare("SELECT id,record_json FROM knowledge_entities WHERE archived_at IS NULL")
            .map_err(sql_error)?;
        let rows = entities
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql_error)?;
        for row in rows {
            let (id, json) = row.map_err(sql_error)?;
            let entity: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
            entries.push((id, format!("{} {:?}", entity.name, entity.kind)));
        }
        Ok(entries)
    }

    /// Legacy export cannot bypass source-export authorization via old families.
    pub fn has_any_code_contribution(&self, id: &str) -> CoreResult<bool> {
        self.lock()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE record_id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(sql_error)
    }
    /// Whether this target has source generation ownership (including legacy
    /// ownership registered during migration).
    pub fn has_code_contribution(&self, id: &str) -> CoreResult<bool> {
        self.lock()?.query_row("SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE record_id=?1 AND kind IN ('chunk','entity'))",[id],|r|r.get(0)).map_err(sql_error)
    }
    /// Rehydrates current eligible knowledge, rather than trusting caller text
    /// to turn a raw source copy into an indexed semantic target.
    pub fn semantic_lexical_text(&self, id: &str) -> CoreResult<Option<String>> {
        let conn = self.lock()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT record_json FROM knowledge_chunks WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some(json) = json {
            let chunk: KnowledgeChunk = serde_json::from_str(&json).map_err(json_error)?;
            return Ok(Some(chunk.text));
        }
        let json: Option<String> = conn
            .query_row(
                "SELECT record_json FROM knowledge_entities WHERE id=?1 AND archived_at IS NULL",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        json.map(|json| {
            let entity: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
            Ok(format!("{} {:?}", entity.name, entity.kind))
        })
        .transpose()
    }
}
