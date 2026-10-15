//! Bounded pair preparation, complete-score validation and parent aggregation.
use super::{
    preflight::Admission,
    views::{Cursor, View},
};
use crate::code_candidates::CodeCandidate;
use engram_domain::*;
use engram_retrieval::{
    CodeRankedSegment, CodeScorerFailurePolicy, CodeScoringPool, code_parent_order,
    code_scoring_pool_with_coverage,
};
use engram_runtime::{CoreError, CoreResult};
use std::collections::{HashMap, HashSet};

pub(super) struct RankedParent {
    pub parent: String,
    /// Original candidate index, optional winning view index, relevant score.
    pub segments: Vec<(usize, Option<usize>, f32)>,
    pub score: f32,
}
pub(super) struct Ranking {
    pub parents: Vec<RankedParent>,
    pub views: Vec<View>,
    pub scores: Vec<f32>,
    pub state: CodeScorerState,
    pub effective: CodeEvidenceReranker,
    pub omitted_segments: u32,
    pub omitted_parents: u32,
    pub remaining_continuations: u32,
    pub prepared_views: u32,
    pub unscored_views: u32,
    pub attempted_pairs: u32,
    pub pool_omitted_segments: u32,
}
fn rows(candidates: &[CodeCandidate]) -> Vec<CodeRankedSegment> {
    candidates
        .iter()
        .map(|c| CodeRankedSegment {
            segment_id: c.segment.id.clone(),
            parent_id: c.parent.id.to_string(),
            fusion_score: c.fusion_score,
        })
        .collect()
}
fn fallback(candidates: &[CodeCandidate], state: CodeScorerState) -> CoreResult<Ranking> {
    let rows = rows(candidates);
    Ok(Ranking {
        parents: code_parent_order(&rows, None)?
            .into_iter()
            .map(|p| RankedParent {
                parent: p.parent_id,
                score: p.score,
                segments: p
                    .segment_indices
                    .into_iter()
                    .map(|i| (i, None, rows[i].fusion_score))
                    .collect(),
            })
            .collect(),
        views: vec![],
        scores: vec![],
        state,
        effective: CodeEvidenceReranker::None,
        omitted_segments: 0,
        omitted_parents: 0,
        remaining_continuations: 0,
        prepared_views: 0,
        unscored_views: 0,
        attempted_pairs: 0,
        pool_omitted_segments: 0,
    })
}
pub(super) async fn rank(
    provider: &crate::EngramProvider,
    candidates: &[CodeCandidate],
    query: &str,
    admission: &Admission,
    scope: &Scope,
    requester: &Requester,
    representation: &str,
) -> CoreResult<Ranking> {
    let Some(scorer) = admission.scorer.as_deref() else {
        return fallback(candidates, admission.state);
    };
    let config = &admission.config.local_scorer;
    let mut prepared_views = 0;
    let mut attempted_pairs = 0;
    let mut pool_omitted_segments = 0;
    let mut tails = HashSet::new();
    let result = async {
        if !(scorer.is_local() && scorer.supports_code_input())
            || scorer.identity() != admission.identity
        {
            return Err(super::preflight::unavailable());
        }
        let originals = rows(candidates);
        let coverage_parents: HashSet<_> = candidates
            .iter()
            .filter(|candidate| {
                candidate
                    .parent
                    .code
                    .as_ref()
                    .is_some_and(|code| code.role == CodeChunkRole::Symbol)
            })
            .map(|candidate| candidate.parent.id.to_string())
            .collect();
        let pool = code_scoring_pool_with_coverage(
            &originals,
            config.max_parents as usize,
            config.max_candidates as usize,
            |parent| coverage_parents.contains(parent),
        )?;
        pool_omitted_segments = pool.omitted_segments as u32;
        let mut cursors: Vec<_> = pool
            .indices
            .iter()
            .map(|&i| Cursor::new(i, &candidates[i]))
            .collect();
        let mut views = Vec::new();
        for (cursor, &index) in cursors.iter_mut().zip(&pool.indices) {
            if !super::eligibility::current(
                provider,
                &candidates[index],
                scope,
                requester,
                representation,
            )
            .await?
            {
                return Err(super::eligibility::changed());
            }
            if let Some(view) = cursor.next(&candidates[index], scorer, query, config)? {
                views.push(view);
                prepared_views += 1;
                if cursor.has_remaining() {
                    tails.insert(index);
                } else {
                    tails.remove(&index);
                }
            }
        }
        let mut parents = Vec::new();
        let mut by_parent: HashMap<&str, Vec<usize>> = HashMap::new();
        for (position, &index) in pool.indices.iter().enumerate() {
            let parent = originals[index].parent_id.as_str();
            if !by_parent.contains_key(parent) {
                parents.push(parent);
            }
            by_parent.entry(parent).or_default().push(position);
        }
        while views.len() < config.max_candidates as usize {
            let before = views.len();
            for parent in &parents {
                if !coverage_parents.contains(*parent) {
                    continue;
                }
                if let Some(&cursor) = by_parent[parent]
                    .iter()
                    .find(|&&i| cursors[i].has_remaining())
                {
                    let original = pool.indices[cursor];
                    if !super::eligibility::current(
                        provider,
                        &candidates[original],
                        scope,
                        requester,
                        representation,
                    )
                    .await?
                    {
                        return Err(super::eligibility::changed());
                    }
                    if let Some(view) =
                        cursors[cursor].next(&candidates[original], scorer, query, config)?
                    {
                        views.push(view);
                        prepared_views += 1;
                        if cursors[cursor].has_remaining() {
                            tails.insert(original);
                        } else {
                            tails.remove(&original);
                        }
                    }
                }
                if views.len() == config.max_candidates as usize {
                    break;
                }
            }
            if before == views.len() {
                break;
            }
        }
        let mut checked = HashSet::new();
        for view in &views {
            if checked.insert(view.original)
                && !super::eligibility::current(
                    provider,
                    &candidates[view.original],
                    scope,
                    requester,
                    representation,
                )
                .await?
            {
                return Err(super::eligibility::changed());
            }
        }
        if !(scorer.is_local() && scorer.supports_code_input())
            || scorer.identity() != admission.identity
        {
            return Err(super::preflight::unavailable());
        }
        let documents: Vec<_> = views.iter().map(|v| v.text.clone()).collect();
        attempted_pairs = documents.len() as u32;
        let scores = if documents.is_empty() {
            vec![]
        } else {
            scorer.score_batch(query, &documents, config.batch_size as usize)?
        };
        if !(scorer.is_local() && scorer.supports_code_input())
            || scorer.identity() != admission.identity
        {
            return Err(super::preflight::unavailable());
        }
        if scores.len() != views.len() || scores.iter().any(|s| !s.is_finite()) {
            return Err(CoreError::ValidationFailed {
                reason: "code scorer returned partial or nonfinite results".into(),
            });
        }
        let view_rows: Vec<_> = views
            .iter()
            .map(|v| CodeRankedSegment {
                segment_id: v.id.clone(),
                parent_id: originals[v.original].parent_id.clone(),
                fusion_score: originals[v.original].fusion_score,
            })
            .collect();
        let scored_pool = CodeScoringPool {
            indices: (0..views.len()).collect(),
            omitted_segments: 0,
            omitted_parents: 0,
        };
        let ordered = code_parent_order(&view_rows, Some((&scored_pool, &scores)))?;
        let parents = ordered
            .into_iter()
            .map(|parent| {
                let mut seen = HashSet::new();
                RankedParent {
                    parent: parent.parent_id,
                    score: parent.score,
                    segments: parent
                        .segment_indices
                        .into_iter()
                        .filter_map(|v| {
                            seen.insert(views[v].original).then_some((
                                views[v].original,
                                Some(v),
                                scores[v],
                            ))
                        })
                        .collect(),
                }
            })
            .collect();
        let distinct = views
            .iter()
            .map(|v| v.original)
            .collect::<HashSet<_>>()
            .len();
        Ok(Ranking {
            parents,
            omitted_segments: (candidates.len() - distinct) as u32,
            omitted_parents: pool.omitted_parents as u32,
            remaining_continuations: cursors.iter().filter(|c| c.has_remaining()).count() as u32,
            views,
            scores,
            state: CodeScorerState::Ready,
            prepared_views,
            unscored_views: 0,
            attempted_pairs,
            pool_omitted_segments,
            effective: CodeEvidenceReranker::CrossEncoder,
        })
    }
    .await;
    match result {
        Ok(ranking) => Ok(ranking),
        Err(error @ CoreError::Conflict { .. }) => Err(error),
        Err(error) if config.on_failure == CodeScorerFailurePolicy::Error => Err(error),
        Err(_) => {
            let mut result = fallback(candidates, CodeScorerState::Failed)?;
            result.prepared_views = prepared_views;
            result.unscored_views = prepared_views;
            result.attempted_pairs = attempted_pairs;
            result.remaining_continuations = tails.len() as u32;
            result.pool_omitted_segments = pool_omitted_segments;
            Ok(result)
        }
    }
}
