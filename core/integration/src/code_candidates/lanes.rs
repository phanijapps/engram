//! Existing index lanes adapted to current canonical code segment targets.
use super::{LaneHit, resolution::Resolver, trace};
use engram_domain::*;
use engram_retrieval::{CodeRankedSegment, CodeRetrievalLane, code_scoring_pool};
use engram_runtime::{CoreError, CoreResult};
use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};
pub(super) async fn vector(
    resolver: &mut Resolver<'_>,
    query: &crate::code_query::PreparedCodeQuery,
    representation: &crate::code_representation::CodeRepresentation,
    cap: usize,
    phases: &mut BTreeMap<String, f64>,
) -> CoreResult<Vec<LaneHit>> {
    let model = resolver.provider.require_embedding_provider()?;
    let index = resolver
        .provider
        .code_vectors()
        .ok_or_else(|| CoreError::ProviderUnavailable {
            provider: "code vector index".into(),
        })?;
    if index.embedding_space() != &representation.embedding_space {
        return Err(CoreError::ProviderUnavailable {
            provider: "code representation requires reindexing".into(),
        });
    }
    let start = Instant::now();
    let vector = model.embed_prepared_code_query(query)?;
    phases.insert(
        "vector_embedding".into(),
        start.elapsed().as_secs_f64() * 1000.0,
    );
    let start = Instant::now();
    let raw = index.search(index.embedding_space(), vector, cap).await?;
    phases.insert(
        "vector_search".into(),
        start.elapsed().as_secs_f64() * 1000.0,
    );
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for (rank, (id, distance)) in raw.into_iter().take(cap).enumerate() {
        if !distance.is_finite() || !seen.insert(id.clone()) {
            continue;
        }
        if let Some(resolved) = resolver.segment(id.as_str()).await? {
            result.push(LaneHit {
                resolved,
                trace: trace("vector", rank + 1, distance),
            });
        }
    }
    Ok(result)
}

pub(super) async fn indexed(
    resolver: &mut Resolver<'_>,
    lane: CodeRetrievalLane,
    cap: usize,
) -> CoreResult<Vec<LaneHit>> {
    let index = resolver.provider.code_retrieval_lane(lane).ok_or_else(|| {
        CoreError::ProviderUnavailable {
            provider: lane.as_str().into(),
        }
    })?;
    let mut request = resolver.request.clone();
    request.modes = vec![if lane == CodeRetrievalLane::Lexical {
        RetrievalMode::Keyword
    } else {
        RetrievalMode::Graph
    }];
    let raw = index.retrieve_candidates(&request).await?;
    let mut all = Vec::new();
    let mut seen = HashSet::new();
    for row in raw.into_iter().take(cap) {
        if !row.score.total.is_finite() {
            continue;
        }
        let resolved = match row.target_type {
            RetrievalTargetType::Chunk => resolver.parent(&row.target_id, cap).await?,
            RetrievalTargetType::Entity => resolver.entity(&row.target_id, cap).await?,
            _ => vec![],
        };
        for segment in resolved {
            if seen.insert(segment.segment.id.clone()) {
                all.push((segment, row.score.total));
            }
        }
    }
    let ranks: Vec<_> = all
        .iter()
        .map(|(r, score)| CodeRankedSegment {
            segment_id: r.segment.id.clone(),
            parent_id: r.parent.id.to_string(),
            fusion_score: *score,
        })
        .collect();
    let pool = code_scoring_pool(&ranks, cap)?;
    let mut rows: Vec<_> = all.into_iter().map(Some).collect();
    pool.indices
        .into_iter()
        .enumerate()
        .map(|(rank, i)| {
            let (resolved, score) = rows.get_mut(i).and_then(Option::take).ok_or_else(|| {
                CoreError::ValidationFailed {
                    reason: "invalid code candidate pool".into(),
                }
            })?;
            Ok(LaneHit {
                resolved,
                trace: trace(lane.as_str(), rank + 1, score),
            })
        })
        .collect()
}
