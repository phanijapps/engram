//! Source import validation and fenced apply, using the existing knowledge port.
use crate::{ImportData, record_key_hash};
use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{CodeSourceWrite, KnowledgeRepository};
use engram_runtime::{CoreError, CoreResult};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static ATTEMPT: AtomicU64 = AtomicU64::new(0);
pub(super) fn identity() -> String {
    format!(
        "{}:{}:{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        ATTEMPT.fetch_add(1, Ordering::Relaxed)
    )
}
fn invalid(reason: &str) -> CoreError {
    CoreError::ValidationFailed {
        reason: reason.into(),
    }
}
fn payloads(b: &CodeSourceBundle) -> Vec<CodeSourceWrite> {
    let mut out = Vec::new();
    out.extend(b.sources.iter().cloned().map(CodeSourceWrite::Source));
    out.extend(b.documents.iter().cloned().map(CodeSourceWrite::Document));
    out.extend(b.chunks.iter().cloned().map(CodeSourceWrite::Chunk));
    out.extend(b.graphs.iter().cloned().map(CodeSourceWrite::Graph));
    out.extend(b.entities.iter().cloned().map(CodeSourceWrite::Entity));
    out.extend(
        b.relationships
            .iter()
            .cloned()
            .map(CodeSourceWrite::Relationship),
    );
    out.extend(
        b.unresolved_references
            .iter()
            .cloned()
            .map(CodeSourceWrite::UnresolvedReference),
    );
    out
}
pub(super) fn fingerprint(
    data: &ImportData,
    requester: &Requester,
    scope: &Scope,
    target: &str,
) -> CoreResult<String> {
    serde_json::to_string(&(data, requester, scope, target, "code-source-bundle-v1"))
        .map(|v| record_key_hash(&v))
        .map_err(|e| invalid(&e.to_string()))
}
pub(super) async fn validate(
    data: &ImportData,
    requester: &Requester,
    scope: &Scope,
    target: &Arc<dyn KnowledgeRepository>,
) -> CoreResult<()> {
    if ![
        "code:source:export",
        "code:source:write",
        "code:source:activate",
    ]
    .iter()
    .all(|permission| requester.permissions.iter().any(|p| p == permission))
    {
        return Err(CoreError::PolicyDenied {
            reason: "source export, target write and activation authority required".into(),
        });
    }
    let port = target
        .code_sources()
        .ok_or_else(|| CoreError::CapabilityUnsupported {
            capability: "code_source_import".into(),
            reason: "target source cell unsupported".into(),
        })?;
    let mut files = std::collections::HashSet::new();
    for b in &data.code_sources {
        let m = &b.manifest;
        if b.version != 1 {
            return Err(CoreError::CapabilityUnsupported {
                capability: "code_source_import".into(),
                reason: "bundle reader version unsupported".into(),
            });
        }
        if !m.file.is_valid() || &m.file.scope != scope {
            return Err(CoreError::PolicyDenied {
                reason: "code-source scope translation unsupported; exact target scope required"
                    .into(),
            });
        }
        engram_knowledge::authorize_code_source_access(
            &CodeSourceReadRequest {
                file: m.file.clone(),
                requester: requester.clone(),
                generation: Some(m.generation.clone()),
                max_bytes: m.byte_length.max(1),
            },
            scope,
            m,
            Utc::now(),
        )?;
        if m.state != CodeSourceState::Active
            || !files.insert(serde_json::to_string(&m.file).map_err(|e| invalid(&e.to_string()))?)
        {
            return Err(invalid("bundle is inactive or repeats a file"));
        }
        if port
            .get_code_source(&m.file)
            .await?
            .is_some_and(|prior| prior.policy != m.policy)
        {
            return Err(CoreError::PolicyDenied {
                reason: "target source policy collision".into(),
            });
        }
        if m.policy.expires_at.is_some_and(|v| v <= Utc::now()) {
            return Err(CoreError::PolicyDenied {
                reason: "source policy expired".into(),
            });
        }
        let staged = |kind: CodeSourceRecordKind, id: &str| {
            m.records.iter().any(|r| r.kind == kind && r.id == id)
        };
        for source in &b.sources {
            if source.scope != m.file.scope
                || source.policy != m.policy
                || source.provenance.actor != m.provenance.actor
            {
                return Err(invalid("bundle source scope/policy mismatch"));
            }
        }
        for graph in &b.graphs {
            let doc = graph
                .metadata
                .as_ref()
                .and_then(|v| v.get("documentId"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid("bundle graph document missing"))?;
            if graph.scope != m.file.scope
                || graph.policy != m.policy
                || graph.provenance.actor != m.provenance.actor
                || !staged(CodeSourceRecordKind::Document, doc)
                || graph
                    .metadata
                    .as_ref()
                    .and_then(|v| v.get("path"))
                    .and_then(|v| v.as_str())
                    != Some(m.file.path.as_str())
            {
                return Err(invalid("bundle graph identity/policy mismatch"));
            }
        }
        for entity in &b.entities {
            if entity.scope != m.file.scope
                || entity.provenance.actor != m.provenance.actor
                || entity
                    .graph_id
                    .as_ref()
                    .is_some_and(|g| !staged(CodeSourceRecordKind::Graph, g.as_str()))
            {
                return Err(invalid("bundle entity identity mismatch"));
            }
        }
        for relation in &b.relationships {
            if relation.scope != m.file.scope
                || relation.provenance.actor != m.provenance.actor
                || relation
                    .graph_id
                    .as_ref()
                    .is_some_and(|g| !staged(CodeSourceRecordKind::Graph, g.as_str()))
            {
                return Err(invalid("bundle relationship identity mismatch"));
            }
        }
        for reference in &b.unresolved_references {
            if reference.scope != m.file.scope
                || !staged(
                    CodeSourceRecordKind::Entity,
                    reference.from_entity_id.as_str(),
                )
                || reference
                    .graph_id
                    .as_ref()
                    .is_some_and(|g| !staged(CodeSourceRecordKind::Graph, g.as_str()))
            {
                return Err(invalid("bundle unresolved reference identity mismatch"));
            }
        }
        let roots: std::collections::HashSet<_> = b
            .relationships
            .iter()
            .filter(|r| r.predicate == "belongs_to")
            .map(|r| {
                r.object
                    .id
                    .as_ref()
                    .map(|v| v.as_str())
                    .ok_or_else(|| invalid("repository root reference missing"))
            })
            .collect::<CoreResult<_>>()?;
        if roots.len() != b.context_entities.len() {
            return Err(invalid("bundle repository context closure mismatch"));
        }
        for root in &b.context_entities {
            if !roots.contains(root.id.as_str())
                || root.id != engram_code::repository_entity_id(&m.file.scope, &m.file.repository)
                || root.kind != EntityKind::Repository
                || root.scope != m.file.scope
                || root.graph_id.is_some()
                || root
                    .metadata
                    .as_ref()
                    .and_then(|v| v.get("stableSourceKey"))
                    .and_then(|v| v.as_str())
                    != Some(m.file.repository.as_str())
            {
                return Err(invalid("bundle repository context identity mismatch"));
            }
            if let Some(existing) = target.get_entity(&root.id, scope).await?
                && (existing.kind != root.kind
                    || existing.scope != root.scope
                    || existing.name != root.name
                    || existing.graph_id.is_some()
                    || existing
                        .metadata
                        .as_ref()
                        .and_then(|v| v.get("stableSourceKey"))
                        != root
                            .metadata
                            .as_ref()
                            .and_then(|v| v.get("stableSourceKey")))
            {
                return Err(CoreError::Conflict {
                    reason: "target repository context collision".into(),
                });
            }
        }
        let records: Vec<_> = payloads(b).iter().map(|p| p.record()).collect();
        let keys: std::collections::HashSet<_> = records
            .iter()
            .map(|r| format!("{:?}:{}", r.kind, r.id))
            .collect();
        if keys.len() != records.len()
            || records.len() != m.records.len()
            || m.records.iter().any(|r| !records.contains(r))
        {
            return Err(invalid("bundle contribution inventory mismatch"));
        }
        let source = b
            .sources
            .iter()
            .find(|v| {
                v.scope == m.file.scope
                    && v.policy == m.policy
                    && v.metadata
                        .as_ref()
                        .and_then(|v| v.get("stableSourceKey"))
                        .and_then(|v| v.as_str())
                        == Some(m.file.repository.as_str())
            })
            .ok_or_else(|| invalid("bundle source identity mismatch"))?;
        let doc = b
            .documents
            .iter()
            .find(|v| Some(&v.id) == m.document_id.as_ref())
            .ok_or_else(|| invalid("bundle document missing"))?;
        if doc.source_id != source.id
            || doc.policy != m.policy
            || doc.kind != SourceDocumentKind::Code
            || doc.content_hash != m.source_hash
            || doc.path.as_deref() != Some(&m.file.path)
        {
            return Err(invalid("bundle document identity mismatch"));
        }
        let mut text = String::new();
        let mut end = 0;
        let mut snapshots = std::collections::HashSet::new();
        for id in &m.snapshot_ids {
            let c = b
                .chunks
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| invalid("bundle snapshot missing"))?;
            let code = c
                .code
                .as_ref()
                .ok_or_else(|| invalid("bundle snapshot coverage missing"))?;
            if !snapshots.insert(id.as_str())
                || code.role != CodeChunkRole::SourceSnapshot
                || !code.complete
                || code.ranges
                    != vec![CodeByteRange {
                        start: end,
                        end: end + c.text.len() as u64,
                    }]
                || !code.slice_separator.is_empty()
            {
                return Err(invalid("bundle snapshot partition mismatch"));
            }
            end += c.text.len() as u64;
            text.push_str(&c.text);
        }
        if snapshots.is_empty()
            || end != m.byte_length
            || format!("sha256:{}", record_key_hash(&text)) != m.source_hash
        {
            return Err(invalid("bundle source digest mismatch"));
        }
        engram_knowledge::validate_code_unit_hierarchy(
            b.chunks.iter().filter_map(|c| c.code.as_ref()),
        )?;
        for c in &b.chunks {
            let code = c
                .code
                .as_ref()
                .ok_or_else(|| invalid("bundle code descriptor missing"))?;
            if code.file != m.file
                || code.generation != m.generation
                || code.source_hash != m.source_hash
                || code.fingerprint != m.fingerprint
                || c.policy != m.policy
                || c.provenance.actor != m.provenance.actor
                || c.source_id != source.id
                || c.document_id != doc.id
            {
                return Err(invalid("bundle chunk identity/policy mismatch"));
            }
            if code.ranges.windows(2).any(|r| r[0].end > r[1].start)
                || code.declaration_range.is_some_and(|span| {
                    text.get(span.start as usize..span.end as usize).is_none()
                        || code
                            .ranges
                            .iter()
                            .any(|r| r.start < span.start || r.end > span.end)
                })
            {
                return Err(invalid("bundle declaration or ordered coverage mismatch"));
            }
            let slices: Option<Vec<_>> = code
                .ranges
                .iter()
                .map(|r| text.get(r.start as usize..r.end as usize))
                .collect();
            if code.ranges.is_empty()
                || slices
                    .ok_or_else(|| invalid("bundle range is not UTF-8 source"))?
                    .join(&code.slice_separator)
                    != c.text
                || format!("sha256:{}", record_key_hash(&c.text)) != c.content_hash
            {
                return Err(invalid("bundle semantic coverage mismatch"));
            }
        }
    }
    Ok(())
}
pub(super) async fn apply(
    data: &ImportData,
    target: &Arc<dyn KnowledgeRepository>,
) -> CoreResult<()> {
    let port = target
        .code_sources()
        .ok_or_else(|| CoreError::CapabilityUnsupported {
            capability: "code_source_import".into(),
            reason: "target source cell unsupported".into(),
        })?;
    for b in &data.code_sources {
        for root in &b.context_entities {
            port.ensure_repository_context(&b.manifest.file, root.clone())
                .await?;
        }
        let prior = port.get_code_source(&b.manifest.file).await?;
        let mut m = b.manifest.clone();
        m.attempt_id = identity();
        m.state = CodeSourceState::Updating;
        port.begin_code_source(m.clone(), prior.as_ref().map(|p| p.attempt_id.as_str()))
            .await?;
        let result = async {
            port.stage_code_records(&m.file, &m.attempt_id, &m.records)
                .await?;
            for (owner, record) in port.prior_code_records(&m.file, &m.attempt_id).await? {
                port.remove_code_record(&m.file, &m.attempt_id, &owner, &record)
                    .await?;
            }
            for payload in payloads(b) {
                port.write_code_record(&m.file, &m.attempt_id, payload)
                    .await?;
            }
            m.state = CodeSourceState::Active;
            port.activate_code_source(m.clone()).await
        }
        .await;
        if result.is_err() {
            let _ = port.fail_code_source(&m.file, &m.attempt_id).await;
        }
        result?;
    }
    Ok(())
}
