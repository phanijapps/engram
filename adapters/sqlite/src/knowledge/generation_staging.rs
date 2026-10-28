//! Scoped immutable contributor and exact-source descriptors for staged code records.
use super::{
    SqlKnowledgeStore,
    schema::{json_error, sql_error},
};
use async_trait::async_trait;
use engram_domain::{CodeLineage, IndexedSource, Scope};
use engram_knowledge::{CodeGenerationStaging, CoreError, CoreResult};
use rusqlite::{OptionalExtension, params};

fn invalid() -> CoreError {
    CoreError::InvalidRequest {
        reason: "conflicting or invalid generation stage record".into(),
    }
}
#[async_trait]
impl CodeGenerationStaging for SqlKnowledgeStore {
    async fn stage_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
        lineage: &CodeLineage,
    ) -> CoreResult<()> {
        lineage.validate().map_err(|_| invalid())?;
        if record_id.is_empty() || scope.tenant.is_empty() {
            return Err(invalid());
        }
        let json = serde_json::to_string(lineage).map_err(json_error)?;
        let key = serde_json::to_string(scope).map_err(json_error)?;
        let mut conn = self.lock_conn()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS code_stage_lineage(scope_key TEXT NOT NULL,record_id TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,record_json TEXT NOT NULL,PRIMARY KEY(scope_key,record_id));").map_err(sql_error)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT record_json FROM code_stage_lineage WHERE scope_key=?1 AND record_id=?2",
                params![key, record_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if old.as_ref().is_some_and(|old| old != &json) {
            return Err(invalid());
        }
        tx.execute("INSERT OR IGNORE INTO code_stage_lineage(scope_key,record_id,repo_id,generation_id,record_json) VALUES(?1,?2,?3,?4,?5)",params![key,record_id,lineage.repo_id.0,lineage.generation_id,json]).map_err(sql_error)?;
        tx.commit().map_err(sql_error)
    }
    async fn stage_source(&self, scope: &Scope, source: &IndexedSource) -> CoreResult<()> {
        source.validate().map_err(|_| invalid())?;
        if scope.tenant.is_empty() {
            return Err(invalid());
        }
        let json = serde_json::to_string(source).map_err(json_error)?;
        let key = serde_json::to_string(scope).map_err(json_error)?;
        let mut conn = self.lock_conn()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS code_stage_sources(scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,path TEXT NOT NULL,record_json TEXT NOT NULL,PRIMARY KEY(scope_key,repo_id,generation_id,path));").map_err(sql_error)?;
        let old: Option<String> = tx.query_row("SELECT record_json FROM code_stage_sources WHERE scope_key=?1 AND repo_id=?2 AND generation_id=?3 AND path=?4",params![key,source.repo_id.0,source.generation_id,source.path],|r|r.get(0)).optional().map_err(sql_error)?;
        if old.as_ref().is_some_and(|old| old != &json) {
            return Err(invalid());
        }
        tx.execute("INSERT OR IGNORE INTO code_stage_sources(scope_key,repo_id,generation_id,path,record_json) VALUES(?1,?2,?3,?4,?5)",params![key,source.repo_id.0,source.generation_id,source.path,json]).map_err(sql_error)?;
        tx.commit().map_err(sql_error)
    }
}
