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
/// Access decision bound to the sole target queried. It cannot accidentally
/// treat an omitted, unrelated record as unmanaged and therefore permitted.
pub struct SourceTargetAccess {
    target_id: String,
    inner: RecordAccess,
}
impl SourceTargetAccess {
    pub fn allows(&self, kind: RetrievalTargetType) -> bool {
        self.inner.allows(kind, &self.target_id)
    }
    pub fn policy(&self, kind: RetrievalTargetType) -> Option<&Policy> {
        self.inner.policy(kind, &self.target_id)
    }
}
impl SqlKnowledgeStore {
    pub fn source_record_access(&self, request: &RetrievalRequest) -> CoreResult<RecordAccess> {
        self.record_access(request, None)
    }

    /// Select all independent owners of one candidate, without rebuilding the
    /// full-store access map for every lexical hit. The decision is bound to
    /// this target and cannot authorize any other ID.
    pub fn source_record_access_for_target(
        &self,
        request: &RetrievalRequest,
        target_id: &str,
    ) -> CoreResult<SourceTargetAccess> {
        Ok(SourceTargetAccess {
            target_id: target_id.into(),
            inner: self.record_access(request, Some(target_id))?,
        })
    }

    fn record_access(
        &self,
        request: &RetrievalRequest,
        target_id: Option<&str>,
    ) -> CoreResult<RecordAccess> {
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
        let mut heads = HashMap::new();
        {
            let mut stmt = tx
                .prepare("SELECT head_key,record_json FROM document_heads")
                .map_err(sql_error)?;
            for row in stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(sql_error)?
            {
                let (key, json) = row.map_err(sql_error)?;
                heads.insert(
                    key,
                    serde_json::from_str::<DocumentHead>(&json).map_err(json_error)?,
                );
            }
        }
        let mut access = RecordAccess::default();
        let now = Utc::now();
        {
            let query = match target_id {
                Some(_) => {
                    "SELECT file_key,attempt_id,kind,record_id FROM code_source_contributions WHERE kind IN ('chunk','entity','relationship') AND record_id=?1 ORDER BY file_key,record_id"
                }
                None => {
                    "SELECT file_key,attempt_id,kind,record_id FROM code_source_contributions WHERE kind IN ('chunk','entity','relationship') ORDER BY file_key,record_id"
                }
            };
            let mut stmt = tx.prepare(query).map_err(sql_error)?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(target_id), |r| {
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
                let code_head = super::document_code_head::head(m);
                let head_key = super::document_head_state::key(&code_head.key)?;
                let head_current = heads.get(&head_key).is_some_and(|h| {
                    h.attempt_id == m.attempt_id
                        && h.generation == m.generation
                        && h.source_hash == m.source_hash
                        && engram_knowledge::document_head_retrieval_authorized(
                            h,
                            &request.scope,
                            &request.requester,
                            now,
                        )
                });
                if head_current
                    && m.state == CodeSourceState::Active
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
        {
            let query = match target_id {
                Some(_) => {
                    "SELECT head_key,attempt_id,record_id,kind FROM document_head_records r WHERE kind IN ('chunk','entity','relationship') AND record_id=?1 AND NOT EXISTS(SELECT 1 FROM code_source_contributions c WHERE c.kind=r.kind AND c.record_id=r.record_id)"
                }
                None => {
                    "SELECT head_key,attempt_id,record_id,kind FROM document_head_records r WHERE kind IN ('chunk','entity','relationship') AND NOT EXISTS(SELECT 1 FROM code_source_contributions c WHERE c.kind=r.kind AND c.record_id=r.record_id)"
                }
            };
            let mut stmt = tx.prepare(query).map_err(sql_error)?;
            for row in stmt
                .query_map(rusqlite::params_from_iter(target_id), |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .map_err(sql_error)?
            {
                let (key, attempt, id, kind) = row.map_err(sql_error)?;
                let kind = match kind.as_str() {
                    "chunk" => RetrievalTargetType::Chunk,
                    "entity" => RetrievalTargetType::Entity,
                    _ => RetrievalTargetType::Relationship,
                };
                access.restrict(kind.clone(), &id);
                if let Some(h) = heads.get(&key)
                    && h.attempt_id == attempt
                    && (kind != RetrievalTargetType::Chunk
                        || h.chunk_ids.iter().any(|v| v.as_str() == id))
                    && engram_knowledge::document_head_retrieval_authorized(
                        h,
                        &request.scope,
                        &request.requester,
                        now,
                    )
                {
                    access.permit(kind, &id, h.policy.clone());
                }
            }
        }
        tx.commit().map_err(sql_error)?;
        Ok(access)
    }
}
