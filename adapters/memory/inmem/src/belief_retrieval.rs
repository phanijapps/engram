//! Belief retrieval candidates for the in-memory adapter.
//!
//! This module turns derived beliefs into retrieval candidates while preserving
//! the contract distinction between source truth and reviewable stance records.

use std::collections::BTreeSet;

use engram_domain::*;
use engram_memory::{CoreError, CoreResult, PolicyAuthorizer};

use crate::scope::scope_allows;

const OPEN_CONTRADICTION_PENALTY: f32 = 0.5;

/// Immutable belief snapshot used by retrieval.
#[derive(Debug, Clone)]
pub(crate) struct BeliefSnapshot {
    pub(crate) belief: Belief,
}

/// Immutable contradiction snapshot used by belief ranking.
#[derive(Debug, Clone)]
pub(crate) struct ContradictionSnapshot {
    pub(crate) contradiction: Contradiction,
}

/// Builds belief retrieval candidates for one request.
///
/// Beliefs are derived state, so this path only returns active, non-stale,
/// scoped, policy-allowed records. Policy denials become omissions so callers
/// can distinguish "not matched" from "not allowed" without leaking content.
pub(crate) fn belief_candidates(
    snapshots: Vec<BeliefSnapshot>,
    contradictions: &[ContradictionSnapshot],
    request: &RetrievalRequest,
    terms: &[String],
    include_explanations: bool,
    now: Timestamp,
    authorizer: &dyn PolicyAuthorizer,
) -> CoreResult<(Vec<RetrievalResult>, Vec<OmittedResult>)> {
    let mut candidates = Vec::new();
    let mut omitted = Vec::new();
    for snapshot in snapshots {
        if !scope_allows(&snapshot.belief.scope, &request.scope) {
            continue;
        }
        if !filters_allow(&snapshot.belief, request.filters.as_ref()) {
            continue;
        }
        if !retrievable_lifecycle(&snapshot.belief) {
            continue;
        }
        if belief_expired(&snapshot.belief, now) {
            omitted.push(omitted_belief(&snapshot.belief, OmittedReason::Expired));
            continue;
        }
        if !retrieval_allowed(&snapshot.belief) {
            omitted.push(omitted_belief(
                &snapshot.belief,
                OmittedReason::PolicyDenied,
            ));
            continue;
        }
        if let Err(error) = authorizer.can_retrieve(
            &request.requester,
            &snapshot.belief.scope,
            &snapshot.belief.policy,
        ) {
            if matches!(error, CoreError::PolicyDenied { .. }) {
                omitted.push(omitted_belief(
                    &snapshot.belief,
                    OmittedReason::PolicyDenied,
                ));
                continue;
            }
            return Err(error);
        }
        if let Some((score, matched_terms)) = keyword_score(&snapshot.belief, &request.query, terms)
        {
            let open_contradictions =
                open_contradiction_ids(&snapshot.belief, contradictions, &request.scope);
            let score = contradiction_adjusted_score(score, &open_contradictions);
            candidates.push((score, matched_terms, open_contradictions, snapshot));
        }
    }

    candidates.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| right.3.belief.created_at.cmp(&left.3.belief.created_at))
            .then_with(|| left.3.belief.id.cmp(&right.3.belief.id))
    });

    let results = candidates
        .into_iter()
        .enumerate()
        .map(
            |(index, (score, matched_terms, open_contradictions, snapshot))| {
                belief_result(
                    index,
                    score,
                    matched_terms,
                    open_contradictions,
                    snapshot,
                    include_explanations,
                )
            },
        )
        .collect();
    Ok((results, omitted))
}

fn open_contradiction_ids(
    belief: &Belief,
    contradictions: &[ContradictionSnapshot],
    request_scope: &Scope,
) -> Vec<String> {
    contradictions
        .iter()
        .filter(|snapshot| {
            snapshot.contradiction.status == ContradictionStatus::Open
                && scope_allows(&snapshot.contradiction.scope, request_scope)
                && contradiction_targets_belief(&snapshot.contradiction, &belief.id)
        })
        .map(|snapshot| snapshot.contradiction.id.to_string())
        .collect()
}

fn contradiction_targets_belief(contradiction: &Contradiction, belief_id: &BeliefId) -> bool {
    contradiction.targets.iter().any(|target| {
        target.target_type == ContradictionTargetType::Belief
            && target.target_id == belief_id.to_string()
    })
}

fn contradiction_adjusted_score(score: f32, open_contradictions: &[String]) -> f32 {
    if open_contradictions.is_empty() {
        score
    } else {
        score * OPEN_CONTRADICTION_PENALTY
    }
}

fn filters_allow(belief: &Belief, filters: Option<&QueryFilter>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    if !filters.entity_ids.is_empty()
        && !belief
            .subject
            .entity_ref
            .as_ref()
            .and_then(|entity| entity.id.as_ref())
            .is_some_and(|entity_id| filters.entity_ids.contains(entity_id))
    {
        return false;
    }
    if !filters.concept_ids.is_empty()
        && !belief
            .subject
            .concept_ref
            .as_ref()
            .and_then(|concept| concept.id.as_ref())
            .is_some_and(|concept_id| filters.concept_ids.contains(concept_id))
    {
        return false;
    }
    if let Some(since) = filters.since
        && belief.created_at < since
    {
        return false;
    }
    if let Some(until) = filters.until
        && belief.created_at > until
    {
        return false;
    }
    if let Some(min_confidence) = filters.min_confidence
        && belief.confidence < min_confidence
    {
        return false;
    }
    true
}

fn retrievable_lifecycle(belief: &Belief) -> bool {
    belief.status == BeliefStatus::Active && belief.stale != Some(true)
}

fn belief_expired(belief: &Belief, now: Timestamp) -> bool {
    [belief.policy.expires_at, belief.valid_until]
        .into_iter()
        .flatten()
        .any(|expires_at| expires_at <= now)
}

fn retrieval_allowed(belief: &Belief) -> bool {
    belief.policy.allowed_uses.is_empty()
        || belief.policy.allowed_uses.contains(&AllowedUse::Retrieval)
}

fn keyword_score(belief: &Belief, query: &str, terms: &[String]) -> Option<(f32, Vec<String>)> {
    let content = searchable_content(belief);
    let normalized_query = query.trim().to_lowercase();
    let exact_match = !normalized_query.is_empty() && content.contains(&normalized_query);
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
    let total = ((relevance * 0.8) + (belief.confidence * 0.2)).min(1.0);

    Some((total, matched_terms))
}

fn searchable_content(belief: &Belief) -> String {
    let mut parts = vec![
        belief.subject.key.to_lowercase(),
        belief.content.to_lowercase(),
    ];
    if let Some(reasoning) = &belief.reasoning {
        parts.push(reasoning.to_lowercase());
    }
    parts.extend(
        belief
            .subject
            .aliases
            .iter()
            .map(|alias| alias.to_lowercase()),
    );
    parts
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(" ")
}

fn belief_result(
    index: usize,
    total_score: f32,
    matched_terms: Vec<String>,
    open_contradictions: Vec<String>,
    snapshot: BeliefSnapshot,
    include_explanation: bool,
) -> RetrievalResult {
    let has_open_contradictions = !open_contradictions.is_empty();
    let explanation = include_explanation.then(|| RetrievalExplanation {
        reason: belief_explanation_reason(has_open_contradictions),
        matched_cues: Vec::new(),
        matched_terms,
        path: Vec::new(),
        source_summary: belief_source_summary(&snapshot.belief, &open_contradictions),
    });
    RetrievalResult {
        id: format!("result-{}", snapshot.belief.id),
        target_type: RetrievalTargetType::Belief,
        target_id: snapshot.belief.id.to_string(),
        content: snapshot.belief.content,
        score: RetrievalScore {
            total: total_score,
            relevance: Some(total_score),
            recency: None,
            confidence: Some(snapshot.belief.confidence),
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: snapshot.belief.provenance,
        policy: snapshot.belief.policy,
        explanation,
        fusion_trace: Some(FusionTrace {
            source: "belief.keyword".to_owned(),
            source_rank: Some((index + 1) as u32),
            source_score: Some(total_score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(total_score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(total_score),
            deduplicated_with: Vec::new(),
        }),
        metadata: snapshot.belief.metadata,
    }
}

fn belief_explanation_reason(has_open_contradictions: bool) -> String {
    if has_open_contradictions {
        "Matched derived belief with in-memory keyword retrieval; open contradiction review records reduced ranking.".to_owned()
    } else {
        "Matched derived belief with in-memory keyword retrieval.".to_owned()
    }
}

fn belief_source_summary(belief: &Belief, open_contradictions: &[String]) -> Option<String> {
    if open_contradictions.is_empty() {
        return belief.reasoning.clone();
    }

    let contradiction_summary = format!("open contradictions: {}", open_contradictions.join(","));
    Some(match &belief.reasoning {
        Some(reasoning) if !reasoning.is_empty() => {
            format!("{reasoning}; {contradiction_summary}")
        }
        _ => contradiction_summary,
    })
}

fn omitted_belief(belief: &Belief, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: RetrievalTargetType::Belief,
        target_id: belief.id.to_string(),
        reason,
    }
}
