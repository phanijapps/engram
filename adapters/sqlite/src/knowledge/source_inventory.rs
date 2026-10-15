//! Validates a staged source inventory using only this knowledge cell.
use super::schema::{json_error, sql_error};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub(super) fn invalid(reason: &str) -> CoreError {
    CoreError::ValidationFailed {
        reason: reason.into(),
    }
}

pub(super) fn record_kind(kind: CodeSourceRecordKind) -> &'static str {
    match kind {
        CodeSourceRecordKind::UnresolvedReference => "unresolved_reference",
        CodeSourceRecordKind::Source => "source",
        CodeSourceRecordKind::Chunk => "chunk",
        CodeSourceRecordKind::Entity => "entity",
        CodeSourceRecordKind::Relationship => "relationship",
        CodeSourceRecordKind::Graph => "graph",
        CodeSourceRecordKind::Document => "document",
    }
}
pub(super) fn table(kind: CodeSourceRecordKind) -> &'static str {
    match kind {
        CodeSourceRecordKind::UnresolvedReference => "knowledge_unresolved_refs_records",
        CodeSourceRecordKind::Source => "knowledge_sources",
        CodeSourceRecordKind::Chunk => "knowledge_chunks_records",
        CodeSourceRecordKind::Entity => "knowledge_entities_records",
        CodeSourceRecordKind::Relationship => "knowledge_relationships_records",
        CodeSourceRecordKind::Graph => "knowledge_graphs_records",
        CodeSourceRecordKind::Document => "knowledge_documents",
    }
}

pub(super) fn validate_inventory(
    conn: &Connection,
    m: &CodeSourceGeneration,
    key: &str,
) -> CoreResult<String> {
    // Historical attempts remain persisted. Only the current fenced attempt
    // contributes to activation and retrieval eligibility.
    if m.snapshot_ids.is_empty() {
        return Err(invalid("source snapshot inventory is empty"));
    }
    let document_id = m
        .document_id
        .as_ref()
        .ok_or_else(|| invalid("source document is missing"))?;
    if !m
        .records
        .iter()
        .any(|r| r.kind == CodeSourceRecordKind::Document && r.id == document_id.as_str())
    {
        return Err(invalid("source document is absent from inventory"));
    }
    let document_json: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_documents WHERE id=?1",
            [document_id.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let document: SourceDocument =
        serde_json::from_str(&document_json.ok_or_else(|| invalid("source document is missing"))?)
            .map_err(json_error)?;
    let source_json: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_sources WHERE id=?1",
            [document.source_id.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let source: KnowledgeSource =
        serde_json::from_str(&source_json.ok_or_else(|| invalid("owning source is missing"))?)
            .map_err(json_error)?;
    if source
        .metadata
        .as_ref()
        .and_then(|v| v.get("stableSourceKey"))
        .and_then(|v| v.as_str())
        != Some(m.file.repository.as_str())
        || source.scope != m.file.scope
        || source.policy != m.policy
        || document.policy != m.policy
        || document.kind != SourceDocumentKind::Code
        || document.content_hash != m.source_hash
        || document.path.as_deref() != Some(&m.file.path)
    {
        return Err(invalid(
            "source document or owner scope/policy does not match manifest",
        ));
    }
    for record in &m.records {
        let staged:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE file_key=?1 AND attempt_id=?2 AND kind=?3 AND record_id=?4)",rusqlite::params![key,m.attempt_id,record_kind(record.kind),record.id],|r|r.get(0)).map_err(sql_error)?;
        let exists: bool = conn
            .query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM {} WHERE id=?1)",
                    table(record.kind)
                ),
                [&record.id],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        if !staged || !exists {
            return Err(invalid(
                "source inventory contains missing or unstaged records",
            ));
        }
    }
    let mut stmt=conn.prepare("SELECT kind,record_id FROM code_source_contributions WHERE file_key=?1 AND attempt_id=?2").map_err(sql_error)?;
    let staged = stmt
        .query_map(rusqlite::params![key, m.attempt_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql_error)?;
    for entry in staged {
        let (kind, id) = entry.map_err(sql_error)?;
        if !m
            .records
            .iter()
            .any(|r| record_kind(r.kind) == kind && r.id == id)
        {
            return Err(invalid("activation omitted staged source records"));
        }
    }
    let mut text = String::new();
    let mut offset = 0;
    let mut seen = std::collections::HashSet::new();
    for id in &m.snapshot_ids {
        if !seen.insert(id.as_str()) {
            return Err(invalid("duplicate source snapshot"));
        }
        if !m
            .records
            .iter()
            .any(|r| r.kind == CodeSourceRecordKind::Chunk && r.id == id.as_str())
        {
            return Err(invalid("snapshot is absent from source inventory"));
        }
        let json: Option<String> = conn
            .query_row(
                "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let chunk: KnowledgeChunk =
            serde_json::from_str(&json.ok_or_else(|| invalid("snapshot is missing"))?)
                .map_err(json_error)?;
        let code = chunk
            .code
            .as_ref()
            .ok_or_else(|| invalid("snapshot has no typed source coverage"))?;
        if code.role != CodeChunkRole::SourceSnapshot
            || code.file != m.file
            || code.generation != m.generation
            || code.source_hash != m.source_hash
            || code.fingerprint != m.fingerprint
            || !code.complete
            || code.ranges.len() != 1
            || code.ranges[0].start != offset
            || code.ranges[0].end != offset + chunk.text.len() as u64
            || chunk.policy != m.policy
            || chunk.provenance.actor != m.provenance.actor
            || chunk.document_id.as_str()
                != m.document_id.as_ref().map(|d| d.as_str()).unwrap_or("")
        {
            return Err(invalid(
                "snapshot coverage, identity or policy does not match manifest",
            ));
        }
        let digest = digest(&chunk.text);
        if digest != chunk.content_hash {
            return Err(invalid("snapshot chunk digest mismatch"));
        }
        offset = code.ranges[0].end;
        text.push_str(&chunk.text);
    }
    if offset != m.byte_length || digest(&text) != m.source_hash {
        return Err(invalid("source snapshot partition or digest mismatch"));
    }
    let mut descriptors = Vec::new();
    for record in m
        .records
        .iter()
        .filter(|r| r.kind == CodeSourceRecordKind::Chunk)
    {
        let json: String = conn
            .query_row(
                "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
                [&record.id],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        let chunk: KnowledgeChunk = serde_json::from_str(&json).map_err(json_error)?;
        let code = chunk
            .code
            .as_ref()
            .ok_or_else(|| invalid("source chunk lacks coverage"))?;
        if code.ranges.is_empty()
            || code.file != m.file
            || code.generation != m.generation
            || code.source_hash != m.source_hash
            || code.fingerprint != m.fingerprint
            || chunk.document_id != *document_id
            || chunk.policy != m.policy
            || chunk.provenance.actor != m.provenance.actor
        {
            return Err(invalid("source chunk identity or policy does not match"));
        }
        descriptors.push(code.clone());
        let mut slices = Vec::new();
        let mut last_end = 0;
        for range in &code.ranges {
            if range.start < last_end || range.end < range.start {
                return Err(invalid("source slices are unordered or overlap"));
            }
            let slice = text
                .get(range.start as usize..range.end as usize)
                .ok_or_else(|| invalid("source slice is outside UTF-8 boundaries"))?;
            slices.push(slice);
            last_end = range.end;
        }
        if let Some(span) = code.declaration_range {
            if text.get(span.start as usize..span.end as usize).is_none()
                || code
                    .ranges
                    .iter()
                    .any(|r| r.start < span.start || r.end > span.end)
            {
                return Err(invalid("declaration span does not contain source coverage"));
            }
        }
        if slices.join(&code.slice_separator) != chunk.text
            || digest(&chunk.text) != chunk.content_hash
        {
            return Err(invalid(
                "rendered source slices or digest do not match chunk text",
            ));
        }
    }
    engram_knowledge::validate_code_unit_hierarchy(&descriptors)?;
    Ok(text)
}

fn digest(text: &str) -> String {
    let hex: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}")
}
