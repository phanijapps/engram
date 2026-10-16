//! Generic-document lease orchestration over neutral knowledge ports.
use crate::{IngestedKnowledge, content_hash, source_key::STABLE_SOURCE_KEY};
use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, DocumentHeadRepository, DocumentHeadWrite};
use std::sync::atomic::{AtomicU64, Ordering};
static ATTEMPT: AtomicU64 = AtomicU64::new(0);

fn attempt() -> String {
    format!(
        "doc:{}:{}:{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id(),
        ATTEMPT.fetch_add(1, Ordering::Relaxed)
    )
}
pub(crate) fn key(source: &KnowledgeSource, document: &SourceDocument) -> DocumentHeadKey {
    DocumentHeadKey {
        scope: source.scope.clone(),
        repository: source
            .metadata
            .as_ref()
            .and_then(|m| m.get(STABLE_SOURCE_KEY))
            .and_then(|v| v.as_str())
            .unwrap_or(&source.name)
            .into(),
        source_key: document
            .path
            .as_deref()
            .or(document.uri.as_deref())
            .unwrap_or(&source.name)
            .into(),
    }
}
pub(crate) async fn stage(
    port: &dyn DocumentHeadRepository,
    prepared: &IngestedKnowledge,
) -> CoreResult<DocumentHead> {
    let key = key(&prepared.source, &prepared.document);
    let prior = port.get_document_head(&key).await?;
    let head = DocumentHead {
        generation: content_hash(
            serde_json::to_vec(&(&key, &prepared.document.id, &prepared.document.content_hash))
                .map_err(|e| CoreError::InvalidRequest {
                    reason: e.to_string(),
                })?,
        ),
        key,
        attempt_id: attempt(),
        state: CodeSourceState::Updating,
        source_hash: prepared.document.content_hash.clone(),
        document_id: Some(prepared.document.id.clone()),
        chunk_ids: prepared.chunks.iter().map(|c| c.id.clone()).collect(),
        policy: prepared.source.policy.clone(),
        provenance: prepared.source.provenance.clone(),
    };
    port.begin_document_head(head.clone(), prior.as_ref().map(|h| h.attempt_id.as_str()))
        .await?;
    let result = async {
        port.write_document_record(
            &head.key,
            &head.attempt_id,
            DocumentHeadWrite::Source(prepared.source.clone()),
        )
        .await?;
        port.write_document_record(
            &head.key,
            &head.attempt_id,
            DocumentHeadWrite::Document(prepared.document.clone()),
        )
        .await?;
        for chunk in &prepared.chunks {
            port.write_document_record(
                &head.key,
                &head.attempt_id,
                DocumentHeadWrite::Chunk(Box::new(chunk.clone())),
            )
            .await?;
        }
        Ok::<(), CoreError>(())
    }
    .await;
    if let Err(error) = result {
        let _ = port.fail_document_head(&head.key, &head.attempt_id).await;
        return Err(error);
    }
    Ok(head)
}
pub(crate) async fn persist(
    port: &dyn DocumentHeadRepository,
    prepared: IngestedKnowledge,
) -> CoreResult<IngestedKnowledge> {
    let mut head = stage(port, &prepared).await?;
    head.state = CodeSourceState::Active;
    if let Err(error) = port.activate_document_head(head.clone()).await {
        let _ = port.fail_document_head(&head.key, &head.attempt_id).await;
        return Err(error);
    }
    Ok(prepared)
}
pub(crate) async fn current(
    port: &dyn DocumentHeadRepository,
    key: &DocumentHeadKey,
    hash: &str,
    policy: &Policy,
) -> CoreResult<bool> {
    Ok(port.get_document_head(key).await?.is_some_and(|h| {
        h.state == CodeSourceState::Active && h.source_hash == hash && &h.policy == policy
    }))
}

pub(crate) async fn enrichment(
    port: &dyn DocumentHeadRepository,
    source: &KnowledgeSource,
    document: &SourceDocument,
    chunks: &[KnowledgeChunk],
) -> CoreResult<DocumentHead> {
    let identity = key(source, document);
    let current = port
        .get_document_head(&identity)
        .await?
        .ok_or_else(|| CoreError::Conflict {
            reason: "document enrichment needs verified current inventory".into(),
        })?;
    if current.state != CodeSourceState::Active
        || current.document_id.as_ref() != Some(&document.id)
        || current.source_hash != document.content_hash
        || current.policy != source.policy
        || current.provenance != source.provenance
        || current.chunk_ids != chunks.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
    {
        return Err(CoreError::Conflict {
            reason: "document enrichment source is no longer current".into(),
        });
    }
    stage(
        port,
        &IngestedKnowledge {
            source: source.clone(),
            document: document.clone(),
            chunks: chunks.to_vec(),
        },
    )
    .await
}
pub(crate) async fn exclude(
    port: &dyn DocumentHeadRepository,
    key: &DocumentHeadKey,
    policy: Option<&Policy>,
    actor: &Actor,
) -> CoreResult<()> {
    if let Some(mut h) = port.get_document_head(key).await?
        && h.state == CodeSourceState::Active
        && policy.is_none_or(|p| p != &h.policy)
    {
        let prior = h.attempt_id.clone();
        h.attempt_id = attempt();
        h.state = CodeSourceState::Updating;
        if let Some(policy) = policy {
            h.policy = policy.clone();
            h.provenance.actor = actor.clone();
        }
        port.begin_document_head(h.clone(), Some(&prior)).await?;
        port.fail_document_head(key, &h.attempt_id).await?;
    }
    Ok(())
}

/// Fence a verified file refresh before cleanup can mutate legacy contributions.
/// Empty inventory is deliberately unavailable; only later staged input can activate.
pub(crate) async fn fence_refresh(
    port: &dyn DocumentHeadRepository,
    key: &DocumentHeadKey,
    source_hash: &str,
    policy: &Policy,
    actor: &Actor,
    source_name: &str,
) -> CoreResult<()> {
    let prior = port.get_document_head(key).await?;
    let head = DocumentHead {
        key: key.clone(),
        generation: content_hash(
            serde_json::to_vec(&(key, source_hash, "refresh-v1")).map_err(|e| {
                CoreError::InvalidRequest {
                    reason: e.to_string(),
                }
            })?,
        ),
        attempt_id: attempt(),
        state: CodeSourceState::Updating,
        source_hash: source_hash.into(),
        document_id: None,
        chunk_ids: vec![],
        policy: policy.clone(),
        provenance: Provenance {
            source: source_name.into(),
            actor: actor.clone(),
            observed_at: chrono::Utc::now(),
            evidence: vec![],
            derivations: vec![],
            confidence: None,
            method: Some("document_refresh".into()),
        },
    };
    port.begin_document_head(head.clone(), prior.as_ref().map(|h| h.attempt_id.as_str()))
        .await?;
    port.fail_document_head(key, &head.attempt_id).await
}
