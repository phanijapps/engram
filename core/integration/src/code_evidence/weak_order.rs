//! Calibrated model head with a retrieval-ranked low-confidence tail.
use super::ranking::{RankedParent, Ranking};
use crate::code_candidates::CodeCandidate;
use engram_domain::{CodeCalibrationArtifact, CodeEvidenceReranker, CodeScoreSpace};
use engram_retrieval::{CodeIntelligenceConfig, CodeWeakParentOrder};
use std::{cmp::Ordering, collections::HashMap};

pub(super) fn parents<'a>(
    ranking: &'a Ranking,
    candidates: &[CodeCandidate],
    calibration: Option<&CodeCalibrationArtifact>,
    config: &CodeIntelligenceConfig,
) -> Vec<&'a RankedParent> {
    let mut ordered: Vec<_> = ranking.parents.iter().collect();
    let Some(calibration) = calibration.filter(|c| c.score_space == CodeScoreSpace::CrossEncoder)
    else {
        return ordered;
    };
    if ranking.effective != CodeEvidenceReranker::CrossEncoder
        || config.evidence.weak_parent_order == CodeWeakParentOrder::Scorer
    {
        return ordered;
    }
    let mut prior: HashMap<&str, (u32, f32, usize)> = HashMap::new();
    for (position, candidate) in candidates.iter().enumerate() {
        let entry = prior
            .entry(candidate.parent.id.as_str())
            .or_insert((u32::MAX, 0.0, position));
        entry.1 = entry.1.max(candidate.fusion_score);
        for trace in &candidate.fusion_traces {
            if !matches!(trace.source.as_str(), "vector" | "lexical") {
                continue;
            }
            let weight = config
                .fusion
                .source_weights
                .get(&trace.source)
                .copied()
                .unwrap_or(config.fusion.default_source_weight);
            if weight > 0.0
                && let Some(rank) = trace.source_rank.filter(|&rank| rank > 0)
            {
                entry.0 = entry.0.min(rank);
            }
        }
    }
    ordered.sort_by(|a, b| {
        let a_strong = a.score >= calibration.threshold;
        let b_strong = b.score >= calibration.threshold;
        match (a_strong, b_strong) {
            (true, true) => Ordering::Equal, // Stable: preserve the model head.
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => {
                let a = prior[&a.parent.as_str()];
                let b = prior[&b.parent.as_str()];
                a.0.cmp(&b.0)
                    .then_with(|| b.1.total_cmp(&a.1))
                    .then_with(|| a.2.cmp(&b.2))
            }
        }
    });
    ordered
}
