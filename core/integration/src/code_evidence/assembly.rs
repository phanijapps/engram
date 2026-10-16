//! Exact-source packet assembly with coherent winning-view and output budgets.
use super::{preflight::Admission, ranking::Ranking};
use crate::{EmbeddingProvider, code_candidates::CodeCandidate};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use std::collections::{BTreeMap, HashMap, HashSet};
// Reserve space for final phase/retry diagnostics appended by the operation.
const MAX_BYTES: usize = 261120;

fn reference(c: &CodeCandidate, range: CodeByteRange) -> CodeEvidenceSourceRef {
    CodeEvidenceSourceRef {
        repository: c.segment.file.repository.clone(),
        path: c.segment.file.path.clone(),
        unit_id: c.segment.parent_unit_id.clone(),
        source_hash: c.segment.source_hash.clone(),
        generation: c.segment.generation.clone(),
        representation_id: c.segment.representation_id.clone(),
        start_byte: range.start,
        end_byte: range.end,
        indexed_revision: c.indexed_revision.clone(),
    }
}
fn enriched_facts(c: &CodeCandidate, facts: &[CodeContextFact]) -> Vec<CodeContextFact> {
    let mut facts = facts.to_vec();
    for fact in &mut facts {
        for reference in &mut fact.sources {
            if reference.repository == c.segment.file.repository
                && reference.path == c.segment.file.path
                && reference.generation == c.segment.generation
                && reference.source_hash == c.segment.source_hash
            {
                reference.indexed_revision = c.indexed_revision.clone();
            }
        }
    }
    facts
}
fn parts(c: &CodeCandidate, ranges: &[CodeByteRange]) -> CoreResult<Vec<CodeExcerptPart>> {
    ranges
        .iter()
        .filter(|r| r.start < r.end)
        .map(|&range| {
            Ok(CodeExcerptPart {
                source: reference(c, range),
                text: engram_knowledge::code_segment_text(&c.parent, &[range])?,
            })
        })
        .collect()
}
fn refs(c: &CodeCandidate, ranges: &[CodeByteRange]) -> Vec<CodeEvidenceSourceRef> {
    ranges
        .iter()
        .filter(|r| r.start < r.end)
        .map(|&r| reference(c, r))
        .collect()
}
fn merge(mut ranges: Vec<CodeByteRange>) -> Vec<CodeByteRange> {
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<CodeByteRange> = vec![];
    for r in ranges {
        if let Some(last) = out.last_mut()
            && r.start <= last.end
        {
            last.end = last.end.max(r.end);
            continue;
        }
        out.push(r);
    }
    out
}
fn partial(c: &CodeCandidate, ranges: Vec<CodeByteRange>) -> bool {
    c.parent.code.as_ref().is_none_or(|code| {
        !code.complete
            || code.role == CodeChunkRole::ContainerContext
            || merge(ranges) != merge(code.ranges.clone())
    })
}
fn name(c: &CodeCandidate) -> String {
    c.parent
        .location
        .as_ref()
        .and_then(|l| l.anchor.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| c.segment.parent_unit_id.clone())
}
fn count(model: &dyn EmbeddingProvider, text: &str) -> CoreResult<u64> {
    Ok(model.tokenize_untruncated(text, false)?.total_tokens() as u64)
}
fn fact_tokens(model: &dyn EmbeddingProvider, facts: &[CodeContextFact]) -> CoreResult<u64> {
    facts.iter().try_fold(0, |sum, f| {
        Ok(sum
            + count(
                model,
                &engram_knowledge::code_context_text(std::slice::from_ref(f)),
            )?)
    })
}
fn item_tokens(model: &dyn EmbeddingProvider, item: &CodeEvidenceItem) -> CoreResult<u64> {
    let mut total = fact_tokens(model, &item.context)?;
    for part in &item.excerpts {
        total += count(model, &part.text)?;
    }
    for segment in &item.segments {
        if let Some(facts) = &segment.rerank_context {
            total += fact_tokens(model, facts)?;
        }
    }
    Ok(total)
}
fn valid_facts(facts: &[CodeContextFact]) -> bool {
    facts.len() <= 20
        && facts.iter().all(|f| {
            f.value.chars().count() <= 8192 && !f.sources.is_empty() && f.sources.len() <= 20
        })
}
fn valid_item(item: &CodeEvidenceItem) -> bool {
    !item.excerpts.is_empty()
        && item.excerpts.len() <= 20
        && !item.segments.is_empty()
        && item.segments.len() <= 200
        && valid_facts(&item.context)
        && item
            .excerpts
            .iter()
            .all(|p| p.text.chars().count() <= 131072)
        && item.segments.iter().all(|s| {
            !s.sources.is_empty()
                && s.sources.len() <= 20
                && s.fusion_traces.len() <= 10
                && !s.fusion_traces.is_empty()
                && s.rerank_context.as_ref().is_none_or(|f| valid_facts(f))
        })
}

pub(super) struct PacketInput<'a> {
    pub primary: &'a [CodeCandidate],
    pub supporting: &'a [CodeCandidate],
    pub ranking: &'a Ranking,
    pub admission: &'a Admission,
    pub binding: &'a CodeCalibrationArtifact,
    pub calibration: Option<&'a CodeCalibrationArtifact>,
    pub counts: BTreeMap<String, u32>,
    pub phases: BTreeMap<String, f64>,
    pub failures: Vec<RetrievalSourceFailure>,
    pub reindex_required: bool,
}
pub(super) fn assemble(
    model: &dyn EmbeddingProvider,
    input: PacketInput<'_>,
) -> CoreResult<CodeEvidencePacket> {
    let PacketInput {
        primary,
        supporting,
        ranking,
        admission,
        binding,
        calibration,
        counts,
        phases,
        failures,
        reindex_required,
    } = input;
    let config = &admission.config;
    let mut packet = CodeEvidencePacket {
        items: vec![],
        supporting_units: vec![],
        evidence_status: CodeEvidenceStatus::InsufficientEvidence,
        source_failures: failures,
        diagnostics: CodeEvidenceDiagnostics {
            parent_decisions: vec![],
            omitted_parent_decisions: 0,
            config_hash: binding.config_hash.clone(),
            profile: config.profile,
            requested_reranker: admission.requested,
            effective_reranker: ranking.effective,
            scorer_state: ranking.state,
            scorer_manifest_id: admission.identity.as_ref().map(|i| i.manifest_id.clone()),
            calibration_state: if calibration.is_some() {
                CodeCalibrationState::Validated
            } else {
                CodeCalibrationState::Unvalidated
            },
            candidate_counts: counts,
            omitted_units: 0,
            truncated: false,
            reindex_required,
            config_origins: admission.resolved.origins.clone(),
            query_representation_id: binding.query_representation_id.clone(),
            phase_ms: phases,
            omitted_segments: ranking.omitted_segments,
            omitted_parents: ranking.omitted_parents,
            scored_pairs: ranking.scores.len() as u32,
            calibration_id: calibration.map(|c| c.id.clone()),
            threshold_score_space: calibration.map(|c| c.score_space),
        },
    };
    packet.diagnostics.candidate_counts.insert(
        "segments_with_unprepared_scoring_tails".into(),
        ranking.remaining_continuations,
    );
    packet
        .diagnostics
        .candidate_counts
        .insert("prepared_scoring_views".into(), ranking.prepared_views);
    packet
        .diagnostics
        .candidate_counts
        .insert("prepared_but_unscored_views".into(), ranking.unscored_views);
    packet
        .diagnostics
        .candidate_counts
        .insert("attempted_scoring_pairs".into(), ranking.attempted_pairs);
    packet.diagnostics.candidate_counts.insert(
        "scorer_pool_omitted_segments".into(),
        ranking.pool_omitted_segments,
    );
    let mut budget = config.evidence.token_budget as u64;
    let mut files: HashMap<(&str, &str), u32> = HashMap::new();
    let mut rejected = BTreeMap::new();
    for parent in super::weak_order::parents(ranking, primary, calibration, config) {
        let Some(&(first, _, _)) = parent.segments.first() else {
            rejected.insert(
                parent.parent.clone(),
                CodeParentDecisionReason::InvalidEvidence,
            );
            continue;
        };
        let c = &primary[first];
        if parent.parent != c.parent.id.as_str() {
            return Err(invalid());
        }
        let file = (
            c.segment.file.repository.as_str(),
            c.segment.file.path.as_str(),
        );
        if packet.items.len() >= config.retrieval.result_limit as usize
            || files.get(&file).copied().unwrap_or(0) >= config.evidence.max_per_file
        {
            rejected.insert(
                parent.parent.clone(),
                if packet.items.len() >= config.retrieval.result_limit as usize {
                    CodeParentDecisionReason::ResultLimit
                } else {
                    CodeParentDecisionReason::PerFileLimit
                },
            );
            packet.diagnostics.omitted_units += 1;
            packet.diagnostics.truncated = true;
            continue;
        }
        let mut item = CodeEvidenceItem {
            parent_unit_id: c.segment.parent_unit_id.clone(),
            name: name(c),
            kind: CodePrimaryKind::Code,
            source_category: CodePrimaryCategory::Implementation,
            excerpts: vec![],
            segments: vec![],
            context: vec![],
            partial: true,
            fusion_score: c.fusion_score,
            rerank_score: (ranking.effective == CodeEvidenceReranker::CrossEncoder)
                .then_some(parent.score),
        };
        let mut selected_ranges = Vec::new();
        for &(original, view, score) in &parent.segments {
            let candidate = &primary[original];
            let scoring = view.map(|i| &ranking.views[i]);
            let ranges = scoring
                .map(|v| v.ranges.as_slice())
                .unwrap_or(&candidate.segment.ranges);
            let mut next = item.clone();
            next.excerpts.extend(parts(candidate, ranges)?);
            next.segments.push(CodeSegmentEvidence {
                segment_id: candidate.segment.id.clone(),
                sources: refs(candidate, &candidate.segment.ranges),
                fusion_traces: candidate.fusion_traces.clone(),
                fusion_score: candidate.fusion_score,
                rerank_score: scoring.map(|_| score),
                embedding_input: candidate.segment.embedding_input.clone(),
                rerank_input: scoring.map(|v| v.coverage.clone()),
                rerank_sources: scoring.map(|v| refs(candidate, &v.ranges)),
                rerank_context: scoring.map(|v| enriched_facts(candidate, &v.context)),
            });
            for fact in &enriched_facts(candidate, &candidate.segment.context) {
                if !next.context.contains(fact) {
                    next.context.push(fact.clone());
                }
            }
            let rejection = if !valid_item(&next) {
                Some(CodeParentDecisionReason::InvalidEvidence)
            } else if item_tokens(model, &next)? > budget {
                Some(CodeParentDecisionReason::EvidenceBudget)
            } else if serde_json::to_vec(&next).map_err(|_| invalid())?.len() > MAX_BYTES {
                Some(CodeParentDecisionReason::SerializedByteLimit)
            } else {
                None
            };
            if let Some(reason) = rejection {
                packet.diagnostics.omitted_segments += 1;
                packet.diagnostics.truncated = true;
                // Never retain a lower score after dropping the maximum view.
                if item.segments.is_empty() {
                    rejected.insert(parent.parent.clone(), reason);
                    break;
                }
                continue;
            }
            item = next;
            selected_ranges.extend_from_slice(ranges);
        }
        if item.segments.is_empty() {
            packet.diagnostics.omitted_units += 1;
            continue;
        }
        item.partial = partial(c, selected_ranges);
        item.fusion_score = parent
            .segments
            .iter()
            .filter(|(i, _, _)| {
                item.segments
                    .iter()
                    .any(|s| s.segment_id == primary[*i].segment.id)
            })
            .map(|(i, _, _)| primary[*i].fusion_score)
            .fold(0.0, f32::max);
        budget = budget.saturating_sub(item_tokens(model, &item)?);
        *files.entry(file).or_default() += 1;
        packet.items.push(item);
    }
    // Calibration fits the maximum emitted primary score per query. Apply
    // that same packet statistic, rather than silently deleting lower-scored
    // implementation parts from an otherwise accepted answer. Only returnable
    // evidence may admit a packet; an oversized high score cannot authorize it.
    if calibration.is_some_and(|c| {
        !packet.items.iter().any(|item| {
            let score = match c.score_space {
                CodeScoreSpace::CrossEncoder => item.rerank_score,
                CodeScoreSpace::Rrf => Some(item.fusion_score),
            };
            score.is_some_and(|s| s >= c.threshold)
        })
    }) {
        let denied: HashSet<_> = packet
            .items
            .iter()
            .map(|i| i.parent_unit_id.as_str())
            .collect();
        for candidate in primary {
            if denied.contains(candidate.segment.parent_unit_id.as_str()) {
                rejected.insert(
                    candidate.parent.id.to_string(),
                    CodeParentDecisionReason::BelowThreshold,
                );
            }
        }
        packet.diagnostics.omitted_units += packet.items.len() as u32;
        packet.items.clear();
        budget = config.evidence.token_budget as u64;
        files.clear();
    }
    let mut seen = HashSet::new();
    for c in supporting {
        if !seen.insert(c.parent.id.clone()) {
            continue;
        }
        let file = (
            c.segment.file.repository.as_str(),
            c.segment.file.path.as_str(),
        );
        let excerpts = parts(c, &c.segment.ranges)?;
        let cost = excerpts.iter().try_fold(0u64, |sum, p| {
            Ok::<_, CoreError>(sum + count(model, &p.text)?)
        })?;
        if packet.supporting_units.len() >= config.evidence.max_supporting_units as usize
            || files.get(&file).copied().unwrap_or(0) >= config.evidence.max_per_file
            || cost > budget
            || excerpts.is_empty()
            || excerpts.len() > 20
            || excerpts.iter().any(|p| p.text.chars().count() > 131072)
        {
            packet.diagnostics.omitted_units += 1;
            packet.diagnostics.truncated = true;
            continue;
        }
        budget -= cost;
        *files.entry(file).or_default() += 1;
        packet.supporting_units.push(CodeSupportingUnit {
            unit_id: c.segment.parent_unit_id.clone(),
            name: name(c),
            kind: CodeEvidenceKind::Code,
            source_category: c.segment.source_category,
            excerpts,
            partial: partial(c, c.segment.ranges.clone()),
        });
    }
    while serde_json::to_vec(&packet).map_err(|_| invalid())?.len() > MAX_BYTES {
        if packet.supporting_units.pop().is_none() && packet.items.pop().is_none() {
            return Err(invalid());
        }
        packet.diagnostics.omitted_units += 1;
        packet.diagnostics.truncated = true;
    }
    packet.diagnostics.omitted_parents = packet
        .diagnostics
        .omitted_parents
        .saturating_add((ranking.parents.len().saturating_sub(packet.items.len())) as u32);
    packet.diagnostics.truncated |=
        packet.diagnostics.omitted_segments > 0 || ranking.remaining_continuations > 0;
    packet.evidence_status = if calibration.is_some() && !packet.items.is_empty() {
        CodeEvidenceStatus::CandidatesAvailable
    } else {
        CodeEvidenceStatus::InsufficientEvidence
    };
    super::decisions::attach(&mut packet, primary, ranking, calibration, &rejected)?;
    Ok(packet)
}
fn invalid() -> CoreError {
    CoreError::ValidationFailed {
        reason: "bounded code evidence metadata cannot be serialized".into(),
    }
}

#[cfg(test)]
#[path = "assembly_tests.rs"]
mod tests;
