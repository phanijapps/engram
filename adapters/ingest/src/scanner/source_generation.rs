//! Per-file lease coordination, separate from parser/candidate assembly.
use super::{ScanOptions, source_assembly::FINGERPRINT};
use crate::{IngestedKnowledge, content_hash};
use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{CodeSourceRepository, CodeSourceWrite, CoreError, CoreResult};
use std::sync::atomic::{AtomicU64, Ordering};
static ATTEMPT_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn file_key(opts: &ScanOptions, repository: &str, path: &str) -> CodeFileKey {
    CodeFileKey {
        scope: opts.scope.clone(),
        repository: repository.into(),
        path: path.into(),
    }
}
pub(super) async fn is_current(
    port: &dyn CodeSourceRepository,
    file: &CodeFileKey,
    hash: &str,
    opts: &ScanOptions,
    heads: Option<&dyn engram_knowledge::DocumentHeadRepository>,
) -> CoreResult<bool> {
    let Some(m) = port.get_code_source(file).await? else {
        return Ok(false);
    };
    let Some(heads) = heads else { return Ok(false) };
    let key = DocumentHeadKey {
        scope: file.scope.clone(),
        repository: file.repository.clone(),
        source_key: file.path.clone(),
    };
    let current = heads.get_document_head(&key).await?;
    Ok(m.state == CodeSourceState::Active
        && m.source_hash == hash
        && m.fingerprint == FINGERPRINT
        && m.policy == opts.policy
        && current.is_some_and(|h| {
            h.state == CodeSourceState::Active
                && h.generation == m.generation
                && h.attempt_id == m.attempt_id
                && h.document_id == m.document_id
        }))
}
/// An explicit policy change must exclude the old source even when admission
/// fails. Retain its physical inventory for retry cleanup; never activate it.
pub(super) async fn exclude_changed_policy(
    port: &dyn CodeSourceRepository,
    file: &CodeFileKey,
    opts: &ScanOptions,
) -> CoreResult<()> {
    if let Some(mut prior) = port.get_code_source(file).await? {
        if prior.policy != opts.policy && prior.state == CodeSourceState::Active {
            let expected = prior.attempt_id.clone();
            prior.attempt_id = format!(
                "{}:{}:{}",
                Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                std::process::id(),
                ATTEMPT_COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            prior.state = CodeSourceState::Updating;
            prior.policy = opts.policy.clone();
            prior.provenance.actor = opts.actor.clone();
            port.begin_code_source(prior.clone(), Some(&expected))
                .await?;
            port.fail_code_source(file, &prior.attempt_id).await?;
        }
    }
    Ok(())
}
pub(super) async fn begin(
    port: &dyn CodeSourceRepository,
    file: CodeFileKey,
    hash: &str,
    byte_length: u64,
    opts: &ScanOptions,
) -> CoreResult<CodeSourceGeneration> {
    let prior = port.get_code_source(&file).await?;
    let now = Utc::now();
    let generation = content_hash(serde_json::to_vec(&(&file, hash, FINGERPRINT)).map_err(
        |e| CoreError::InvalidRequest {
            reason: e.to_string(),
        },
    )?);
    let manifest = CodeSourceGeneration {
        file,
        generation,
        attempt_id: format!(
            "{}:{}:{}",
            now.timestamp_nanos_opt().unwrap_or_default(),
            std::process::id(),
            ATTEMPT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ),
        source_hash: hash.into(),
        fingerprint: FINGERPRINT.into(),
        state: CodeSourceState::Updating,
        byte_length,
        document_id: None,
        snapshot_ids: vec![],
        records: vec![],
        policy: opts.policy.clone(),
        provenance: Provenance {
            source: opts.source_name.clone(),
            actor: opts.actor.clone(),
            observed_at: now,
            evidence: vec![],
            derivations: vec![],
            confidence: Some(1.0),
            method: Some("source_preservation_v1".into()),
        },
    };
    port.begin_code_source(
        manifest.clone(),
        prior.as_ref().map(|p| p.attempt_id.as_str()),
    )
    .await?;
    // Superseded contributions remain historical. Current authority filters
    // projections; a refresh never deletes canonical records for reclamation.
    Ok(manifest)
}
pub(super) async fn persist(
    port: &dyn CodeSourceRepository,
    m: &mut CodeSourceGeneration,
    prepared: &IngestedKnowledge,
) -> CoreResult<()> {
    let mut seen = std::collections::HashSet::new();
    m.records
        .retain(|r| seen.insert(format!("{:?}:{}", r.kind, r.id)));
    port.stage_code_records(&m.file, &m.attempt_id, &m.records)
        .await?;
    port.write_code_record(
        &m.file,
        &m.attempt_id,
        CodeSourceWrite::Source(prepared.source.clone()),
    )
    .await?;
    port.write_code_record(
        &m.file,
        &m.attempt_id,
        CodeSourceWrite::Document(prepared.document.clone()),
    )
    .await?;
    for chunk in &prepared.chunks {
        port.write_code_record(
            &m.file,
            &m.attempt_id,
            CodeSourceWrite::Chunk(chunk.clone()),
        )
        .await?;
    }
    Ok(())
}

/// Refresh derived graph facts without reopening filesystem source. The same
/// source generation is retained; a fresh attempt fences all graph mutations.
pub(super) async fn heal_reference(
    port: &dyn CodeSourceRepository,
    mut m: CodeSourceGeneration,
    relationship: KnowledgeRelationship,
    mut reference: UnresolvedReference,
) -> CoreResult<()> {
    let prior_attempt = m.attempt_id.clone();
    m.attempt_id = format!(
        "{}:{}:{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id(),
        ATTEMPT_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    m.state = CodeSourceState::Updating;
    let record = CodeSourceRecord {
        kind: CodeSourceRecordKind::Relationship,
        id: relationship.id.to_string(),
    };
    if !m.records.contains(&record) {
        m.records.push(record);
    }
    reference.status = UnresolvedReferenceStatus::Resolved;
    reference.updated_at = Some(Utc::now());
    port.begin_code_source(m.clone(), Some(&prior_attempt))
        .await?;
    let result = async {
        port.stage_code_records(&m.file, &m.attempt_id, &m.records)
            .await?;

        port.write_code_record(
            &m.file,
            &m.attempt_id,
            CodeSourceWrite::Relationship(relationship),
        )
        .await?;
        port.write_code_record(
            &m.file,
            &m.attempt_id,
            CodeSourceWrite::UnresolvedReference(reference),
        )
        .await?;
        m.state = CodeSourceState::Active;
        port.activate_code_source(m.clone()).await
    }
    .await;
    if result.is_err() {
        let _ = port.fail_code_source(&m.file, &m.attempt_id).await;
    }
    result
}
