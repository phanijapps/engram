//! Knowledge chunk retrieval for the in-memory adapter.
//!
//! This module turns source-grounded chunks into retrieval candidates. It keeps
//! source, document, and chunk policy checks visible before candidates reach the
//! shared fusion step.

use engram_domain::*;
use engram_memory::{CoreError, CoreResult, PolicyAuthorizer};

use crate::scope::scope_allows;

/// Immutable source/document/chunk snapshot used by retrieval.
#[derive(Debug, Clone)]
pub(crate) struct KnowledgeSnapshot {
    pub(crate) source: KnowledgeSource,
    pub(crate) document: SourceDocument,
    pub(crate) chunk: KnowledgeChunk,
}

/// Builds source-grounded chunk retrieval candidates for one request.
///
/// Invalid snapshot chains are filtered before this function is called. Policy
/// denials become omissions so callers can distinguish "not matched" from "not
/// allowed" without leaking chunk content.
pub(crate) fn knowledge_candidates(
    snapshots: Vec<KnowledgeSnapshot>,
    request: &RetrievalRequest,
    terms: &[String],
    include_explanations: bool,
    now: Timestamp,
    authorizer: &dyn PolicyAuthorizer,
) -> CoreResult<(Vec<RetrievalResult>, Vec<OmittedResult>)> {
    let mut candidates = Vec::new();
    let mut omitted = Vec::new();
    for snapshot in snapshots {
        if !scope_allows(&snapshot.source.scope, &request.scope) {
            continue;
        }
        if !filters_allow(&snapshot, request.filters.as_ref()) {
            continue;
        }
        if policy_expired(&snapshot, now) {
            omitted.push(omitted_chunk(&snapshot.chunk, OmittedReason::Expired));
            continue;
        }
        if !retrieval_allowed(&snapshot) {
            omitted.push(omitted_chunk(&snapshot.chunk, OmittedReason::PolicyDenied));
            continue;
        }
        if let Err(error) = authorize_snapshot(authorizer, request, &snapshot) {
            if matches!(error, CoreError::PolicyDenied { .. }) {
                omitted.push(omitted_chunk(&snapshot.chunk, OmittedReason::PolicyDenied));
                continue;
            }
            return Err(error);
        }
        if let Some((score, matched_terms)) = keyword_score(&snapshot.chunk, &request.query, terms)
        {
            candidates.push((score, matched_terms, snapshot));
        }
    }

    candidates.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| right.2.chunk.created_at.cmp(&left.2.chunk.created_at))
            .then_with(|| left.2.chunk.id.cmp(&right.2.chunk.id))
    });

    let results = candidates
        .into_iter()
        .enumerate()
        .map(|(index, (score, matched_terms, snapshot))| {
            chunk_result(index, score, matched_terms, snapshot, include_explanations)
        })
        .collect();
    Ok((results, omitted))
}

fn filters_allow(snapshot: &KnowledgeSnapshot, filters: Option<&QueryFilter>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    if !filters.source_kinds.is_empty() && !filters.source_kinds.contains(&snapshot.source.kind) {
        return false;
    }
    if !filters.chunk_kinds.is_empty() && !filters.chunk_kinds.contains(&snapshot.chunk.kind) {
        return false;
    }
    if !filters.concept_ids.is_empty()
        && !snapshot
            .chunk
            .concepts
            .iter()
            .filter_map(|concept| concept.id.as_ref())
            .any(|concept_id| filters.concept_ids.contains(concept_id))
    {
        return false;
    }
    if !filters.entity_ids.is_empty()
        && !snapshot
            .chunk
            .entities
            .iter()
            .filter_map(|entity| entity.id.as_ref())
            .any(|entity_id| filters.entity_ids.contains(entity_id))
    {
        return false;
    }
    if let Some(since) = filters.since
        && snapshot.chunk.created_at < since
    {
        return false;
    }
    if let Some(until) = filters.until
        && snapshot.chunk.created_at > until
    {
        return false;
    }
    if let Some(min_confidence) = filters.min_confidence
        && snapshot.chunk.provenance.confidence.unwrap_or(0.0) < min_confidence
    {
        return false;
    }
    true
}

fn policy_expired(snapshot: &KnowledgeSnapshot, now: Timestamp) -> bool {
    [
        snapshot.source.policy.expires_at,
        snapshot.document.policy.expires_at,
        snapshot.chunk.policy.expires_at,
    ]
    .into_iter()
    .flatten()
    .any(|expires_at| expires_at <= now)
}

fn retrieval_allowed(snapshot: &KnowledgeSnapshot) -> bool {
    [
        &snapshot.source.policy,
        &snapshot.document.policy,
        &snapshot.chunk.policy,
    ]
    .into_iter()
    .all(|policy| {
        policy.allowed_uses.is_empty() || policy.allowed_uses.contains(&AllowedUse::Retrieval)
    })
}

fn authorize_snapshot(
    authorizer: &dyn PolicyAuthorizer,
    request: &RetrievalRequest,
    snapshot: &KnowledgeSnapshot,
) -> CoreResult<()> {
    authorizer.can_retrieve(
        &request.requester,
        &snapshot.source.scope,
        &snapshot.source.policy,
    )?;
    authorizer.can_retrieve(
        &request.requester,
        &snapshot.source.scope,
        &snapshot.document.policy,
    )?;
    authorizer.can_retrieve(
        &request.requester,
        &snapshot.source.scope,
        &snapshot.chunk.policy,
    )
}

fn keyword_score(
    chunk: &KnowledgeChunk,
    query: &str,
    terms: &[String],
) -> Option<(f32, Vec<String>)> {
    let content = searchable_content(chunk);
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
    let confidence = chunk.provenance.confidence.unwrap_or(1.0);
    let total = ((relevance * 0.85) + (confidence * 0.15)).min(1.0);

    Some((total, matched_terms))
}

fn searchable_content(chunk: &KnowledgeChunk) -> String {
    let mut content = chunk.text.to_lowercase();
    if let Some(summary) = &chunk.summary {
        content.push(' ');
        content.push_str(&summary.to_lowercase());
    }
    content
}

fn chunk_result(
    index: usize,
    total_score: f32,
    matched_terms: Vec<String>,
    snapshot: KnowledgeSnapshot,
    include_explanation: bool,
) -> RetrievalResult {
    let path = snapshot
        .chunk
        .location
        .as_ref()
        .and_then(|location| location.path.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let explanation = include_explanation.then(|| RetrievalExplanation {
        reason: "Matched source-grounded knowledge chunk with in-memory keyword retrieval."
            .to_owned(),
        matched_cues: Vec::new(),
        matched_terms,
        path,
        source_summary: snapshot.chunk.summary.clone().or(snapshot.document.title),
    });
    RetrievalResult {
        id: format!("result-{}", snapshot.chunk.id),
        target_type: RetrievalTargetType::Chunk,
        target_id: snapshot.chunk.id.to_string(),
        content: snapshot.chunk.text,
        score: RetrievalScore {
            total: total_score,
            relevance: Some(total_score),
            recency: None,
            confidence: snapshot.chunk.provenance.confidence,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: snapshot.chunk.provenance,
        policy: snapshot.chunk.policy,
        explanation,
        fusion_trace: Some(FusionTrace {
            source: "knowledge.keyword".to_owned(),
            source_rank: Some((index + 1) as u32),
            source_score: Some(total_score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(total_score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(total_score),
            deduplicated_with: Vec::new(),
        }),
        metadata: snapshot.chunk.metadata,
    }
}

fn omitted_chunk(chunk: &KnowledgeChunk, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: RetrievalTargetType::Chunk,
        target_id: chunk.id.to_string(),
        reason,
    }
}
