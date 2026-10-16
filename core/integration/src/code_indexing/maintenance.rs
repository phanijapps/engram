//! Source-authorized preparation, bounded inference and activation rechecks.
use crate::{EngramProvider, code_representation::code_representation};
use engram_domain::*;
use engram_knowledge::{CodeSegmentSet, code_source_retrieval_authorized};
use engram_runtime::{CoreError, CoreResult};

fn unavailable(reason: &str) -> CoreError {
    CoreError::ProviderUnavailable {
        provider: reason.into(),
    }
}

async fn prepare(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
    m: &CodeSourceGeneration,
    representation: &crate::code_representation::CodeRepresentation,
    config: &engram_retrieval::CodeIndexConfig,
) -> CoreResult<CodeSegmentSet> {
    let port = provider
        .code_segments()
        .ok_or_else(|| unavailable("code segment repository"))?;
    let prior = port
        .code_segment_inventory(&m.file, &representation.id)
        .await?;
    if let Some(set) = port
        .load_current_code_segments(&m.file, &representation.id, &request.requester)
        .await?
    {
        if set.lease.inventory.state != CodeSourceState::Incomplete {
            return Ok(set);
        }
    }
    // Whole-source access uses its existing explicit authorization contract.
    // Never grant code:source:read to a caller merely because it can reindex.
    let source = provider
        .require_code_source()?
        .read_code_source(
            CodeSourceReadRequest {
                file: m.file.clone(),
                requester: request.requester.clone(),
                generation: Some(m.generation.clone()),
                max_bytes: m.byte_length.max(1),
            },
            &request.scope,
        )
        .await?;
    if source.manifest.attempt_id != m.attempt_id {
        return Err(unavailable("source changed before segment preparation"));
    }
    let mut parents = Vec::new();
    for record in m.records.iter().filter(|r| {
        r.kind == CodeSourceRecordKind::Chunk
            && !m.snapshot_ids.iter().any(|id| id.as_str() == r.id)
    }) {
        let parent = provider
            .require_knowledge()?
            .get_chunk(&Id::from(record.id.clone()), &request.scope)
            .await?
            .ok_or_else(|| unavailable("current canonical code parent"))?;
        parents.push(parent);
    }
    let derived = super::derivation::derive(
        &source,
        &parents,
        provider.require_embedding_provider()?.as_ref(),
        config,
        representation,
    );
    let set = match derived {
        Ok(set) => set,
        Err(error) => {
            // Unsupported segmentation is explicit durable derived state, not
            // absence that could be mistaken for a ready representation.
            let failed = CodeSegmentInventory {
                file: m.file.clone(),
                generation: m.generation.clone(),
                attempt_id: m.attempt_id.clone(),
                representation_id: representation.id.clone(),
                segment_ids: vec![],
                state: CodeSourceState::Updating,
            };
            let lease = port.begin_code_segments(failed, prior.as_ref()).await?;
            port.fail_code_segments(&lease).await?;
            return Err(error);
        }
    };
    let lease = port
        .begin_code_segments(set.inventory, prior.as_ref())
        .await?;
    if let Err(error) = port.write_code_segments(&lease, set.segments.clone()).await {
        let _ = port.fail_code_segments(&lease).await;
        return Err(error);
    }
    Ok(CodeSegmentSet {
        lease,
        segments: set.segments,
    })
}

pub(crate) async fn backfill(
    provider: &EngramProvider,
    request: &EmbeddingBackfillRequest,
    limit: usize,
) -> CoreResult<EmbeddingBackfillReport> {
    let model = provider.require_embedding_provider()?;
    if !model.is_local() {
        return Ok(EmbeddingBackfillReport::default());
    }
    let index = provider
        .code_vectors()
        .ok_or_else(|| unavailable("representation-keyed code vector index"))?;
    let port = provider
        .code_segments()
        .ok_or_else(|| unavailable("code segment repository"))?;
    let config = super::index_config(provider);
    let representation = code_representation(model.as_ref(), &config)?;
    if index.embedding_space() != &representation.embedding_space {
        return Err(unavailable(
            "code vector representation requires reindexing",
        ));
    }
    let sources = provider
        .require_code_source()?
        .list_code_sources(&request.scope)
        .await?;
    let mut have = index.embedded_ids().await?;
    let mut report = EmbeddingBackfillReport::default();
    for m in sources {
        if m.state != CodeSourceState::Active
            || !code_source_retrieval_authorized(
                &m,
                &request.requester,
                &request.scope,
                chrono::Utc::now(),
            )
            || request
                .source_filter
                .as_ref()
                .is_some_and(|s| s != &m.provenance.source)
        {
            continue;
        }
        let set = prepare(provider, request, &m, &representation, &config).await?;
        let pending: Vec<_> = set
            .segments
            .iter()
            .filter(|s| !have.contains(&Id::from(s.id.clone())))
            .collect();
        let admitted = limit.saturating_sub(report.embedded).min(pending.len());
        let inference = async {
            for batch in pending[..admitted].chunks(64) {
                let mut targets = Vec::new();
                let mut texts = Vec::new();
                for s in batch {
                    // Updating inventories cannot resolve through the query API.
                    // Reload the current authorized canonical parent directly.
                    let resolved = port
                        .prepare_code_segment(
                            &s.id,
                            &request.scope,
                            &request.requester,
                            &representation.id,
                        )
                        .await?
                        .filter(|r| r.segment == **s)
                        .ok_or_else(|| unavailable("code parent changed before inference"))?;
                    let body = engram_knowledge::code_segment_text(&resolved.parent, &s.ranges)?;
                    let (text, coverage) = super::input::build_input(
                        &body,
                        &s.context,
                        model.as_ref(),
                        &config,
                        &representation,
                    )?
                    .ok_or_else(|| unavailable("segment code/context token budget changed"))?;
                    if coverage != s.embedding_input {
                        return Err(unavailable("segment model-input contract changed"));
                    }
                    targets.push(s);
                    texts.push(text);
                }
                let vectors = model.embed_batch(&texts)?;
                if vectors.len() != targets.len() {
                    return Err(unavailable(
                        "code embedding batch returned the wrong number of vectors",
                    ));
                }
                report.new_embeddings += vectors.len();
                // A source transition during tokenization/inference revokes the
                // entire in-flight batch before it reaches the secondary index.
                let current = provider
                    .require_code_source()?
                    .get_code_source(&m.file)
                    .await?
                    .ok_or_else(|| unavailable("code source disappeared during inference"))?;
                if current.attempt_id != m.attempt_id
                    || !code_source_retrieval_authorized(
                        &current,
                        &request.requester,
                        &request.scope,
                        chrono::Utc::now(),
                    )
                {
                    continue;
                }
                for (s, vector) in targets.into_iter().zip(vectors) {
                    if vector.len() != model.dimensions() as usize
                        || !vector.iter().all(|v| v.is_finite())
                    {
                        return Err(unavailable("code embedding vector shape is invalid"));
                    }
                    // Recheck canonical/head/attempt authority immediately before
                    // each insert; activation also checks it transactionally.
                    let current = port
                        .prepare_code_segment(
                            &s.id,
                            &request.scope,
                            &request.requester,
                            &representation.id,
                        )
                        .await?;
                    if !current.is_some_and(|v| v.segment == **s) {
                        continue;
                    }
                    let id = Id::from(s.id.clone());
                    index
                        .insert(&id, &representation.embedding_space, vector)
                        .await?;
                    have.insert(id);
                    report.embedded += 1;
                    report.code_embedded += 1;
                }
            }
            Ok::<(), CoreError>(())
        }
        .await;
        if let Err(error) = inference {
            if set.lease.inventory.state == CodeSourceState::Updating {
                let _ = port.fail_code_segments(&set.lease).await;
            }
            return Err(error);
        }
        let remaining = set
            .lease
            .inventory
            .segment_ids
            .iter()
            .filter(|id| !have.contains(&Id::from((*id).clone())))
            .count();
        report.remaining += remaining;
        if remaining == 0 && set.lease.inventory.state == CodeSourceState::Updating {
            port.activate_code_segments(&set.lease).await?;
        }
    }
    // The durable index is authoritative; never count legacy canonical vectors
    // or snapshots as having satisfied this representation.
    Ok(report)
}
