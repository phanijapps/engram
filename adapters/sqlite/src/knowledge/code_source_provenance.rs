//! Authorized indexed revision from current canonical document/source metadata.
use super::{
    SqlKnowledgeStore, code_segments_state,
    schema::{json_error, sql_error},
};
use engram_domain::*;
use engram_knowledge::{code_source_retrieval_authorized, source_retrieval_policy_authorized};
use engram_runtime::CoreResult;
use rusqlite::OptionalExtension;

pub(super) fn indexed_revision(
    store: &SqlKnowledgeStore,
    file: &CodeFileKey,
    scope: &Scope,
    requester: &Requester,
) -> CoreResult<Option<String>> {
    if file.scope != *scope {
        return Ok(None);
    }
    let mut conn = store.lock()?;
    let tx = conn.transaction().map_err(sql_error)?;
    let Some(manifest) = code_segments_state::source(&tx, file)? else {
        return Ok(None);
    };
    let now = chrono::Utc::now();
    if !code_source_retrieval_authorized(&manifest, requester, &file.scope, now) {
        return Ok(None);
    }
    let Some(document) = manifest.document_id.as_ref() else {
        return Ok(None);
    };
    let raw: Option<String> = tx
        .query_row(
            "SELECT record_json FROM knowledge_documents WHERE id=?1",
            [document.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let document: SourceDocument = serde_json::from_str(&raw).map_err(json_error)?;
    if !manifest
        .records
        .iter()
        .any(|r| r.kind == CodeSourceRecordKind::Source && r.id == document.source_id.as_str())
        || !source_retrieval_policy_authorized(
            &document.policy,
            &document.provenance.actor,
            requester,
            now,
        )
    {
        return Ok(None);
    }
    let raw: Option<String> = tx
        .query_row(
            "SELECT record_json FROM knowledge_sources WHERE id=?1",
            [document.source_id.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let source: KnowledgeSource = serde_json::from_str(&raw).map_err(json_error)?;
    if source.scope != file.scope
        || !source_retrieval_policy_authorized(
            &source.policy,
            &source.provenance.actor,
            requester,
            now,
        )
    {
        return Ok(None);
    }
    Ok(document
        .version
        .filter(|s| !s.is_empty())
        .or(source.version.filter(|s| !s.is_empty()))
        .or_else(|| {
            source
                .metadata
                .as_ref()
                .and_then(|m| m.get("revision"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        }))
}
