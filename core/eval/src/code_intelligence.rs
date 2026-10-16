//! Frozen parent-level code retrieval metrics. Labels never participate in retrieval.
use std::collections::{HashMap, HashSet};

use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

/// A corpus-pinned canonical parent judgment (0 irrelevant, 1 supporting,
/// 2 relevant implementation, 3 required implementation).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeJudgment {
    pub parent_id: String,
    pub grade: u8,
    pub required_ranges: Vec<(u64, u64)>,
}

/// Primary implementation evidence only; source references are not excerpts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedCodeUnit {
    pub parent_id: String,
    pub excerpts: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeQueryMetrics {
    pub candidate_recall_at_50: Option<f64>,
    pub required_unit_recall_at_10: Option<f64>,
    pub required_range_coverage: Option<f64>,
    pub all_required_recovered: bool,
    pub mrr_at_10: f64,
    pub ndcg_at_10: f64,
    /// Evaluation-only diagnostic, never a runtime evidence assertion.
    pub unjudged_primary_items: usize,
}

/// Evaluate one query. `candidate_parents` is the pre-score/pre-threshold stable
/// RRF order of eligible implementation parents, not the scorer pool. Inspected
/// unvalidated items cannot positively pass final ranking or range metrics.
pub fn evaluate_code_query(
    judgments: &[CodeJudgment],
    candidate_parents: &[String],
    primary: &[ObservedCodeUnit],
    supporting: &[ObservedCodeUnit],
    validated: bool,
) -> CoreResult<CodeQueryMetrics> {
    let mut labels = HashMap::new();
    for j in judgments {
        if j.parent_id.is_empty()
            || j.grade > 3
            || j.required_ranges.iter().any(|(a, b)| a >= b)
            || labels.insert(j.parent_id.as_str(), j).is_some()
        {
            return Err(CoreError::InvalidRequest {
                reason: "invalid or duplicate code judgment".into(),
            });
        }
    }
    if primary
        .iter()
        .chain(supporting.iter())
        .any(|p| p.excerpts.iter().any(|(a, b)| a >= b))
    {
        return Err(CoreError::InvalidRequest {
            reason: "invalid observed excerpt range".into(),
        });
    }
    let required: Vec<_> = judgments.iter().filter(|j| j.grade == 3).collect();
    let mut seen = HashSet::new();
    let candidates: HashSet<_> = candidate_parents
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .take(50)
        .map(String::as_str)
        .collect();
    let final_items = if validated {
        &primary[..primary.len().min(10)]
    } else {
        &[]
    };
    let recovered: HashSet<_> = final_items.iter().map(|p| p.parent_id.as_str()).collect();
    let recall = |ids: &HashSet<&str>| {
        (!required.is_empty()).then(|| {
            required
                .iter()
                .filter(|j| ids.contains(j.parent_id.as_str()))
                .count() as f64
                / required.len() as f64
        })
    };
    let mut seen = HashSet::new();
    let mut mrr = 0.0;
    let mut dcg = 0.0;
    for (rank, item) in final_items.iter().enumerate() {
        let grade = if seen.insert(item.parent_id.as_str()) {
            labels.get(item.parent_id.as_str()).map_or(0, |j| j.grade)
        } else {
            0
        };
        if grade >= 2 && mrr == 0.0 {
            mrr = 1.0 / (rank + 1) as f64;
        }
        dcg += gain(grade) / ((rank + 2) as f64).log2();
    }
    let mut ideal: Vec<_> = judgments.iter().map(|j| j.grade).collect();
    ideal.sort_unstable_by(|a, b| b.cmp(a));
    let idcg: f64 = ideal
        .iter()
        .take(10)
        .enumerate()
        .map(|(rank, grade)| gain(*grade) / ((rank + 2) as f64).log2())
        .sum();
    let mut expected_bytes = 0u64;
    let mut covered_bytes = 0u64;
    for j in &required {
        let ranges = union(&j.required_ranges);
        expected_bytes += ranges.iter().map(|(a, b)| b - a).sum::<u64>();
        let validated_supporting = if validated { supporting } else { &[] };
        let observed: Vec<_> = final_items
            .iter()
            .chain(validated_supporting.iter())
            .filter(|p| p.parent_id == j.parent_id)
            .flat_map(|p| p.excerpts.iter().copied())
            .collect();
        let observed = union(&observed);
        for (a, b) in &ranges {
            covered_bytes += observed
                .iter()
                .map(|(x, y)| b.min(y).saturating_sub(*a.max(x)))
                .sum::<u64>();
        }
    }
    let final_recall = recall(&recovered);
    Ok(CodeQueryMetrics {
        candidate_recall_at_50: recall(&candidates),
        required_unit_recall_at_10: final_recall,
        required_range_coverage: (expected_bytes > 0)
            .then(|| covered_bytes as f64 / expected_bytes as f64),
        all_required_recovered: final_recall == Some(1.0),
        mrr_at_10: mrr,
        ndcg_at_10: if idcg > 0.0 { dcg / idcg } else { 0.0 },
        unjudged_primary_items: primary
            .iter()
            .take(10)
            .filter(|p| !labels.contains_key(p.parent_id.as_str()))
            .count(),
    })
}

fn gain(grade: u8) -> f64 {
    ((1u32 << grade) - 1) as f64
}

fn union(ranges: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (a, b) in sorted {
        if let Some(last) = merged.last_mut()
            && a <= last.1
        {
            last.1 = last.1.max(b);
        } else {
            merged.push((a, b));
        }
    }
    merged
}
