//! Base hierarchy-node construction for in-memory consolidation.
//!
//! This module owns the deterministic hierarchy substrate builder for memory
//! records. It creates layer-0 base nodes only; aggregate clustering, relation
//! inference, summaries, embeddings, and retrieval expansion remain separate
//! future responsibilities.

use engram_core::CoreResult;
use engram_domain::*;
use serde_json::json;

use crate::{
    consolidation::common::{add_counter, memory_ref},
    scope::scope_allows,
    service::InMemoryMemoryService,
};

/// Creates missing base hierarchy nodes for scoped active memories.
///
/// The task is idempotent for active memory-backed base nodes already present
/// in the in-memory hierarchy state. Each created node is paired with a
/// `HierarchyBuilt` event on the source memory for auditability.
pub(super) fn build_base_nodes(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    started_at: Timestamp,
    stats: &mut ConsolidationStats,
) -> CoreResult<ConsolidationTaskResult> {
    let now = service.clock.now();
    let mut state = service.lock_state()?;
    let mut read_count = 0_u64;
    let mut skipped_count = 0_u64;
    let mut created_memory_ids = Vec::new();

    let existing_memory_targets = state
        .hierarchy_nodes
        .values()
        .filter(|node| {
            scope_allows(&node.scope, &request.scope)
                && node.status == HierarchyNodeStatus::Active
                && node.kind == HierarchyNodeKind::Base
                && node.source_target_type == Some(RetrievalTargetType::Memory)
        })
        .filter_map(|node| node.source_target_id.clone())
        .collect::<std::collections::BTreeSet<_>>();

    let candidates = state
        .memories
        .values()
        .filter_map(|record| {
            if !scope_allows(&record.scope, &request.scope) {
                return None;
            }
            read_count += 1;
            if !hierarchy_candidate(record, started_at)
                || existing_memory_targets.contains(record.id.as_str())
            {
                skipped_count += 1;
                return None;
            }
            Some(record.clone())
        })
        .collect::<Vec<_>>();

    for record in candidates {
        let node_id = service.ids.new_id("hierarchy-node");
        let node = base_node(&record, node_id.clone(), request, now);
        let event = hierarchy_built_event(service, request, &record, &node_id, now);
        state.hierarchy_nodes.insert(node_id.to_string(), node);
        state.events.push(event);
        created_memory_ids.push(record.id);
    }

    let created_count = created_memory_ids.len() as u64;
    add_counter(&mut stats.memories_read, read_count);
    stats.memories_written = Some(0);
    add_counter(&mut stats.hierarchy_nodes_created, created_count);
    stats.hierarchy_relations_created = Some(0);
    stats.model_calls = Some(0);

    Ok(ConsolidationTaskResult {
        task: ConsolidationTaskKind::HierarchyBuild,
        status: ConsolidationTaskStatus::Completed,
        started_at,
        completed_at: Some(now),
        items_read: Some(read_count),
        items_written: Some(created_count),
        items_updated: Some(0),
        items_skipped: Some(skipped_count),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs: created_memory_ids.into_iter().map(memory_ref).collect(),
    })
}

fn hierarchy_candidate(record: &MemoryRecord, now: Timestamp) -> bool {
    if record.status != MemoryStatus::Active {
        return false;
    }
    record
        .policy
        .expires_at
        .is_none_or(|expires_at| expires_at > now)
}

fn base_node(
    record: &MemoryRecord,
    node_id: HierarchyNodeId,
    request: &ConsolidationRequest,
    now: Timestamp,
) -> HierarchyNode {
    HierarchyNode {
        id: node_id,
        scope: record.scope.clone(),
        kind: HierarchyNodeKind::Base,
        layer: 0,
        name: base_node_name(record),
        summary: record.content.summary.clone(),
        parent_id: None,
        members: Vec::new(),
        source_target_type: Some(RetrievalTargetType::Memory),
        source_target_id: Some(record.id.to_string()),
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: record.policy.clone(),
        provenance: hierarchy_provenance(request, record, now),
        created_at: now,
        updated_at: None,
        metadata: None,
    }
}

fn base_node_name(record: &MemoryRecord) -> String {
    let text = record
        .content
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        return format!("Memory {}", record.id);
    }
    text.chars().take(80).collect()
}

fn hierarchy_built_event(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    record: &MemoryRecord,
    node_id: &HierarchyNodeId,
    now: Timestamp,
) -> MemoryEvent {
    MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::HierarchyBuilt,
        scope: record.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(record.id.clone()),
        payload: json!({
            "reason": "memory_base_hierarchy_build",
            "memoryId": record.id.to_string(),
            "hierarchyNodeId": node_id.to_string(),
        }),
        provenance: hierarchy_provenance(request, record, now),
        occurred_at: now,
        recorded_at: now,
    }
}

fn hierarchy_provenance(
    request: &ConsolidationRequest,
    record: &MemoryRecord,
    now: Timestamp,
) -> Provenance {
    Provenance {
        source: "consolidation".to_owned(),
        actor: request.requester.actor.clone(),
        observed_at: now,
        evidence: vec![memory_ref(record.id.clone())],
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("hierarchy_base_build".to_owned()),
    }
}
