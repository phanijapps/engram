//! Local authority lookups and derived-inventory state, inside caller transactions.
use super::{
    document_head_state,
    schema::{json_error, sql_error},
};
use engram_domain::*;
use engram_knowledge::CodeSegmentLease;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn conflict(reason: &str) -> CoreError {
    CoreError::Conflict {
        reason: reason.into(),
    }
}
pub(super) fn invalid(reason: &str) -> CoreError {
    CoreError::ValidationFailed {
        reason: reason.into(),
    }
}
pub(super) fn source(
    conn: &Connection,
    file: &CodeFileKey,
) -> CoreResult<Option<CodeSourceGeneration>> {
    let key = super::code_source::file_key(file)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT record_json FROM code_source_generations WHERE file_key=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let m: CodeSourceGeneration = serde_json::from_str(&raw).map_err(json_error)?;
    let head_key = document_head_state::key(&DocumentHeadKey {
        scope: file.scope.clone(),
        repository: file.repository.clone(),
        source_key: file.path.clone(),
    })?;
    let head = document_head_state::load(conn, &head_key)?;
    Ok((m.state == CodeSourceState::Active
        && m.file == *file
        && head.is_some_and(|h| {
            h.state == CodeSourceState::Active
                && h.attempt_id == m.attempt_id
                && h.generation == m.generation
                && h.source_hash == m.source_hash
                && h.document_id == m.document_id
                && h.policy == m.policy
        }))
    .then_some(m))
}
pub(super) fn require_source(
    conn: &Connection,
    inv: &CodeSegmentInventory,
) -> CoreResult<CodeSourceGeneration> {
    source(conn, &inv.file)?
        .filter(|m| m.generation == inv.generation && m.attempt_id == inv.attempt_id)
        .ok_or_else(|| conflict("segment inventory source attempt is not current and active"))
}
pub(super) fn load(
    conn: &Connection,
    file: &CodeFileKey,
    representation: &str,
) -> CoreResult<Option<CodeSegmentLease>> {
    let row: Option<(String, u64)> = conn.query_row(
        "SELECT record_json,lease_revision FROM code_segment_inventories WHERE file_key=?1 AND representation_id=?2",
        params![super::code_source::file_key(file)?, representation],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional().map_err(sql_error)?;
    row.map(|(raw, revision)| {
        Ok(CodeSegmentLease {
            inventory: serde_json::from_str(&raw).map_err(json_error)?,
            revision,
        })
    })
    .transpose()
}
pub(super) fn save(conn: &Connection, lease: &CodeSegmentLease) -> CoreResult<()> {
    let inv = &lease.inventory;
    conn.execute("INSERT INTO code_segment_inventories(file_key,representation_id,record_json,lease_revision) VALUES(?1,?2,?3,?4) ON CONFLICT(file_key,representation_id) DO UPDATE SET record_json=excluded.record_json,lease_revision=excluded.lease_revision", params![super::code_source::file_key(&inv.file)?,inv.representation_id,serde_json::to_string(inv).map_err(json_error)?,lease.revision]).map_err(sql_error)?;
    conn.execute(
        "UPDATE code_index_revision SET revision=revision+1 WHERE singleton=1",
        [],
    )
    .map_err(sql_error)?;
    Ok(())
}
pub(super) fn lease(
    conn: &Connection,
    expected: &CodeSegmentLease,
) -> CoreResult<CodeSourceGeneration> {
    let inv = &expected.inventory;
    if inv.state != CodeSourceState::Updating
        || load(conn, &inv.file, &inv.representation_id)?.as_ref() != Some(expected)
    {
        return Err(conflict(
            "segment inventory lease is superseded or unavailable",
        ));
    }
    require_source(conn, inv)
}
pub(super) fn segments(
    conn: &Connection,
    inv: &CodeSegmentInventory,
) -> CoreResult<Option<Vec<CodeEmbeddingSegment>>> {
    let mut result = Vec::with_capacity(inv.segment_ids.len());
    for id in &inv.segment_ids {
        let raw: Option<String> = conn.query_row("SELECT record_json FROM code_embedding_segments WHERE id=?1 AND file_key=?2 AND representation_id=?3", params![id,super::code_source::file_key(&inv.file)?,inv.representation_id], |r| r.get(0)).optional().map_err(sql_error)?;
        let Some(raw) = raw else {
            // A durable begin may outlive the process before its atomic write.
            // This set cannot be resumed; the caller must rederive through CAS.
            return Ok(None);
        };
        result.push(serde_json::from_str(&raw).map_err(json_error)?);
    }
    Ok(Some(result))
}
