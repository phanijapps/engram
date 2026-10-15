//! Shared bounded retrieval embedding maintenance. Local code inference uses
//! current source authorization; external enrichment eligibility stays separate.

use std::collections::{BTreeMap, HashMap, HashSet};

use engram_domain::{
    CodeChunkRole, CodeSourceGeneration, CodeSourceRecordKind, CodeSourceState,
    EmbeddingBackfillReport, EmbeddingBackfillRequest, Id, KnowledgeChunkKind, SourceDocumentKind,
};
use engram_knowledge::code_source_retrieval_authorized;
use engram_runtime::{CoreError, CoreResult};

use crate::{ChunkRef, EngramProvider};

pub(crate) fn capability(
    embedder: Option<&dyn crate::EmbeddingProvider>,
    index: Option<&dyn engram_retrieval::VectorIndex>,
    query: bool,
    sources: bool,
) -> engram_domain::CapabilityState {
    if query
        && sources
        && embedder
            .zip(index)
            .is_some_and(|(e, i)| e.is_local() && &e.embedding_space() == i.embedding_space())
    {
        engram_domain::CapabilityState::Supported
    } else {
        engram_domain::CapabilityState::Unsupported {
            reason: engram_domain::CapabilityReason::ProviderUnavailable,
        }
    }
}

const INPUT_CHAR_LIMIT: usize = 8192;
const BATCH_SIZE: usize = 64;
type Sources = HashMap<(String, String), CodeSourceGeneration>;

async fn head_attempt(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
    reference: &ChunkRef,
) -> CoreResult<Option<Option<String>>> {
    let Some(heads) = provider.require_knowledge()?.document_heads() else {
        return Ok(if reference.code.is_none() {
            Some(None)
        } else {
            None
        });
    };
    if !heads.is_managed_chunk(&reference.id).await? {
        return Ok(if reference.code.is_none() {
            Some(None)
        } else {
            None
        });
    }
    Ok(heads
        .current_document_for_chunk(&reference.id, &request.scope)
        .await?
        .filter(|h| {
            engram_knowledge::document_head_retrieval_authorized(
                h,
                &request.scope,
                &request.requester,
                chrono::Utc::now(),
            )
        })
        .map(|h| Some(h.attempt_id)))
}

struct Prepared {
    reference: ChunkRef,
    text: String,
    attempt: Option<String>,
}

fn code_allowed(
    reference: &ChunkRef,
    manifest: &CodeSourceGeneration,
    request: &EmbeddingBackfillRequest,
) -> bool {
    let Some(code) = &reference.code else {
        return false;
    };
    code.role != CodeChunkRole::SourceSnapshot
        && code.file == manifest.file
        && code.generation == manifest.generation
        && code.source_hash == manifest.source_hash
        && code.fingerprint == manifest.fingerprint
        && manifest.state == CodeSourceState::Active
        && manifest
            .records
            .iter()
            .any(|r| r.kind == CodeSourceRecordKind::Chunk && r.id == reference.id.as_str())
        && code_source_retrieval_authorized(
            manifest,
            &request.requester,
            &request.scope,
            chrono::Utc::now(),
        )
}

async fn eligible_refs(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
) -> CoreResult<Vec<ChunkRef>> {
    let local = provider.require_embedding_provider()?.is_local();
    let refs = provider
        .require_knowledge_query()?
        .list_chunk_refs(&request.scope)
        .await?;
    let sources: Sources = if local && refs.iter().any(|r| r.code.is_some()) {
        if let Some(port) = provider.code_sources() {
            port.list_code_sources(&request.scope)
                .await?
                .into_iter()
                .map(|m| ((m.file.repository.clone(), m.file.path.clone()), m))
                .collect()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };
    let refs: Vec<_> = refs
        .into_iter()
        .filter(|r| {
            if !r.semantic_eligible || !r.has_text {
                return false;
            }
            match &r.code {
                Some(code) => {
                    local
                        && sources
                            .get(&(code.file.repository.clone(), code.file.path.clone()))
                            .is_some_and(|m| code_allowed(r, m, request))
                }
                None => r.model_eligible,
            }
        })
        .collect();
    let mut eligible = Vec::new();
    for reference in refs {
        if head_attempt(provider, request, &reference).await?.is_some() {
            eligible.push(reference);
        }
    }
    Ok(eligible)
}

// Resolve actual canonical content before every use. Never treat a source read
// or a backend-provided flag as permission to send raw code to an external model.
async fn prepare(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
    reference: &ChunkRef,
) -> CoreResult<Option<Prepared>> {
    let Some(head_attempt) = head_attempt(provider, request, reference).await? else {
        return Ok(None);
    };
    let Some(chunk) = provider
        .require_knowledge()?
        .get_chunk(&reference.id, &request.scope)
        .await?
    else {
        return Ok(None);
    };
    if chunk.code != reference.code
        || chunk.content_hash != reference.content_hash
        || chunk.text.trim().is_empty()
    {
        return Ok(None);
    }
    let attempt = if let Some(code) = &reference.code {
        if !provider.require_embedding_provider()?.is_local() {
            return Ok(None);
        }
        let Some(port) = provider.code_sources() else {
            return Ok(None);
        };
        let Some(manifest) = port.get_code_source(&code.file).await? else {
            return Ok(None);
        };
        if !code_allowed(reference, &manifest, request)
            || head_attempt.as_deref() != Some(manifest.attempt_id.as_str())
        {
            return Ok(None);
        }
        Some(manifest.attempt_id)
    } else {
        // Includes old code with unknown coverage; it must be rescanned first.
        if !reference.model_eligible
            || chunk.document_kind == Some(SourceDocumentKind::Code)
            || chunk.kind == KnowledgeChunkKind::CodeSymbol
            || (chunk.kind == KnowledgeChunkKind::CodeBlock && chunk.document_kind.is_none())
        {
            return Ok(None);
        }
        head_attempt
    };
    Ok(Some(Prepared {
        reference: reference.clone(),
        text: chunk.text.trim().chars().take(INPUT_CHAR_LIMIT).collect(),
        attempt,
    }))
}

async fn still_current(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
    prepared: &Prepared,
) -> CoreResult<bool> {
    Ok(prepare(provider, request, &prepared.reference)
        .await?
        .is_some_and(|now| now.attempt == prepared.attempt && now.text == prepared.text))
}

async fn insert_current(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
    targets: &[Prepared],
    vector: &[f32],
    reused: bool,
    report: &mut EmbeddingBackfillReport,
) -> CoreResult<()> {
    let embedder = provider.require_embedding_provider()?;
    if vector.len() != embedder.dimensions() as usize || !vector.iter().all(|v| v.is_finite()) {
        return Err(CoreError::ValidationFailed {
            reason: "embedding vector shape is invalid".into(),
        });
    }
    let index = provider.require_vectors()?;
    for target in targets {
        if !still_current(provider, request, target).await? {
            continue;
        }
        index
            .insert(
                &target.reference.id,
                &embedder.embedding_space(),
                vector.to_vec(),
            )
            .await?;
        report.embedded += 1;
        report.code_embedded += usize::from(target.reference.code.is_some());
        report.reused += usize::from(reused);
    }
    Ok(())
}

pub(crate) async fn backfill(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
) -> CoreResult<EmbeddingBackfillReport> {
    if !(1..=1024).contains(&request.limit) {
        return Err(CoreError::InvalidRequest {
            reason: "embedding limit must be in 1..1024".into(),
        });
    }
    let embedder = provider.require_embedding_provider()?;
    let index = provider.require_vectors()?;
    if &embedder.embedding_space() != index.embedding_space() {
        return Err(CoreError::InvalidRequest {
            reason: "embedding space requires reindexing".into(),
        });
    }
    let refs = eligible_refs(provider, request).await?;
    let have: HashSet<Id> = index.embedded_ids().await?;
    let in_delta = |r: &ChunkRef| {
        request
            .source_filter
            .as_ref()
            .is_none_or(|s| &r.source == s)
    };
    let mut pending: Vec<_> = refs
        .iter()
        .filter(|r| in_delta(r) && !have.contains(&r.id))
        .collect();
    pending.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    let mut report = EmbeddingBackfillReport::default();
    if pending.is_empty() {
        return Ok(report);
    }

    let mut twins = HashMap::new();
    for reference in &refs {
        if have.contains(&reference.id) && !reference.content_hash.is_empty() {
            twins
                .entry(reference.content_hash.as_str())
                .or_insert(reference);
        }
    }
    let mut groups: BTreeMap<String, Vec<Prepared>> = BTreeMap::new();
    for reference in pending.into_iter().take(request.limit) {
        if let Some(prepared) = prepare(provider, request, reference).await? {
            let key = if reference.content_hash.is_empty() {
                format!("id:{}", reference.id.as_str())
            } else {
                format!("hash:{}", reference.content_hash)
            };
            groups.entry(key).or_default().push(prepared);
        }
    }
    let mut groups: Vec<_> = groups.into_values().collect();
    groups.sort_by(|a, b| a[0].reference.id.as_str().cmp(b[0].reference.id.as_str()));
    for batch in groups.chunks(BATCH_SIZE) {
        let mut fresh = Vec::new();
        for group in batch {
            let mut current = Vec::new();
            for prepared in group {
                if still_current(provider, request, prepared).await? {
                    current.push(prepared);
                }
            }
            if current.is_empty() {
                continue;
            }
            let first = current[0];
            if let Some(twin) = twins.get(first.reference.content_hash.as_str()) {
                if prepare(provider, request, twin).await?.is_some() {
                    if let Some(vector) = index.vector_for_target(&twin.id).await? {
                        // Recheck reuse targets just as strictly as inference targets.
                        let mut authorized = Vec::new();
                        for target in current {
                            if let Some(p) = prepare(provider, request, &target.reference).await? {
                                if p.attempt == target.attempt {
                                    authorized.push(p);
                                }
                            }
                        }
                        insert_current(provider, request, &authorized, &vector, true, &mut report)
                            .await?;
                        continue;
                    }
                }
            }
            fresh.push(current);
        }
        if fresh.is_empty() {
            continue;
        }
        let texts: Vec<String> = fresh.iter().map(|g| g[0].text.clone()).collect();
        let vectors = embedder.embed_batch(&texts)?;
        if vectors.len() != texts.len() {
            return Err(CoreError::ValidationFailed {
                reason: "embedding batch returned the wrong number of vectors".into(),
            });
        }
        for (group, vector) in fresh.into_iter().zip(vectors) {
            report.new_embeddings += 1;
            let mut authorized = Vec::new();
            for target in group {
                if let Some(p) = prepare(provider, request, &target.reference).await? {
                    if p.attempt == target.attempt && p.text == target.text {
                        authorized.push(p);
                    }
                }
            }
            insert_current(provider, request, &authorized, &vector, false, &mut report).await?;
        }
    }
    let have = index.embedded_ids().await?;
    report.remaining = eligible_refs(provider, request)
        .await?
        .iter()
        .filter(|r| in_delta(r) && !have.contains(&r.id))
        .count();
    Ok(report)
}
