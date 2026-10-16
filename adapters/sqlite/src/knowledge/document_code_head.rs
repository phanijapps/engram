//! Mirrors verified code-source authority into the unified document head cell.
use super::{document_head_state as state, schema::sql_error, source_inventory::record_kind};
use engram_domain::*;
use engram_runtime::CoreResult;
use rusqlite::{Connection, params};

pub(super) fn head(m: &CodeSourceGeneration) -> DocumentHead {
    DocumentHead {
        key: DocumentHeadKey {
            scope: m.file.scope.clone(),
            repository: m.file.repository.clone(),
            source_key: m.file.path.clone(),
        },
        generation: m.generation.clone(),
        attempt_id: m.attempt_id.clone(),
        state: m.state,
        source_hash: m.source_hash.clone(),
        document_id: m.document_id.clone(),
        chunk_ids: m
            .records
            .iter()
            .filter(|r| r.kind == CodeSourceRecordKind::Chunk)
            .map(|r| Id::from(r.id.clone()))
            .collect(),
        policy: m.policy.clone(),
        provenance: m.provenance.clone(),
    }
}
pub(super) fn begin(
    conn: &Connection,
    m: &CodeSourceGeneration,
    expected: Option<&str>,
) -> CoreResult<()> {
    let h = head(m);
    let key = state::key(&h.key)?;
    if let Some(prior) = state::load(conn, &key)? {
        if expected.is_some_and(|v| v != prior.attempt_id)
            || (expected.is_none() && prior.state == CodeSourceState::Updating)
        {
            return Err(state::conflict(
                "document head and code-source lease differ",
            ));
        }
    }
    let reused:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM document_head_attempts WHERE head_key=?1 AND attempt_id=?2)",params![key,h.attempt_id],|r|r.get(0)).map_err(sql_error)?;
    if reused {
        return Err(state::conflict(
            "code document head attempt was already used",
        ));
    }
    conn.execute(
        "INSERT INTO document_head_attempts(head_key,attempt_id) VALUES(?1,?2)",
        params![key, h.attempt_id],
    )
    .map_err(sql_error)?;
    super::document_head_legacy::attribute(conn, &key, &h.key)?;
    state::save(conn, &key, &h)
}
pub(super) fn updating_lease(conn: &Connection, m: &CodeSourceGeneration) -> CoreResult<()> {
    let h = head(m);
    let key = state::key(&h.key)?;
    state::lease(conn, &key, &m.attempt_id)?;
    Ok(())
}
pub(super) fn synchronize(conn: &Connection, m: &CodeSourceGeneration) -> CoreResult<()> {
    let h = head(m);
    let key = state::key(&h.key)?;
    state::lease(conn, &key, &m.attempt_id)?;
    for record in &m.records {
        if matches!(
            record.kind,
            CodeSourceRecordKind::Source
                | CodeSourceRecordKind::Document
                | CodeSourceRecordKind::Chunk
        ) {
            let foreign:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM document_head_records WHERE kind=?1 AND record_id=?2 AND head_key!=?3)",params![record_kind(record.kind),record.id,key],|r|r.get(0)).map_err(sql_error)?;
            if foreign {
                return Err(state::conflict(
                    "code canonical record belongs to another document head",
                ));
            }
            conn.execute("INSERT OR IGNORE INTO document_head_records(head_key,attempt_id,kind,record_id) VALUES(?1,?2,?3,?4)",params![key,m.attempt_id,record_kind(record.kind),record.id]).map_err(sql_error)?;
        }
    }
    state::save(conn, &key, &h)
}
