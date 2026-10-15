//! Existing weighted RRF over segment identities, retaining contributing traces.
use super::{CodeCandidate, LaneHit};
use engram_domain::*;
use engram_retrieval::{
    CodeIntelligenceConfig, ReciprocalFusionConfig, ReciprocalRankFusion, RetrievalFusion,
};
use engram_runtime::CoreResult;
use std::collections::{BTreeMap, HashSet};

pub(super) fn fuse(
    request: &RetrievalRequest,
    config: &CodeIntelligenceConfig,
    hits: Vec<LaneHit>,
) -> CoreResult<Vec<CodeCandidate>> {
    let mut request = request.clone();
    request.limit = None; // The final parent budget must not truncate candidate depth.
    let mut candidates = Vec::new();
    let mut originals: BTreeMap<String, CodeCandidate> = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut ranks: BTreeMap<String, u32> = BTreeMap::new();
    for mut hit in hits {
        let id = hit.resolved.segment.id.clone();
        if !seen.insert((hit.trace.source.clone(), id.clone())) {
            continue;
        }
        let rank = ranks.entry(hit.trace.source.clone()).or_default();
        *rank += 1;
        // Existing RRF ranks admitted inputs contiguously, after source checks.
        hit.trace.source_rank = Some(*rank);
        let score = hit.trace.source_score.unwrap_or(0.0);
        let parent = &hit.resolved.parent;
        candidates.push(RetrievalResult {
            id: id.clone(),
            target_id: id.clone(),
            target_type: RetrievalTargetType::Chunk,
            content: String::new(),
            score: RetrievalScore {
                total: score,
                relevance: None,
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: None,
            },
            provenance: parent.provenance.clone(),
            policy: parent.policy.clone(),
            explanation: None,
            fusion_trace: Some(hit.trace.clone()),
            metadata: None,
        });
        originals
            .entry(id)
            .and_modify(|candidate| candidate.fusion_traces.push(hit.trace.clone()))
            .or_insert(CodeCandidate {
                indexed_revision: None,
                segment: hit.resolved.segment,
                parent: hit.resolved.parent,
                fusion_score: 0.0,
                fusion_traces: vec![hit.trace],
            });
    }
    // Expansion has its own provenance/rank list, using the admitted graph
    // weight. It must not create a second contribution in the same lane.
    let configured = config.fusion.to_reciprocal_config()?;
    let mut weights = config.fusion.source_weights.clone();
    weights.insert("graph_expansion".into(), configured.source_weight("graph"));
    let fusion =
        ReciprocalFusionConfig::new(configured.k(), config.fusion.default_source_weight, weights)?;
    let fused = ReciprocalRankFusion::new(fusion).fuse(&request, candidates)?;
    Ok(fused
        .into_iter()
        .enumerate()
        .filter_map(|(rank, row)| {
            originals.remove(&row.target_id).map(|mut candidate| {
                candidate.fusion_score = row.score.total;
                for trace in &mut candidate.fusion_traces {
                    trace.fusion_strategy = Some(FusionStrategy::ReciprocalRankFusion);
                    trace.fusion_score = Some(row.score.total);
                    trace.rank = Some((rank + 1) as u32);
                }
                candidate
            })
        })
        .collect())
}
