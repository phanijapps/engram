//! Current canonical parent and complete derivative range validation.
use super::{
    code_segments_state::{conflict, invalid},
    schema::{json_error, sql_error},
};
use engram_domain::*;
use engram_runtime::CoreResult;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

pub(super) fn parent(
    conn: &Connection,
    m: &CodeSourceGeneration,
    s: &CodeEmbeddingSegment,
) -> CoreResult<KnowledgeChunk> {
    if s.file != m.file
        || s.generation != m.generation
        || s.attempt_id != m.attempt_id
        || s.source_hash != m.source_hash
        || !m
            .records
            .iter()
            .any(|r| r.kind == CodeSourceRecordKind::Chunk && r.id == s.parent_chunk_id.as_str())
    {
        return Err(conflict(
            "segment parent does not belong to the current source attempt",
        ));
    }
    let raw: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
            [s.parent_chunk_id.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let p: KnowledgeChunk =
        serde_json::from_str(&raw.ok_or_else(|| invalid("segment canonical parent is missing"))?)
            .map_err(json_error)?;
    let c = p
        .code
        .as_ref()
        .ok_or_else(|| invalid("segment canonical parent has no code coverage"))?;
    if c.role == CodeChunkRole::SourceSnapshot
        || c.unit.as_deref() != Some(&s.parent_unit_id)
        || c.file != m.file
        || c.generation != m.generation
        || c.source_hash != m.source_hash
        || c.fingerprint != m.fingerprint
        || Some(&p.document_id) != m.document_id.as_ref()
    {
        return Err(invalid(
            "segment parent coverage is incompatible or a snapshot",
        ));
    }
    engram_knowledge::code_segment_text(&p, &s.ranges)?;
    Ok(p)
}
pub(super) fn segment(
    conn: &Connection,
    m: &CodeSourceGeneration,
    inv: &CodeSegmentInventory,
    s: &CodeEmbeddingSegment,
) -> CoreResult<KnowledgeChunk> {
    if s.ranges.len() > CodeEmbeddingSegment::MAX_RANGES
        || s.id.is_empty()
        || s.parent_unit_id.is_empty()
        || s.representation_id != inv.representation_id
        || !inv.segment_ids.contains(&s.id)
        || !s.embedding_input.is_valid()
        || s.embedding_input.query_tokens != 0
        || s.context.len() > 20
        || (s.context.is_empty() && s.embedding_input.context_tokens != 0)
    {
        return Err(invalid("segment identity/token coverage is invalid"));
    }
    let primary = parent(conn, m, s)?;
    for fact in &s.context {
        if fact.sources.len() != 1 {
            return Err(invalid("context needs one current canonical contributor"));
        }
        let source = &fact.sources[0];
        let id: Option<String> = conn.query_row(
            "SELECT id FROM knowledge_chunks_records WHERE json_extract(record_json,'$.code.unit')=?1 AND json_extract(record_json,'$.documentId')=?2",
            rusqlite::params![source.unit_id, m.document_id.as_ref().map(|id| id.as_str())], |row| row.get(0),
        ).optional().map_err(sql_error)?;
        let mut owner = s.clone();
        owner.parent_chunk_id = Id::from(
            id.ok_or_else(|| invalid("context contributor is missing from current source"))?,
        );
        owner.parent_unit_id = source.unit_id.clone();
        owner.ranges = vec![CodeByteRange {
            start: source.start_byte,
            end: source.end_byte,
        }];
        let donor = parent(conn, m, &owner)?;
        engram_knowledge::validate_code_context_fact(fact, &primary, &donor, &s.representation_id)?;
    }
    Ok(primary)
}
fn merged(mut ranges: Vec<CodeByteRange>) -> Vec<CodeByteRange> {
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut result: Vec<CodeByteRange> = Vec::new();
    for r in ranges {
        if let Some(last) = result.last_mut()
            && r.start <= last.end
        {
            last.end = last.end.max(r.end);
            continue;
        }
        result.push(r);
    }
    result
}
pub(super) fn complete(
    conn: &Connection,
    m: &CodeSourceGeneration,
    inv: &CodeSegmentInventory,
    segments: &[CodeEmbeddingSegment],
) -> CoreResult<()> {
    if segments.iter().map(|s| &s.id).ne(inv.segment_ids.iter()) {
        return Err(invalid(
            "segment inventory order or count differs from its records",
        ));
    }
    let order: HashMap<_, _> = m
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == CodeSourceRecordKind::Chunk)
        .map(|(i, r)| (r.id.as_str(), i))
        .collect();
    let mut cover: HashMap<String, Vec<CodeByteRange>> = HashMap::new();
    let mut prior = None;
    for s in segments {
        segment(conn, m, inv, s)?;
        let current = (
            *order
                .get(s.parent_chunk_id.as_str())
                .ok_or_else(|| invalid("segment parent order is unknown"))?,
            s.ranges
                .first()
                .ok_or_else(|| invalid("segment has no ranges"))?
                .start,
        );
        if prior.is_some_and(|p| p >= current) {
            return Err(invalid(
                "segment order is not canonical parent order followed by ascending source byte",
            ));
        }
        prior = Some(current);
        cover
            .entry(s.parent_chunk_id.to_string())
            .or_default()
            .extend_from_slice(&s.ranges);
    }
    for record in m.records.iter().filter(|r| {
        r.kind == CodeSourceRecordKind::Chunk
            && !m.snapshot_ids.iter().any(|id| id.as_str() == r.id)
    }) {
        let raw: String = conn
            .query_row(
                "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
                [&record.id],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        let parent: KnowledgeChunk = serde_json::from_str(&raw).map_err(json_error)?;
        if parent.text.is_empty() {
            continue;
        }
        let code = parent
            .code
            .as_ref()
            .ok_or_else(|| invalid("canonical parent coverage is missing"))?;
        let actual = cover.remove(&record.id).unwrap_or_default();
        if merged(actual) != merged(code.ranges.clone()) {
            return Err(invalid(
                "segment inventory does not completely cover every canonical parent",
            ));
        }
    }
    if !cover.is_empty() {
        return Err(invalid(
            "segment inventory has undeclared canonical parents",
        ));
    }
    Ok(())
}
