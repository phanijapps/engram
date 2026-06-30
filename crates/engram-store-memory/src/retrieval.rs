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
    let mut candidates = Vec::new();
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

        if let Some((score, matched_terms)) = keyword_score(&record, &request.query, &terms) {
            candidates.push((score, matched_terms, record));
        }
    }

    candidates.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| right.2.created_at.cmp(&left.2.created_at))
            .then_with(|| left.2.id.cmp(&right.2.id))
    });

    let mut candidate_results = Vec::new();
    for (index, (score, matched_terms, record)) in candidates.into_iter().enumerate() {
        candidate_results.push(retrieval_result(
            index,
            score,
            matched_terms,
            record,
            include_explanations,
        ));
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
    total_score: f32,
    matched_terms: Vec<String>,
    record: MemoryRecord,
    include_explanation: bool,
) -> RetrievalResult {
    let explanation = include_explanation.then(|| RetrievalExplanation {
        reason: "Matched memory content with in-memory keyword retrieval.".to_owned(),
        matched_cues: Vec::new(),
        matched_terms,
        path: Vec::new(),
        source_summary: record.content.summary.clone(),
    });
    RetrievalResult {
        id: format!("result-{}", record.id),
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        content: record.content.text,
        score: RetrievalScore {
            total: total_score,
            relevance: Some(total_score),
            recency: None,
            confidence: record.provenance.confidence,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: record.provenance,
        policy: record.policy,
        explanation,
        fusion_trace: Some(FusionTrace {
            source: "memory.keyword".to_owned(),
            source_rank: Some((index + 1) as u32),
            source_score: Some(total_score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(total_score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(total_score),
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

fn omitted_result(record: &MemoryRecord, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        reason,
    }
}
