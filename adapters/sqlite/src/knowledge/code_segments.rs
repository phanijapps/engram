//! Derived representation lifecycle; storage transactions stay in this cell.
use super::{
    SqlKnowledgeStore, code_segments_state as state, code_segments_validation as validation,
    schema::{json_error, sql_error},
};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{
    CodeSegmentLease, CodeSegmentRepository, CodeSegmentSet, ResolvedCodeSegment,
    code_source_retrieval_authorized,
};
use engram_runtime::CoreResult;
use rusqlite::{OptionalExtension, TransactionBehavior, params};

#[async_trait]
impl CodeSegmentRepository for SqlKnowledgeStore {
    async fn code_index_revision(&self) -> CoreResult<u64> {
        self.lock()?
            .query_row(
                "SELECT revision FROM code_index_revision WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(sql_error)
    }
    async fn code_segment_inventory(
        &self,
        file: &CodeFileKey,
        representation: &str,
    ) -> CoreResult<Option<CodeSegmentLease>> {
        let conn = self.lock()?;
        state::load(&conn, file, representation)
    }
    async fn begin_code_segments(
        &self,
        inventory: CodeSegmentInventory,
        expected: Option<&CodeSegmentLease>,
    ) -> CoreResult<CodeSegmentLease> {
        if inventory.state != CodeSourceState::Updating
            || inventory.representation_id.is_empty()
            || inventory.segment_ids.len() > CodeSegmentInventory::MAX_SEGMENTS
            || inventory
                .segment_ids
                .iter()
                .enumerate()
                .any(|(i, id)| id.is_empty() || inventory.segment_ids[..i].contains(id))
        {
            return Err(state::invalid("invalid updating segment inventory"));
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        state::require_source(&tx, &inventory)?;
        if state::load(&tx, &inventory.file, &inventory.representation_id)?.as_ref() != expected {
            return Err(state::conflict("segment inventory compare-and-set failed"));
        }
        let revision = tx.query_row("UPDATE code_index_revision SET revision=revision+1 WHERE singleton=1 RETURNING revision", [], |r| r.get(0)).map_err(sql_error)?;
        let lease = CodeSegmentLease {
            inventory,
            revision,
        };
        state::save(&tx, &lease)?;
        tx.commit().map_err(sql_error)?;
        Ok(lease)
    }
    async fn write_code_segments(
        &self,
        lease: &CodeSegmentLease,
        segments: Vec<CodeEmbeddingSegment>,
    ) -> CoreResult<()> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let m = state::lease(&tx, lease)?;
        let inventory = &lease.inventory;
        let key = super::code_source::file_key(&inventory.file)?;
        for s in segments {
            validation::segment(&tx, &m, inventory, &s)?;
            let prior: Option<String> = tx
                .query_row(
                    "SELECT record_json FROM code_embedding_segments WHERE id=?1",
                    [&s.id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            if let Some(raw) = prior {
                let mut prior: CodeEmbeddingSegment =
                    serde_json::from_str(&raw).map_err(json_error)?;
                // A graph-only healed source attempt may reuse identical model
                // input. Every content/representation identity stays immutable.
                prior.attempt_id = s.attempt_id.clone();
                if prior != s {
                    return Err(state::conflict(
                        "segment ID would overwrite another content or representation owner",
                    ));
                }
            }
            tx.execute("INSERT INTO code_embedding_segments(id,file_key,representation_id,record_json) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET record_json=excluded.record_json", params![s.id,key,s.representation_id,serde_json::to_string(&s).map_err(json_error)?]).map_err(sql_error)?;
        }
        tx.execute(
            "UPDATE code_index_revision SET revision=revision+1 WHERE singleton=1",
            [],
        )
        .map_err(sql_error)?;
        tx.commit().map_err(sql_error)
    }
    async fn activate_code_segments(&self, lease: &CodeSegmentLease) -> CoreResult<()> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let m = state::lease(&tx, lease)?;
        let inventory = &lease.inventory;
        let segments = state::segments(&tx, inventory)?
            .ok_or_else(|| state::invalid("declared segment record is missing"))?;
        validation::complete(&tx, &m, inventory, &segments)?;
        let mut active = lease.clone();
        active.inventory.state = CodeSourceState::Active;
        state::save(&tx, &active)?;
        tx.commit().map_err(sql_error)
    }
    async fn fail_code_segments(&self, lease: &CodeSegmentLease) -> CoreResult<()> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let inv = &lease.inventory;
        if inv.state != CodeSourceState::Updating
            || state::load(&tx, &inv.file, &inv.representation_id)?.as_ref() != Some(lease)
        {
            return Err(state::conflict(
                "failed segment inventory has been superseded",
            ));
        }
        let mut failed = lease.clone();
        failed.inventory.state = CodeSourceState::Incomplete;
        state::save(&tx, &failed)?;
        tx.commit().map_err(sql_error)
    }
    async fn load_current_code_segments(
        &self,
        file: &CodeFileKey,
        representation: &str,
        requester: &Requester,
    ) -> CoreResult<Option<CodeSegmentSet>> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(sql_error)?;
        let Some(m) = state::source(&tx, file)? else {
            return Ok(None);
        };
        if !code_source_retrieval_authorized(&m, requester, &file.scope, chrono::Utc::now()) {
            return Ok(None);
        }
        let Some(lease) = state::load(&tx, file, representation)? else {
            return Ok(None);
        };
        let inv = &lease.inventory;
        if inv.state == CodeSourceState::Incomplete
            || inv.generation != m.generation
            || inv.attempt_id != m.attempt_id
        {
            return Ok(None);
        }
        let Some(segments) = state::segments(&tx, inv)? else {
            return Ok(None);
        };
        for s in &segments {
            validation::segment(&tx, &m, inv, s)?;
        }
        Ok(Some(CodeSegmentSet { lease, segments }))
    }
    async fn resolve_code_segment(
        &self,
        id: &str,
        scope: &Scope,
        requester: &Requester,
        representation: &str,
    ) -> CoreResult<Option<ResolvedCodeSegment>> {
        resolve(self, id, scope, requester, representation, true)
    }
    async fn prepare_code_segment(
        &self,
        id: &str,
        scope: &Scope,
        requester: &Requester,
        representation: &str,
    ) -> CoreResult<Option<ResolvedCodeSegment>> {
        resolve(self, id, scope, requester, representation, false)
    }
}

fn resolve(
    store: &SqlKnowledgeStore,
    id: &str,
    scope: &Scope,
    requester: &Requester,
    representation: &str,
    active_only: bool,
) -> CoreResult<Option<ResolvedCodeSegment>> {
    let mut conn = store.lock()?;
    let tx = conn.transaction().map_err(sql_error)?;
    let raw: Option<String> = tx
        .query_row(
            "SELECT record_json FROM code_embedding_segments WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let s: CodeEmbeddingSegment = serde_json::from_str(&raw).map_err(json_error)?;
    if s.id != id || &s.file.scope != scope || s.representation_id != representation {
        return Ok(None);
    }
    let Some(m) = state::source(&tx, &s.file)? else {
        return Ok(None);
    };
    if !code_source_retrieval_authorized(&m, requester, scope, chrono::Utc::now()) {
        return Ok(None);
    }
    let Some(lease) = state::load(&tx, &s.file, representation)? else {
        return Ok(None);
    };
    let inv = &lease.inventory;
    if inv.state == CodeSourceState::Incomplete
        || (active_only && inv.state != CodeSourceState::Active)
        || inv.generation != m.generation
        || inv.attempt_id != m.attempt_id
        || !inv.segment_ids.iter().any(|v| v == id)
    {
        return Ok(None);
    }
    let parent = validation::segment(&tx, &m, inv, &s)?;
    Ok(Some(ResolvedCodeSegment { segment: s, parent }))
}
