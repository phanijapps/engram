//! Source-owner policy selection for a request, independent of graph cache state.
use super::{
    SqlKnowledgeStore,
    schema::{json_error, sql_error},
};
use chrono::Utc;
use engram_domain::*;
use engram_retrieval::RecordAccess;
use engram_runtime::{CoreError, CoreResult};
use std::collections::HashMap;
impl SqlKnowledgeStore {
    pub fn source_record_access(&self, request: &RetrievalRequest) -> CoreResult<RecordAccess> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(sql_error)?;
        let mut manifests = HashMap::new();
        {
            let mut stmt = tx
                .prepare(
                    "SELECT file_key,record_json FROM code_source_generations ORDER BY file_key",
                )
                .map_err(sql_error)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(sql_error)?;
            for row in rows {
                let (key, json) = row.map_err(sql_error)?;
                let m: CodeSourceGeneration = serde_json::from_str(&json).map_err(json_error)?;
                manifests.insert(key, m);
            }
        }
        let mut access = RecordAccess::default();
        let now = Utc::now();
        {
            let mut stmt=tx.prepare("SELECT file_key,attempt_id,kind,record_id FROM code_source_contributions WHERE kind IN ('chunk','entity','relationship') ORDER BY file_key,record_id").map_err(sql_error)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .map_err(sql_error)?;
            for row in rows {
                let (key, attempt, kind, id) = row.map_err(sql_error)?;
                let kind = match kind.as_str() {
                    "chunk" => RetrievalTargetType::Chunk,
                    "entity" => RetrievalTargetType::Entity,
                    "relationship" => RetrievalTargetType::Relationship,
                    _ => {
                        return Err(CoreError::ValidationFailed {
                            reason: "unknown source ownership kind".into(),
                        });
                    }
                };
                access.restrict(kind.clone(), &id);
                if attempt == "legacy:unverified" {
                    continue;
                }
                let m = manifests
                    .get(&key)
                    .ok_or_else(|| CoreError::ProviderUnavailable {
                        provider: "source ownership manifest unavailable".into(),
                    })?;
                if m.state == CodeSourceState::Active
                    && m.attempt_id == attempt
                    && engram_knowledge::code_source_retrieval_authorized(
                        m,
                        &request.requester,
                        &request.scope,
                        now,
                    )
                    && !(kind == RetrievalTargetType::Chunk
                        && m.snapshot_ids.iter().any(|c| c.as_str() == id))
                {
                    access.permit(kind, &id, m.policy.clone());
                }
            }
        }
        tx.commit().map_err(sql_error)?;
        Ok(access)
    }
}
