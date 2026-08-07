//! Deterministic hierarchy build: cluster the knowledge graph via Louvain
//! communities on call edges and persist layer-0 cluster nodes + inter-cluster
//! relations.
//!
//! Shared by the stdio engram-mcp `hierarchy_build` tool and the N-API
//! `NativeHierarchyApi.buildHierarchyJson` binding. This removes the prior
//! duplication — the cluster→persist logic used to live inline in
//! `mcp/engram-mcp/src/hierarchy.rs`; both surfaces now route through here.
//!
//! The call-edge extraction + entity-key helpers below intentionally duplicate
//! the tiny `engram_codegraph_queries` versions: this core facade must not
//! depend on the on-top codegraph layer (AGENTS.md layering rule). The
//! duplication is two ~10-line pure functions.
//!
//! Deterministic — no LLM. Louvain runs over `calls` / `sends_request` /
//! `handled_by` edges via `engram_graph_analytics::communities`.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, EntityRef, HierarchyMemberType, HierarchyMembership,
    HierarchyNode, HierarchyNodeKind, HierarchyNodeStatus, HierarchyRelation, Id,
    KnowledgeRelationship, Policy, Provenance, Retention, RetrievalTargetType, Scope, Sensitivity,
    Visibility,
};
use engram_graph_analytics::communities;
use engram_hierarchy::HierarchyRepository;
use engram_runtime::CoreResult;
use serde::Serialize;

use crate::KnowledgeQuery;

/// Predicates treated as directed call edges for community detection.
const CALL_PREDICATES: &[&str] = &["calls", "sends_request", "handled_by"];

/// Outcome stats from a hierarchy build run; callers format user-facing text
/// from these counts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyBuildStats {
    /// Layer-0 cluster nodes created.
    pub cluster_count: usize,
    /// Entities assigned to a cluster.
    pub entities_clustered: usize,
    /// Total entities visible in scope (clustered or not).
    pub total_entities: usize,
    /// All relationships visible in scope (the "scan first" diagnostic uses
    /// this: zero relationships means the scope has not been scanned).
    pub total_relationships: usize,
    /// Inter-cluster `connected_to` relations created.
    pub inter_cluster_relation_count: usize,
}

/// Stable string key for an entity reference: its **name** (what graph queries
/// and callers address symbols by), else its resolved id. Mirrors
/// `engram_codegraph_queries::entity_key` — duplicated here so this core crate
/// does not reach into the on-top codegraph layer.
fn entity_key(reference: &EntityRef) -> Option<String> {
    if let Some(name) = &reference.name {
        return Some(name.clone());
    }
    reference.id.as_ref().map(|id| id.as_str().to_owned())
}

/// Extracts `(caller, callee)` pairs from call-edge relationships. Mirrors
/// `engram_codegraph_queries::call_edges`.
fn call_edges(relationships: &[KnowledgeRelationship]) -> Vec<(String, String)> {
    relationships
        .iter()
        .filter(|r| CALL_PREDICATES.contains(&r.predicate.as_str()))
        .filter_map(|r| {
            let caller = entity_key(&r.subject)?;
            let callee = entity_key(&r.object)?;
            Some((caller, callee))
        })
        .collect()
}

/// Default policy stamped onto built cluster nodes. Matches the `engram-mcp`
/// `policy()` helper: workspace-visible, durable, low-sensitivity,
/// retrieval-allowed, tombstone delete.
fn default_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

/// Provenance stamped onto built nodes/relations, attributing the build to
/// `source` (e.g. `"engram-mcp"`, `"engram-node"`). The method records the
/// algorithm (`louvain-communities`) so a reader can see how a cluster arose.
fn build_provenance(source: &str) -> Provenance {
    Provenance {
        source: source.to_owned(),
        actor: Actor {
            id: Id::from("engram"),
            kind: ActorKind::System,
            display_name: Some(source.to_owned()),
            metadata: None,
        },
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("louvain-communities".to_owned()),
    }
}

/// Clusters a scope's knowledge graph via Louvain communities on call edges and
/// persists one layer-0 `HierarchyNode` per cluster (with `Entity` members) plus
/// `connected_to` inter-cluster relations.
///
/// Deterministic — no LLM. `source` stamps the build provenance (e.g.
/// `"engram-mcp"`, `"engram-node"`). Returns stats describing the build;
/// callers format user-facing text. An empty graph yields zero clusters and
/// zero relations (not an error): callers branch on the stats for messaging.
pub async fn build_hierarchy_from_communities(
    knowledge_query: &Arc<dyn KnowledgeQuery>,
    hierarchy: &Arc<dyn HierarchyRepository>,
    scope: &Scope,
    max_passes: usize,
    source: &str,
) -> CoreResult<HierarchyBuildStats> {
    // Entities are only used for the count message; a listing failure degrades
    // to "0 total entities" rather than failing the whole build (matches the
    // prior stdio-mcp behavior, which used .unwrap_or_default() here).
    let entities = knowledge_query
        .list_entities(scope)
        .await
        .unwrap_or_default();
    let relationships = knowledge_query.list_relationships(scope).await?;
    let total_relationships = relationships.len();

    let edges = call_edges(&relationships);
    let communities = communities(&edges, max_passes);
    if communities.is_empty() {
        return Ok(HierarchyBuildStats {
            cluster_count: 0,
            entities_clustered: 0,
            total_entities: entities.len(),
            total_relationships,
            inter_cluster_relation_count: 0,
        });
    }

    // Group entity names by community id.
    let mut clusters: HashMap<usize, Vec<String>> = HashMap::new();
    for (name, comm) in &communities {
        clusters.entry(*comm).or_default().push(name.clone());
    }

    let now = Utc::now();
    let prov = build_provenance(source);
    let pol = default_policy();

    // One layer-0 cluster node per community, with entity members.
    let mut cluster_ids: HashMap<usize, Id> = HashMap::new();
    for (comm_id, members) in &clusters {
        let node_id = Id::from(format!("cluster-{comm_id}"));
        let memberships = members
            .iter()
            .map(|name| HierarchyMembership {
                id: format!("member-{comm_id}-{name}"),
                parent_id: node_id.clone(),
                member_type: HierarchyMemberType::Entity,
                member_id: name.clone(),
                weight: None,
                rank: None,
                provenance: prov.clone(),
                created_at: now,
            })
            .collect::<Vec<_>>();
        let node = HierarchyNode {
            id: node_id.clone(),
            scope: scope.clone(),
            kind: HierarchyNodeKind::Cluster,
            layer: 0,
            name: format!("Cluster {comm_id}"),
            summary: Some(format!("{} entities", members.len())),
            parent_id: None,
            members: memberships,
            source_target_type: Some(RetrievalTargetType::Entity),
            source_target_id: None,
            embedding_refs: Vec::new(),
            status: HierarchyNodeStatus::Active,
            policy: pol.clone(),
            provenance: prov.clone(),
            created_at: now,
            updated_at: None,
            metadata: None,
        };
        hierarchy.put_node(node).await?;
        cluster_ids.insert(*comm_id, node_id);
    }

    // Inter-cluster `connected_to` relations from cross-community call edges.
    let mut inter_counts: HashMap<(usize, usize), u32> = HashMap::new();
    for r in &relationships {
        if !CALL_PREDICATES.contains(&r.predicate.as_str()) {
            continue;
        }
        let s_comm = entity_key(&r.subject).and_then(|k| communities.get(&k));
        let o_comm = entity_key(&r.object).and_then(|k| communities.get(&k));
        if let (Some(sc), Some(oc)) = (s_comm, o_comm)
            && sc != oc
        {
            *inter_counts.entry((*sc, *oc)).or_insert(0) += 1;
        }
    }
    let mut inter_cluster_relation_count = 0;
    for ((sc, oc), count) in &inter_counts {
        let rel = HierarchyRelation {
            id: format!("hrel-{sc}-{oc}"),
            scope: scope.clone(),
            source_id: cluster_ids[sc].clone(),
            target_id: cluster_ids[oc].clone(),
            predicate: "connected_to".to_owned(),
            layer: Some(0),
            strength: Some(*count as f32),
            is_inter_cluster: Some(true),
            evidence: Vec::new(),
            provenance: prov.clone(),
            created_at: now,
        };
        hierarchy.put_relation(rel).await?;
        inter_cluster_relation_count += 1;
    }

    Ok(HierarchyBuildStats {
        cluster_count: clusters.len(),
        entities_clustered: communities.len(),
        total_entities: entities.len(),
        total_relationships,
        inter_cluster_relation_count,
    })
}
