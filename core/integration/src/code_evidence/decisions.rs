//! Bounded selection diagnostics, attached after evidence budgets are settled.
use super::ranking::Ranking;
use crate::code_candidates::CodeCandidate;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_DECISIONS: usize = 200;
// Evidence assembly retains its original 261120-byte limit. Diagnostics may
// use another 512 bytes; 512 remain for the operation's final timing/retry fields.
const MAX_BYTES: usize = 261632;

pub(super) fn attach(
    packet: &mut CodeEvidencePacket,
    primary: &[CodeCandidate],
    ranking: &Ranking,
    calibration: Option<&CodeCalibrationArtifact>,
    rejected: &BTreeMap<String, CodeParentDecisionReason>,
) -> CoreResult<()> {
    let mut stats: HashMap<&str, (u32, f32, &str)> = HashMap::new();
    for c in primary {
        let stat = stats.entry(c.parent.id.as_str()).or_insert((
            0,
            c.fusion_score,
            &c.segment.parent_unit_id,
        ));
        stat.0 += 1;
        stat.1 = stat.1.max(c.fusion_score);
    }
    let selected: HashSet<_> = packet
        .items
        .iter()
        .map(|i| i.parent_unit_id.as_str())
        .collect();
    let mut seen = HashSet::new();
    let mut decisions = Vec::new();
    let mut omitted = 0u32;
    let scored = ranking.effective == CodeEvidenceReranker::CrossEncoder;
    for parent in &ranking.parents {
        seen.insert(parent.parent.as_str());
        if decisions.len() == MAX_DECISIONS {
            omitted += 1;
            continue;
        }
        let (retrieved_segments, fusion_score, unit_id) = stats[parent.parent.as_str()];
        let reason = if selected.contains(unit_id) {
            CodeParentDecisionReason::Selected
        } else {
            rejected
                .get(&parent.parent)
                .copied()
                .unwrap_or(CodeParentDecisionReason::SerializedByteLimit)
        };
        decisions.push(CodeParentDecision {
            parent_unit_id: unit_id.into(),
            reason,
            fusion_score,
            rerank_score: scored.then_some(parent.score),
            applied_threshold: calibration.map(|c| c.threshold),
            retrieved_segments,
            scored_segments: if scored {
                parent.segments.len() as u32
            } else {
                0
            },
        });
    }
    // Keep first RRF appearance order for parents excluded before scoring.
    for c in primary {
        let id = c.parent.id.as_str();
        if !seen.insert(id) {
            continue;
        }
        if decisions.len() == MAX_DECISIONS {
            omitted += 1;
            continue;
        }
        let (retrieved_segments, fusion_score, unit_id) = stats[id];
        decisions.push(CodeParentDecision {
            parent_unit_id: unit_id.into(),
            reason: CodeParentDecisionReason::ScorerPool,
            fusion_score,
            rerank_score: None,
            applied_threshold: calibration.map(|c| c.threshold),
            retrieved_segments,
            scored_segments: 0,
        });
    }
    packet.diagnostics.parent_decisions = decisions;
    packet.diagnostics.omitted_parent_decisions = omitted;
    while serde_json::to_vec(packet)
        .map_err(|_| CoreError::ValidationFailed {
            reason: "code decision diagnostics cannot be serialized".into(),
        })?
        .len()
        > MAX_BYTES
    {
        if packet.diagnostics.parent_decisions.pop().is_none() {
            return Err(CoreError::ValidationFailed {
                reason: "code decision diagnostics exceed packet bound".into(),
            });
        }
        packet.diagnostics.omitted_parent_decisions += 1;
    }
    Ok(())
}
