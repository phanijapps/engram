//! Entity aggregate hierarchy construction for in-memory consolidation.
//!
//! This module builds deterministic layer-1 aggregates over existing
//! memory-backed base nodes. It deliberately avoids keyword clustering,
//! embeddings, model summaries, and relation inference.

use std::collections::{BTreeMap, BTreeSet};

use engram_core::CoreResult;
use engram_domain::*;
use serde_json::json;

use crate::{
    consolidation::common::{add_counter, memory_ref},
    scope::scope_allows,
    service::InMemoryMemoryService,
};

/// Builds deterministic entity aggregate nodes for scoped memory base nodes.
///
/// The builder groups already-materialized base nodes by the first explicit
/// entity on each source memory. Groups with at least two eligible members get
/// one active layer-1 aggregate node, and each member base node is attached to
/// that aggregate through `parent_id` plus a membership entry. Existing
/// aggregate metadata is used as the idempotency key so repeated consolidation
/// can update missing attachments without creating duplicate aggregate nodes.
pub(super) fn build_entity_aggregates(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    started_at: Timestamp,
    stats: &mut ConsolidationStats,
) -> CoreResult<ConsolidationTaskResult> {
    let now = service.clock.now();
    let mut state = service.lock_state()?;
    let mut skipped_count = 0_u64;
    let mut created_count = 0_u64;
    let mut updated_count = 0_u64;
    let mut groups = BTreeMap::<String, Vec<BaseCandidate>>::new();

    for node in state.hierarchy_nodes.values() {
        if !base_node_candidate(node, &request.scope) {
            continue;
        }
        let Some(memory_id) = node.source_target_id.as_ref() else {
            continue;
        };
        let Some(memory) = state.memories.get(memory_id) else {
            continue;
        };
        if !memory_candidate(memory, &request.scope, started_at) {
            continue;
        }
        let Some(entity) = memory.content.entities.first() else {
            skipped_count += 1;
            continue;
        };
        let Some(group) = AggregateGroup::from_entity(entity) else {
            skipped_count += 1;
            continue;
        };
        groups
            .entry(group.key.clone())
            .or_default()
            .push(BaseCandidate {
                node_id: node.id.clone(),
                memory_id: memory.id.clone(),
                memory_policy: memory.policy.clone(),
                group,
            });
    }

    let existing_aggregates = state
        .hierarchy_nodes
        .values()
        .filter(|node| {
            scope_allows(&node.scope, &request.scope)
                && node.status == HierarchyNodeStatus::Active
                && node.kind == HierarchyNodeKind::Aggregate
        })
        .filter_map(|node| Some((aggregate_key(node)?, node.id.clone())))
        .collect::<BTreeMap<_, _>>();

    let mut aggregate_ids = existing_aggregates;
    let mut attach_actions = Vec::new();
    for (aggregate_key, candidates) in groups {
        if candidates.len() < 2 {
            skipped_count += candidates.len() as u64;
            continue;
        }
        let aggregate_id = if let Some(existing_id) = aggregate_ids.get(&aggregate_key) {
            existing_id.clone()
        } else {
            let aggregate_id = service.ids.new_id("hierarchy-node");
            let aggregate = aggregate_node(
                aggregate_id.clone(),
                request,
                &candidates[0].group,
                &candidates,
                now,
            );
            state
                .hierarchy_nodes
                .insert(aggregate_id.to_string(), aggregate);
            aggregate_ids.insert(aggregate_key, aggregate_id.clone());
            created_count += 1;
            aggregate_id
        };

        for candidate in candidates {
            attach_actions.push((aggregate_id.clone(), candidate));
        }
    }

    let mut memberships = BTreeMap::<String, Vec<HierarchyMembership>>::new();
    let mut events = Vec::new();
    for (aggregate_id, candidate) in attach_actions {
        let Some(base_node) = state.hierarchy_nodes.get_mut(candidate.node_id.as_str()) else {
            continue;
        };
        if base_node.parent_id.as_ref() == Some(&aggregate_id) {
            continue;
        }
        base_node.parent_id = Some(aggregate_id.clone());
        base_node.updated_at = Some(now);
        updated_count += 1;
        memberships
            .entry(aggregate_id.to_string())
            .or_default()
            .push(membership(
                &aggregate_id,
                &candidate,
                updated_count as u32,
                request,
                now,
            ));
        events.push(hierarchy_built_event(
            service,
            request,
            &candidate,
            &aggregate_id,
            now,
        ));
    }

    for (aggregate_id, new_members) in memberships {
        let Some(aggregate) = state.hierarchy_nodes.get_mut(aggregate_id.as_str()) else {
            continue;
        };
        let mut existing_members = aggregate
            .members
            .iter()
            .map(|member| member.member_id.clone())
            .collect::<BTreeSet<_>>();
        for member in new_members {
            if existing_members.insert(member.member_id.clone()) {
                aggregate.members.push(member);
            }
        }
        aggregate.updated_at = Some(now);
    }
    state.events.extend(events);

    add_counter(&mut stats.hierarchy_nodes_created, created_count);
    stats.hierarchy_relations_created = Some(0);
    stats.memories_written = Some(0);
    stats.model_calls = Some(0);

    Ok(ConsolidationTaskResult {
        task: ConsolidationTaskKind::HierarchyBuild,
        status: ConsolidationTaskStatus::Completed,
        started_at,
        completed_at: Some(now),
        items_read: Some(0),
        items_written: Some(created_count),
        items_updated: Some(updated_count),
        items_skipped: Some(skipped_count),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs: Vec::new(),
    })
}

#[derive(Debug, Clone)]
struct AggregateGroup {
    key: String,
    label: String,
}

impl AggregateGroup {
    fn from_entity(entity: &EntityRef) -> Option<Self> {
        let key_source = entity
            .id
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| entity.name.clone())?;
        let key = key_source.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        let label = entity
            .name
            .clone()
            .unwrap_or_else(|| key_source.trim().to_owned());
        Some(Self {
            key: format!("entity:{key}"),
            label,
        })
    }
}

#[derive(Debug, Clone)]
struct BaseCandidate {
    node_id: HierarchyNodeId,
    memory_id: MemoryId,
    memory_policy: Policy,
    group: AggregateGroup,
}

fn base_node_candidate(node: &HierarchyNode, scope: &Scope) -> bool {
    scope_allows(&node.scope, scope)
        && node.status == HierarchyNodeStatus::Active
        && node.kind == HierarchyNodeKind::Base
        && node.source_target_type == Some(RetrievalTargetType::Memory)
}

fn memory_candidate(record: &MemoryRecord, scope: &Scope, now: Timestamp) -> bool {
    scope_allows(&record.scope, scope)
        && record.status == MemoryStatus::Active
        && record
            .policy
            .expires_at
            .is_none_or(|expires_at| expires_at > now)
}

fn aggregate_key(node: &HierarchyNode) -> Option<String> {
    node.metadata
        .as_ref()?
        .get("aggregateKey")?
        .as_str()
        .map(ToOwned::to_owned)
}

fn aggregate_node(
    aggregate_id: HierarchyNodeId,
    request: &ConsolidationRequest,
    group: &AggregateGroup,
    candidates: &[BaseCandidate],
    now: Timestamp,
) -> HierarchyNode {
    HierarchyNode {
        id: aggregate_id,
        scope: request.scope.clone(),
        kind: HierarchyNodeKind::Aggregate,
        layer: 1,
        name: format!("Entity: {}", group.label),
        summary: Some(format!(
            "Aggregate of {} memories about {}.",
            candidates.len(),
            group.label
        )),
        parent_id: None,
        members: Vec::new(),
        source_target_type: None,
        source_target_id: None,
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: candidates[0].memory_policy.clone(),
        provenance: aggregate_provenance(request, candidates, now),
        created_at: now,
        updated_at: None,
        metadata: Some(
            [
                ("aggregateKey".to_owned(), json!(group.key)),
                ("aggregateKind".to_owned(), json!("entity")),
                ("entityLabel".to_owned(), json!(group.label)),
            ]
            .into_iter()
            .collect(),
        ),
    }
}

fn membership(
    aggregate_id: &HierarchyNodeId,
    candidate: &BaseCandidate,
    rank: u32,
    request: &ConsolidationRequest,
    now: Timestamp,
) -> HierarchyMembership {
    HierarchyMembership {
        id: format!("{aggregate_id}:{}", candidate.node_id),
        parent_id: aggregate_id.clone(),
        member_type: HierarchyMemberType::HierarchyNode,
        member_id: candidate.node_id.to_string(),
        weight: Some(1.0),
        rank: Some(rank),
        provenance: aggregate_provenance(request, std::slice::from_ref(candidate), now),
        created_at: now,
    }
}

fn hierarchy_built_event(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    candidate: &BaseCandidate,
    aggregate_id: &HierarchyNodeId,
    now: Timestamp,
) -> MemoryEvent {
    MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::HierarchyBuilt,
        scope: request.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(candidate.memory_id.clone()),
        payload: json!({
            "reason": "memory_entity_aggregate_hierarchy_build",
            "memoryId": candidate.memory_id.to_string(),
            "baseHierarchyNodeId": candidate.node_id.to_string(),
            "aggregateHierarchyNodeId": aggregate_id.to_string(),
            "aggregateKey": candidate.group.key,
        }),
        provenance: aggregate_provenance(request, std::slice::from_ref(candidate), now),
        occurred_at: now,
        recorded_at: now,
    }
}

fn aggregate_provenance(
    request: &ConsolidationRequest,
    candidates: &[BaseCandidate],
    now: Timestamp,
) -> Provenance {
    Provenance {
        source: "consolidation".to_owned(),
        actor: request.requester.actor.clone(),
        observed_at: now,
        evidence: candidates
            .iter()
            .map(|candidate| memory_ref(candidate.memory_id.clone()))
            .collect(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("hierarchy_entity_aggregate_build".to_owned()),
    }
}
