//! Hierarchy context annotation for in-memory retrieval.
//!
//! This module uses already-materialized hierarchy nodes to explain matching
//! retrieval results. It does not create hierarchy records, expand candidates,
//! or infer aggregate summaries.

use std::collections::{BTreeMap, BTreeSet};

use engram_core::{CoreError, CoreResult, PolicyAuthorizer};
use engram_domain::*;

use crate::scope::scope_allows;

/// Adds hierarchy path context to retrieval results when hierarchical mode is
/// requested.
pub(crate) fn apply_hierarchy_context(
    results: &mut [RetrievalResult],
    nodes: &[HierarchyNode],
    request: &RetrievalRequest,
) {
    if !request.modes.contains(&RetrievalMode::Hierarchical) {
        return;
    }

    let visible_nodes = visible_nodes(nodes, &request.scope);
    if visible_nodes.is_empty() {
        return;
    }

    let base_nodes = visible_nodes
        .values()
        .filter(|node| node.kind == HierarchyNodeKind::Base)
        .filter_map(|node| {
            let target_type = node.source_target_type.as_ref()?;
            let target_id = node.source_target_id.as_ref()?;
            Some((
                (
                    retrieval_target_label(target_type).to_owned(),
                    target_id.clone(),
                ),
                node.id.to_string(),
            ))
        })
        .collect::<BTreeMap<_, _>>();

    for result in results {
        let key = (
            retrieval_target_label(&result.target_type).to_owned(),
            result.target_id.clone(),
        );
        let Some(base_node_id) = base_nodes.get(&key) else {
            continue;
        };
        let path = parent_path(base_node_id, &visible_nodes);
        if path.is_empty() {
            continue;
        }
        result.score.hierarchical_fit = Some(1.0);
        if let Some(explanation) = &mut result.explanation {
            explanation.path = path;
            explanation.reason = format!("{} Hierarchy context attached.", explanation.reason);
        }
    }
}

/// Adds hierarchy-expanded memory candidates for sibling base nodes.
///
/// Expansion is intentionally one level: matched memory base node -> parent ->
/// sibling memory base nodes. It does not walk arbitrary graph relations or
/// synthesize aggregate candidates.
pub(crate) fn expand_hierarchy_memory_candidates(
    results: &mut Vec<RetrievalResult>,
    records: &[MemoryRecord],
    nodes: &[HierarchyNode],
    request: &RetrievalRequest,
    include_explanations: bool,
    now: Timestamp,
    authorizer: &dyn PolicyAuthorizer,
) -> CoreResult<Vec<OmittedResult>> {
    if !request.modes.contains(&RetrievalMode::Hierarchical) {
        return Ok(Vec::new());
    }

    let visible_nodes = visible_nodes(nodes, &request.scope);
    if visible_nodes.is_empty() {
        return Ok(Vec::new());
    }

    let base_nodes_by_memory = memory_base_nodes(&visible_nodes);
    let seed_parent_ids = results
        .iter()
        .filter(|result| result.target_type == RetrievalTargetType::Memory)
        .filter_map(|result| base_nodes_by_memory.get(&result.target_id))
        .filter_map(|node| node.parent_id.as_ref().map(ToString::to_string))
        .collect::<BTreeSet<_>>();
    if seed_parent_ids.is_empty() {
        return Ok(Vec::new());
    }

    let records_by_id = records
        .iter()
        .cloned()
        .map(|record| (record.id.to_string(), record))
        .collect::<BTreeMap<_, _>>();
    let mut known_memory_ids = results
        .iter()
        .filter(|result| result.target_type == RetrievalTargetType::Memory)
        .map(|result| result.target_id.clone())
        .collect::<BTreeSet<_>>();
    let mut omitted = Vec::new();
    let mut expanded = Vec::new();

    for (memory_id, node) in base_nodes_by_memory {
        if !node
            .parent_id
            .as_ref()
            .is_some_and(|parent_id| seed_parent_ids.contains(parent_id.as_str()))
        {
            continue;
        }
        if !known_memory_ids.insert(memory_id.clone()) {
            continue;
        }
        let Some(record) = records_by_id.get(&memory_id) else {
            continue;
        };
        if let Some(skip) = memory_omission(record, request, now, authorizer)? {
            if let ExpansionSkip::Omitted(reason) = skip {
                omitted.push(omitted_memory(record, reason));
            }
            continue;
        }
        expanded.push(expanded_memory_result(
            expanded.len(),
            record.clone(),
            &node,
            &visible_nodes,
            include_explanations,
        ));
    }

    results.append(&mut expanded);
    Ok(omitted)
}

fn visible_nodes(nodes: &[HierarchyNode], scope: &Scope) -> BTreeMap<String, HierarchyNode> {
    nodes
        .iter()
        .filter(|node| {
            scope_allows(&node.scope, scope) && node.status == HierarchyNodeStatus::Active
        })
        .cloned()
        .map(|node| (node.id.to_string(), node))
        .collect()
}

fn memory_base_nodes(
    visible_nodes: &BTreeMap<String, HierarchyNode>,
) -> BTreeMap<String, HierarchyNode> {
    visible_nodes
        .values()
        .filter(|node| node.kind == HierarchyNodeKind::Base)
        .filter(|node| node.source_target_type == Some(RetrievalTargetType::Memory))
        .filter_map(|node| Some((node.source_target_id.as_ref()?.clone(), node.clone())))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExpansionSkip {
    Filtered,
    Omitted(OmittedReason),
}

fn memory_omission(
    record: &MemoryRecord,
    request: &RetrievalRequest,
    now: Timestamp,
    authorizer: &dyn PolicyAuthorizer,
) -> CoreResult<Option<ExpansionSkip>> {
    if !scope_allows(&record.scope, &request.scope) {
        return Ok(Some(ExpansionSkip::Filtered));
    }
    if !memory_filter_allows(record, request.filters.as_ref()) {
        return Ok(Some(ExpansionSkip::Filtered));
    }
    if let Some(expires_at) = record.policy.expires_at
        && expires_at <= now
    {
        return Ok(Some(ExpansionSkip::Omitted(OmittedReason::Expired)));
    }
    if matches!(
        record.status,
        MemoryStatus::Redacted | MemoryStatus::Forgotten
    ) {
        return Ok(Some(ExpansionSkip::Omitted(OmittedReason::Redacted)));
    }
    if matches!(record.status, MemoryStatus::Archived)
        && !request
            .filters
            .as_ref()
            .and_then(|filters| filters.include_archived)
            .unwrap_or(false)
    {
        return Ok(Some(ExpansionSkip::Filtered));
    }
    if !record.policy.allowed_uses.is_empty()
        && !record.policy.allowed_uses.contains(&AllowedUse::Retrieval)
    {
        return Ok(Some(ExpansionSkip::Omitted(OmittedReason::PolicyDenied)));
    }
    if let Err(error) = authorizer.can_retrieve(&request.requester, &record.scope, &record.policy) {
        if matches!(error, CoreError::PolicyDenied { .. }) {
            return Ok(Some(ExpansionSkip::Omitted(OmittedReason::PolicyDenied)));
        }
        return Err(error);
    }

    Ok(None)
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

fn expanded_memory_result(
    index: usize,
    record: MemoryRecord,
    node: &HierarchyNode,
    visible_nodes: &BTreeMap<String, HierarchyNode>,
    include_explanation: bool,
) -> RetrievalResult {
    let confidence = record.provenance.confidence.unwrap_or(1.0);
    let total_score = ((confidence * 0.25) + 0.35).min(0.75);
    let path = parent_path(node.id.as_str(), visible_nodes);
    let explanation = include_explanation.then(|| RetrievalExplanation {
        reason: "Expanded related memory through hierarchy sibling context.".to_owned(),
        matched_cues: Vec::new(),
        matched_terms: Vec::new(),
        path,
        source_summary: record.content.summary.clone(),
    });
    RetrievalResult {
        id: format!("result-{}-hierarchy-expansion", record.id),
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        content: record.content.text,
        score: RetrievalScore {
            total: total_score,
            relevance: Some(0.25),
            recency: None,
            confidence: record.provenance.confidence,
            cue_match: None,
            hierarchical_fit: Some(1.0),
            policy_fit: Some(1.0),
        },
        provenance: record.provenance,
        policy: record.policy,
        explanation,
        fusion_trace: Some(FusionTrace {
            source: "hierarchy.expansion".to_owned(),
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

fn omitted_memory(record: &MemoryRecord, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        reason,
    }
}

fn parent_path(seed_id: &str, nodes: &BTreeMap<String, HierarchyNode>) -> Vec<String> {
    let mut path = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current_id = Some(seed_id.to_owned());

    while let Some(node_id) = current_id {
        if !visited.insert(node_id.clone()) {
            break;
        }
        let Some(node) = nodes.get(&node_id) else {
            break;
        };
        path.push(format!("{}:{}", node.kind_label(), node.name));
        current_id = node.parent_id.as_ref().map(ToString::to_string);
    }

    path
}

fn retrieval_target_label(target_type: &RetrievalTargetType) -> &'static str {
    match target_type {
        RetrievalTargetType::Memory => "memory",
        RetrievalTargetType::Event => "event",
        RetrievalTargetType::Chunk => "chunk",
        RetrievalTargetType::Document => "document",
        RetrievalTargetType::Entity => "entity",
        RetrievalTargetType::Relationship => "relationship",
        RetrievalTargetType::Concept => "concept",
        RetrievalTargetType::Belief => "belief",
        RetrievalTargetType::Contradiction => "contradiction",
        RetrievalTargetType::HierarchyNode => "hierarchy_node",
        RetrievalTargetType::HierarchyRelation => "hierarchy_relation",
    }
}

trait HierarchyNodeKindLabel {
    fn kind_label(&self) -> &'static str;
}

impl HierarchyNodeKindLabel for HierarchyNode {
    fn kind_label(&self) -> &'static str {
        match &self.kind {
            HierarchyNodeKind::Base => "base",
            HierarchyNodeKind::Aggregate => "aggregate",
            HierarchyNodeKind::Schema => "schema",
            HierarchyNodeKind::Topic => "topic",
            HierarchyNodeKind::Cluster => "cluster",
            HierarchyNodeKind::Domain => "domain",
        }
    }
}
