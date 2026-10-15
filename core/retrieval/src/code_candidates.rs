//! Pure segment-pool budgeting and parent aggregation after existing RRF.
use std::collections::{BTreeMap, HashSet};

use engram_runtime::{CoreError, CoreResult};

pub struct CodeRankedSegment {
    pub segment_id: String,
    pub parent_id: String,
    pub fusion_score: f32,
}

pub struct CodeScoringPool {
    pub indices: Vec<usize>,
    pub omitted_segments: usize,
    pub omitted_parents: usize,
}

pub struct CodeParentRank {
    pub parent_id: String,
    /// Best scored segment first; equal scores retain original RRF order.
    pub segment_indices: Vec<usize>,
    pub score: f32,
}

fn invalid(reason: &str) -> CoreError {
    CoreError::ValidationFailed {
        reason: reason.into(),
    }
}

fn groups(rows: &[CodeRankedSegment]) -> CoreResult<Vec<Vec<usize>>> {
    let mut ids = HashSet::new();
    let mut positions = BTreeMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row.segment_id.is_empty()
            || row.parent_id.is_empty()
            || !row.fusion_score.is_finite()
            || !ids.insert(&row.segment_id)
        {
            return Err(invalid("invalid or duplicate fused code segment"));
        }
        let position = *positions.entry(&row.parent_id).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[position].push(index);
    }
    Ok(groups)
}

/// The input must already be in stable best-first RRF order. Every parent gets
/// one opportunity before subsequent segments are admitted in round-robin order.
pub fn code_scoring_pool(rows: &[CodeRankedSegment], limit: usize) -> CoreResult<CodeScoringPool> {
    if limit == 0 {
        return Err(invalid("code scorer pool limit must be positive"));
    }
    let groups = groups(rows)?;
    let admitted = groups.len().min(limit);
    let mut indices: Vec<_> = groups[..admitted].iter().map(|group| group[0]).collect();
    let mut offset = 1;
    while indices.len() < limit {
        let before = indices.len();
        for group in &groups[..admitted] {
            if let Some(index) = group.get(offset) {
                indices.push(*index);
                if indices.len() == limit {
                    break;
                }
            }
        }
        if before == indices.len() {
            break;
        }
        offset += 1;
    }
    Ok(CodeScoringPool {
        omitted_segments: rows.len() - indices.len(),
        omitted_parents: groups.len() - admitted,
        indices,
    })
}

/// With successful model scores, only pool members participate. With `None`,
/// all fused segments participate using RRF (including on model failure).
/// Partial/invalid scores are errors; callers must discard the entire batch.
pub fn code_parent_order(
    rows: &[CodeRankedSegment],
    scored: Option<(&CodeScoringPool, &[f32])>,
) -> CoreResult<Vec<CodeParentRank>> {
    let groups = groups(rows)?;
    let scores: BTreeMap<usize, f32> = match scored {
        None => rows
            .iter()
            .enumerate()
            .map(|(i, row)| (i, row.fusion_score))
            .collect(),
        Some((pool, values)) => {
            if pool.indices.len() != values.len() || values.iter().any(|v| !v.is_finite()) {
                return Err(invalid(
                    "code scorer returned incomplete or nonfinite scores",
                ));
            }
            let mut scores = BTreeMap::new();
            for (&index, &score) in pool.indices.iter().zip(values) {
                if index >= rows.len() || scores.insert(index, score).is_some() {
                    return Err(invalid("invalid or duplicate code scorer pool index"));
                }
            }
            scores
        }
    };
    let mut result = Vec::new();
    for group in groups {
        let mut segment_indices: Vec<_> = group
            .into_iter()
            .filter(|i| scores.contains_key(i))
            .collect();
        segment_indices.sort_by(|a, b| scores[b].total_cmp(&scores[a]).then_with(|| a.cmp(b)));
        if let Some(&best) = segment_indices.first() {
            result.push(CodeParentRank {
                parent_id: rows[best].parent_id.clone(),
                score: scores[&best],
                segment_indices,
            });
        }
    }
    // Stable sort preserves first RRF parent appearance on tied maxima.
    result.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(result)
}
