//! Exact and keyword retrieval for the in-memory adapter.
//!
//! This module is the deterministic retrieval baseline for early conformance
//! fixtures. It deliberately avoids embeddings, SQL, graph search, and model
//! providers so later adapters can prove basic scope, policy, scoring, and
//! omission behavior before adding advanced indexes.

use std::collections::BTreeSet;

use engram_domain::*;
use engram_retrieval::{RetrievalCompositionInput, compose_context};
use engram_runtime::{CoreError, CoreResult};

use crate::{
    belief_retrieval::{BeliefSnapshot, ContradictionSnapshot, belief_candidates},
    external_retrieval::external_candidates,
    hierarchy_retrieval::{apply_hierarchy_context, expand_hierarchy_memory_candidates},
    knowledge_retrieval::{KnowledgeSnapshot, knowledge_candidates},
    scope::scope_allows,
    service::InMemoryMemoryService,
    validation::validate_retrieval_request,
};

/// Retrieves context from in-memory records using the deterministic baseline.
///
/// This path applies validation, scope checks, policy checks, keyword scoring,
/// budget omission reporting, and context composition. It is intentionally
/// narrow so future SQL/vector adapters can reuse the same fixture expectations.
pub(crate) async fn retrieve(
    service: &InMemoryMemoryService,
    request: RetrievalRequest,
) -> CoreResult<ContextPayload> {
    validate_retrieval_request(&request)?;

    let now = service.clock.now();
    let terms = query_terms(&request.query);
    let include_explanations = request.include_explanations.unwrap_or(false);
    let (records, knowledge_snapshots, belief_snapshots, contradiction_snapshots, hierarchy_nodes) = {
        let state = service.lock_state()?;
        let records = state.memories.values().cloned().collect::<Vec<_>>();
        let knowledge_snapshots = state
            .knowledge_chunks
            .values()
            .filter_map(|chunk| {
                let document = state.source_documents.get(chunk.document_id.as_str())?;
                let source = state.knowledge_sources.get(document.source_id.as_str())?;
                Some(KnowledgeSnapshot {
                    source: source.clone(),
                    document: document.clone(),
                    chunk: chunk.clone(),
                })
            })
            .collect::<Vec<_>>();
        let belief_snapshots = state
            .beliefs
            .values()
            .cloned()
            .map(|belief| BeliefSnapshot { belief })
            .collect::<Vec<_>>();
        let contradiction_snapshots = state
            .contradictions
            .values()
            .cloned()
            .map(|contradiction| ContradictionSnapshot { contradiction })
            .collect::<Vec<_>>();
        let hierarchy_nodes = state.hierarchy_nodes.values().cloned().collect::<Vec<_>>();
        (
            records,
            knowledge_snapshots,
            belief_snapshots,
            contradiction_snapshots,
            hierarchy_nodes,
        )
    };

    let expansion_records = records.clone();
    let active_temporal = request.modes.contains(&RetrievalMode::Temporal);
    let active_cue = request.modes.contains(&RetrievalMode::Cue) && !request.cues.is_empty();
    let mut candidates: Vec<MemoryCandidate> = Vec::new();
    let mut omitted = Vec::new();
    for record in records {
        if !scope_allows(&record.scope, &request.scope) {
            continue;
        }
        if !memory_filter_allows(&record, request.filters.as_ref()) {
            continue;
        }
        if let Some(expires_at) = record.policy.expires_at
            && expires_at <= now
        {
            omitted.push(omitted_result(&record, OmittedReason::Expired));
            continue;
        }
        if matches!(
            record.status,
            MemoryStatus::Redacted | MemoryStatus::Forgotten
        ) {
            omitted.push(omitted_result(&record, OmittedReason::Redacted));
            continue;
        }
        if matches!(record.status, MemoryStatus::Archived)
            && !request
                .filters
                .as_ref()
                .and_then(|filters| filters.include_archived)
                .unwrap_or(false)
        {
            continue;
        }
        if !record.policy.allowed_uses.is_empty()
            && !record.policy.allowed_uses.contains(&AllowedUse::Retrieval)
        {
            omitted.push(omitted_result(&record, OmittedReason::PolicyDenied));
            continue;
        }
        if let Err(error) =
            service
                .authorizer
                .can_retrieve(&request.requester, &record.scope, &record.policy)
        {
            if matches!(error, CoreError::PolicyDenied { .. }) {
                omitted.push(omitted_result(&record, OmittedReason::PolicyDenied));
                continue;
            }
            return Err(error);
        }

        // Keyword retrieval stays always-on; Temporal and Cue are additive modes
        // that can surface a memory even without a keyword match. A memory is a
        // candidate if any active mode scores it; score.total is the best mode.
        let keyword = keyword_score(&record, &request.query, &terms);
        let temporal = if active_temporal {
            temporal_score(&record, request.filters.as_ref(), now)
        } else {
            None
        };
        let cue = if active_cue {
            cue_score(&record, &request.cues)
        } else {
            None
        };
        let total = [
            keyword.as_ref().map(|(score, _)| *score),
            temporal,
            cue.as_ref().map(|(score, _)| *score),
        ]
        .into_iter()
        .flatten()
        .max_by(|left, right| left.total_cmp(right));
        let Some(total) = total else {
            continue;
        };
        let (relevance, matched_terms) = match keyword {
            Some((score, terms)) => (Some(score), terms),
            None => (None, Vec::new()),
        };
        let (cue_match, matched_cues) = match cue {
            Some((score, cues)) => (Some(score), cues),
            None => (None, Vec::new()),
        };
        candidates.push(MemoryCandidate {
            total,
            relevance,
            recency: temporal,
            cue_match,
            matched_terms,
            matched_cues,
            record,
        });
    }

    candidates.sort_by(|left, right| {
        right
            .total
            .total_cmp(&left.total)
            .then_with(|| right.record.created_at.cmp(&left.record.created_at))
            .then_with(|| left.record.id.cmp(&right.record.id))
    });

    let mut candidate_results = Vec::new();
    for (index, candidate) in candidates.into_iter().enumerate() {
        candidate_results.push(retrieval_result(index, candidate, include_explanations));
    }
    let (mut knowledge_results, knowledge_omissions) = knowledge_candidates(
        knowledge_snapshots,
        &request,
        &terms,
        include_explanations,
        now,
        service.authorizer.as_ref(),
    )?;
    candidate_results.append(&mut knowledge_results);
    omitted.extend(knowledge_omissions);
    let (mut belief_results, belief_omissions) = belief_candidates(
        belief_snapshots,
        &contradiction_snapshots,
        &request,
        &terms,
        include_explanations,
        now,
        service.authorizer.as_ref(),
    )?;
    candidate_results.append(&mut belief_results);
    omitted.extend(belief_omissions);
    let hierarchy_omissions = expand_hierarchy_memory_candidates(
        &mut candidate_results,
        &expansion_records,
        &hierarchy_nodes,
        &request,
        include_explanations,
        now,
        service.authorizer.as_ref(),
    )?;
    omitted.extend(hierarchy_omissions);
    let (mut external_results, source_failures) =
        external_candidates(&service.retrieval_indexes, &request).await;
    candidate_results.append(&mut external_results);
    apply_hierarchy_context(&mut candidate_results, &hierarchy_nodes, &request);

    compose_context(RetrievalCompositionInput {
        request: &request,
        fusion: service.retrieval_fusion.as_ref(),
        candidates: candidate_results,
        omitted,
        source_failures,
        created_at: now,
    })
}

fn memory_filter_allows(record: &MemoryRecord, filters: Option<&QueryFilter>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    if !filters.memory_kinds.is_empty() && !filters.memory_kinds.contains(&record.kind) {
        return false;
    }
    if let Some(since) = filters.since
        && record.created_at < since
    {
        return false;
    }
    if let Some(until) = filters.until
        && record.created_at > until
    {
        return false;
    }
    if let Some(min_confidence) = filters.min_confidence
        && record.provenance.confidence.unwrap_or(0.0) < min_confidence
    {
        return false;
    }
    true
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let term = term.trim().to_lowercase();
            (!term.is_empty()).then_some(term)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn keyword_score(
    record: &MemoryRecord,
    query: &str,
    terms: &[String],
) -> Option<(f32, Vec<String>)> {
    let content = searchable_content(record);
    let normalized_query = query.trim().to_lowercase();
    let exact_match = content.contains(&normalized_query);
    let matched_terms = terms
        .iter()
        .filter(|term| content.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if !exact_match && matched_terms.is_empty() {
        return None;
    }

    let term_score = if terms.is_empty() {
        0.0
    } else {
        matched_terms.len() as f32 / terms.len() as f32
    };
    let relevance = if exact_match {
        1.0_f32.max(term_score)
    } else {
        term_score
    };
    let confidence = record.provenance.confidence.unwrap_or(1.0);
    let total = ((relevance * 0.85) + (confidence * 0.15)).min(1.0);

    Some((total, matched_terms))
}

fn searchable_content(record: &MemoryRecord) -> String {
    let mut content = record.content.text.to_lowercase();
    if let Some(summary) = &record.content.summary {
        content.push(' ');
        content.push_str(&summary.to_lowercase());
    }
    content
}

fn retrieval_result(
    index: usize,
    candidate: MemoryCandidate,
    include_explanation: bool,
) -> RetrievalResult {
    let MemoryCandidate {
        total,
        relevance,
        recency,
        cue_match,
        matched_terms,
        matched_cues,
        record,
    } = candidate;
    let explanation = include_explanation.then(|| RetrievalExplanation {
        reason: "Matched memory content with in-memory retrieval.".to_owned(),
        matched_cues,
        matched_terms,
        path: Vec::new(),
        source_summary: record.content.summary.clone(),
    });
    let mut matched_sources = Vec::new();
    if relevance.is_some() {
        matched_sources.push("memory.keyword");
    }
    if recency.is_some() {
        matched_sources.push("memory.temporal");
    }
    if cue_match.is_some() {
        matched_sources.push("memory.cue");
    }
    let source = if matched_sources.is_empty() {
        "memory.keyword".to_owned()
    } else {
        matched_sources.join("+")
    };
    RetrievalResult {
        id: format!("result-{}", record.id),
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        content: record.content.text,
        score: RetrievalScore {
            total,
            relevance,
            recency,
            confidence: record.provenance.confidence,
            cue_match,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: record.provenance,
        policy: record.policy,
        explanation,
        fusion_trace: Some(FusionTrace {
            source,
            source_rank: Some((index + 1) as u32),
            source_score: Some(total),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(total),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(total),
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

struct MemoryCandidate {
    total: f32,
    relevance: Option<f32>,
    recency: Option<f32>,
    cue_match: Option<f32>,
    matched_terms: Vec<String>,
    matched_cues: Vec<Cue>,
    record: MemoryRecord,
}

/// Recency score for temporal retrieval within the requested time window.
///
/// The memory is already in-window (`memory_filter_allows` applied `since`/`until`).
/// With a full window the score ramps linearly from the `since` edge (0.0) to the
/// `until` edge (1.0); without a full window every in-window memory scores 1.0 and
/// ordering falls back to `created_at`.
fn temporal_score(
    record: &MemoryRecord,
    filters: Option<&QueryFilter>,
    now: Timestamp,
) -> Option<f32> {
    // Window-relative recency when both edges are set (0.0 at `since`, 1.0 at `until`).
    if let Some(filters) = filters
        && let (Some(since), Some(until)) = (filters.since, filters.until)
        && until > since
    {
        let span = (until - since).num_seconds().max(1) as f32;
        let offset = (record.created_at - since).num_seconds().max(0) as f32;
        return Some((offset / span).clamp(0.0, 1.0));
    }
    // Otherwise age-based decay from `now` so newer memories rank higher even
    // without an explicit window.
    let age_seconds = (now - record.created_at).num_seconds().max(0) as f32;
    Some(1.0 / (1.0 + age_seconds))
}

/// Weighted cue-match ratio for cue retrieval against the memory's links.
///
/// A cue matches a link whose `rel == cue.slot` and whose `target_id` satisfies
/// the cue operator. Returns the matched-weight fraction and the matched cues, or
/// `None` when no cue matches.
fn cue_score(record: &MemoryRecord, cues: &[Cue]) -> Option<(f32, Vec<Cue>)> {
    let total_weight: f32 = cues
        .iter()
        .map(|cue| cue.weight.unwrap_or(1.0).max(0.0))
        .sum();
    if total_weight <= 0.0 {
        return None;
    }
    let mut matched = Vec::new();
    let mut matched_weight = 0.0_f32;
    for cue in cues {
        if cue_matches_link(record, cue) {
            matched.push(cue.clone());
            matched_weight += cue.weight.unwrap_or(1.0).max(0.0);
        }
    }
    if matched.is_empty() {
        return None;
    }
    Some((matched_weight / total_weight, matched))
}

fn cue_matches_link(record: &MemoryRecord, cue: &Cue) -> bool {
    let operator = cue.operator.clone().unwrap_or(CueOperator::Equals);
    record
        .links
        .iter()
        .any(|link| link.rel == cue.slot && cue_satisfies(&operator, &link.target_id, &cue.value))
}

fn cue_satisfies(operator: &CueOperator, target_id: &str, value: &Scalar) -> bool {
    let value_str = value.as_str();
    match operator {
        CueOperator::Equals => value_str.is_some_and(|value| target_id == value),
        CueOperator::Contains => value_str.is_some_and(|value| target_id.contains(value)),
        CueOperator::StartsWith => value_str.is_some_and(|value| target_id.starts_with(value)),
        CueOperator::EndsWith => value_str.is_some_and(|value| target_id.ends_with(value)),
        CueOperator::Exists => true,
        CueOperator::In => value
            .as_array()
            .is_some_and(|array| array.iter().any(|item| item.as_str() == Some(target_id))),
        CueOperator::Range => {
            if let Some(array) = value.as_array()
                && let (Some(lower), Some(upper)) = (
                    array.first().and_then(|item| item.as_str()),
                    array.get(1).and_then(|item| item.as_str()),
                )
            {
                target_id >= lower && target_id <= upper
            } else {
                false
            }
        }
    }
}

fn omitted_result(record: &MemoryRecord, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        reason,
    }
}
