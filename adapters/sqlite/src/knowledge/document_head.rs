//! Guarded current-document transitions; all transactions stay in this cell.
use super::{document_head_state as state, schema::sql_error, service::SqlKnowledgeStore};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{DocumentHeadRepository, DocumentHeadWrite};
use engram_runtime::CoreResult;
use rusqlite::{TransactionBehavior, params};

#[async_trait]
impl DocumentHeadRepository for SqlKnowledgeStore {
    async fn is_managed_chunk(&self, id: &ChunkId) -> CoreResult<bool> {
        self.lock()?.query_row("SELECT EXISTS(SELECT 1 FROM document_head_records WHERE kind='chunk' AND record_id=?1)", [id.as_str()], |r| r.get(0)).map_err(sql_error)
    }
    async fn get_document_head(&self, key: &DocumentHeadKey) -> CoreResult<Option<DocumentHead>> {
        let key = state::key(key)?;
        let conn = self.lock()?;
        state::load(&conn, &key)
    }
    async fn list_document_heads(
        &self,
        scope: &Scope,
        repository: &str,
    ) -> CoreResult<Vec<DocumentHead>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare("SELECT record_json FROM document_heads WHERE json_extract(record_json,'$.key.repository')=?1 ORDER BY head_key").map_err(sql_error)?;
        let mut result = Vec::new();
        for row in stmt
            .query_map([repository], |r| r.get::<_, String>(0))
            .map_err(sql_error)?
        {
            let h: DocumentHead = serde_json::from_str(&row.map_err(sql_error)?)
                .map_err(super::schema::json_error)?;
            if h.key.scope == *scope {
                result.push(h);
            }
        }
        Ok(result)
    }
    async fn begin_document_head(
        &self,
        head: DocumentHead,
        expected: Option<&str>,
    ) -> CoreResult<()> {
        let key = state::key(&head.key)?;
        if head.state != CodeSourceState::Updating
            || head.attempt_id.is_empty()
            || head.generation.is_empty()
            || head.source_hash.is_empty()
            || head
                .chunk_ids
                .iter()
                .enumerate()
                .any(|(n, id)| head.chunk_ids[..n].contains(id))
        {
            return Err(state::invalid("invalid updating document head"));
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let prior = state::load(&tx, &key)?;
        let code_key = serde_json::to_string(&CodeFileKey {
            scope: head.key.scope.clone(),
            repository: head.key.repository.clone(),
            path: head.key.source_key.clone(),
        })
        .map_err(super::schema::json_error)?;
        let code_owned: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM code_source_generations WHERE file_key=?1)",
                [&code_key],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        if code_owned {
            return Err(state::conflict(
                "code-source-owned head requires the code-source lease",
            ));
        }
        if prior.as_ref().map(|h| h.attempt_id.as_str()) != expected {
            return Err(state::conflict("document head compare-and-set failed"));
        }
        let reused:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM document_head_attempts WHERE head_key=?1 AND attempt_id=?2)",params![key,head.attempt_id],|r|r.get(0)).map_err(sql_error)?;
        if reused {
            return Err(state::conflict("document head attempt was already used"));
        }
        tx.execute(
            "INSERT INTO document_head_attempts(head_key,attempt_id) VALUES(?1,?2)",
            params![key, head.attempt_id],
        )
        .map_err(sql_error)?;
        super::document_head_legacy::attribute(&tx, &key, &head.key)?;
        state::save(&tx, &key, &head)?;
        tx.commit().map_err(sql_error)
    }
    async fn write_document_record(
        &self,
        key: &DocumentHeadKey,
        attempt: &str,
        payload: DocumentHeadWrite,
    ) -> CoreResult<()> {
        let key = state::key(key)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let h = state::lease(&tx, &key, attempt)?;
        super::document_head_records::write(&tx, &key, &h, payload)?;
        tx.commit().map_err(sql_error)
    }
    async fn activate_document_head(&self, head: DocumentHead) -> CoreResult<()> {
        if head.state != CodeSourceState::Active {
            return Err(state::invalid("activation requires active document head"));
        }
        let key = state::key(&head.key)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let mut lease = state::lease(&tx, &key, &head.attempt_id)?;
        lease.state = CodeSourceState::Active;
        if lease != head {
            return Err(state::conflict(
                "activation changed immutable document lease",
            ));
        }
        super::document_head_records::verify(&tx, &key, &head)?;
        state::save(&tx, &key, &head)?;
        tx.commit().map_err(sql_error)
    }
    async fn fail_document_head(&self, key: &DocumentHeadKey, attempt: &str) -> CoreResult<()> {
        let key = state::key(key)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let mut h = state::lease(&tx, &key, attempt)?;
        h.state = CodeSourceState::Incomplete;
        state::save(&tx, &key, &h)?;
        tx.commit().map_err(sql_error)
    }
    async fn current_document_for_chunk(
        &self,
        id: &ChunkId,
        scope: &Scope,
    ) -> CoreResult<Option<DocumentHead>> {
        let conn = self.lock()?;
        let mut query=conn.prepare("SELECT h.record_json FROM document_heads h JOIN document_head_records r ON r.head_key=h.head_key AND r.attempt_id=h.attempt_id WHERE h.state='active' AND r.kind='chunk' AND r.record_id=?1").map_err(sql_error)?;
        let rows = query
            .query_map([id.as_str()], |r| r.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut found = None;
        for row in rows {
            let h: DocumentHead = serde_json::from_str(&row.map_err(sql_error)?)
                .map_err(super::schema::json_error)?;
            if &h.key.scope == scope
                && h.chunk_ids.contains(id)
                && h.policy.allowed_uses.contains(&AllowedUse::Retrieval)
                && h.policy
                    .expires_at
                    .is_none_or(|expiry| expiry > chrono::Utc::now())
            {
                if found.is_some() {
                    return Err(state::conflict("ambiguous current document ownership"));
                }
                found = Some(h);
            }
        }
        Ok(found)
    }
}
