//! Recipe-owned code-vector construction and compatibility recall adapter.
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{CodeSegmentRepository, code_segment_text};
use engram_retrieval::{CodeIndexConfig, RetrievalIndex};
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::{
    SqlKnowledgeStore, SqliteVectorIndex, VectorQueryProvider, VectorResolvedTarget,
    VectorRetrievalIndex, VectorSearchResult, VectorTargetResolver,
};
use std::{collections::HashSet, path::Path, sync::Arc};

pub(super) fn open(
    vector_path: &Path,
    model: &dyn crate::EmbeddingProvider,
    config: &CodeIndexConfig,
) -> CoreResult<(Arc<SqliteVectorIndex>, String)> {
    let representation = crate::code_representation::code_representation(model, config)?;
    let suffix = representation
        .id
        .strip_prefix("code-passage-v1/")
        .ok_or_else(|| CoreError::InvalidRequest {
            reason: "unknown code representation version".into(),
        })?;
    let path = vector_path
        .to_str()
        .ok_or_else(|| CoreError::InvalidRequest {
            reason: "code-vector path is not valid Unicode".into(),
        })?;
    let index = SqliteVectorIndex::open_with_embedding_space_namespace(
        path,
        representation.embedding_space,
        suffix,
    )?;
    if index.requires_reindex() {
        return Err(CoreError::ProviderUnavailable {
            provider: "code vector representation requires reindexing".into(),
        });
    }
    Ok((Arc::new(index), representation.id))
}

pub(super) struct SegmentResolver {
    pub store: Arc<SqlKnowledgeStore>,
    pub representation: String,
}
#[async_trait]
impl VectorTargetResolver for SegmentResolver {
    async fn source_revision(&self, _: &RetrievalRequest) -> CoreResult<u64> {
        self.store.code_index_revision().await
    }
    async fn resolve(
        &self,
        hit: &VectorSearchResult,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<VectorResolvedTarget>> {
        let Some(resolved) = self
            .store
            .resolve_code_segment(
                &hit.target_id,
                &request.scope,
                &request.requester,
                &self.representation,
            )
            .await?
        else {
            return Ok(None);
        };
        code_segment_text(&resolved.parent, &resolved.segment.ranges)?;
        let p = resolved.parent;
        Ok(Some(VectorResolvedTarget {
            target_type: RetrievalTargetType::Chunk,
            target_id: p.id.to_string(),
            content: p.text,
            provenance: p.provenance,
            policy: p.policy,
            explanation: None,
            metadata: p.metadata,
        }))
    }
}

struct FixedQuery(Vec<f32>);
impl VectorQueryProvider for FixedQuery {
    fn query_vector(&self, _: &RetrievalRequest) -> CoreResult<Vec<f32>> {
        Ok(self.0.clone())
    }
}

/// One legacy vector lane over document and complete code representations.
/// Query inference is shared; score fusion remains the existing outer RRF.
pub(super) struct CompatibleVectorLane {
    pub base: Arc<SqliteVectorIndex>,
    pub code: Arc<SqliteVectorIndex>,
    pub query: Arc<dyn VectorQueryProvider>,
    pub store: Arc<SqlKnowledgeStore>,
    pub representation: String,
}
#[async_trait]
impl RetrievalIndex for CompatibleVectorLane {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let query: Arc<dyn VectorQueryProvider> =
            Arc::new(FixedQuery(self.query.query_vector(request)?));
        let base = VectorRetrievalIndex::new(
            (*self.base).clone(),
            query.clone(),
            Arc::new(super::recall_lanes::KnowledgeVectorResolver::new(
                self.store.clone(),
            )),
        );
        let code = VectorRetrievalIndex::new(
            (*self.code).clone(),
            query,
            Arc::new(SegmentResolver {
                store: self.store.clone(),
                representation: self.representation.clone(),
            }),
        );
        let mut results = base.retrieve_candidates(request).await?;
        results.extend(code.retrieve_candidates(request).await?);
        results.sort_by(|a, b| {
            b.score
                .total
                .total_cmp(&a.score.total)
                .then_with(|| a.target_id.cmp(&b.target_id))
        });
        let mut seen = HashSet::new();
        results.retain(|r| seen.insert(r.target_id.clone()));
        results.truncate(request.limit.unwrap_or(50) as usize);
        for (rank, result) in results.iter_mut().enumerate() {
            if let Some(trace) = &mut result.fusion_trace {
                trace.source_rank = Some((rank + 1) as u32);
            }
        }
        Ok(results)
    }
}
