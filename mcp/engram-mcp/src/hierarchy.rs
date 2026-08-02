//! Hierarchy-surface tools (RFC-0016, Layer 4): build + navigation over the
//! `hierarchy` provider handle.
//!
//! `hierarchy_build` clusters the knowledge graph via Louvain communities,
//! persists cluster nodes (layer 0) with entity members + inter-cluster
//! relations, and makes `hierarchy_path` return results.

use std::collections::HashMap;

use chrono::Utc;
use engram_codegraph_queries::{call_communities, entity_key};
use engram_domain::{
    HierarchyMemberType, HierarchyNode, HierarchyNodeKind, HierarchyNodeStatus, HierarchyRelation,
    Id, RetrievalTargetType,
};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::codegraph::fetch_rels;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, policy, provenance};

/// `hierarchy_build`: cluster the KG via Louvain communities, persist cluster
/// nodes + inter-cluster relations. After this, `hierarchy_path` returns
/// navigation results. Optional `{ max_passes }` (default 3).
pub fn hierarchy_build(app: &App, args: &Value) -> Result<Value, ToolError> {
    let max_passes = args["max_passes"].as_u64().unwrap_or(3) as usize;
    let query = app.provider.require_knowledge_query().map_err(internal)?;
    let entities = block_on(query.list_entities(&app.scope)).unwrap_or_default();
    let rels = fetch_rels(app)?;
    if rels.is_empty() {
        return Ok(protocol::text_content(
            "No relationships in scope — scan_repo first, then build the hierarchy.",
        ));
    }

    // Cluster via Louvain communities on call edges.
    let communities = call_communities(&rels, max_passes);
    if communities.is_empty() {
        return Ok(protocol::text_content("No clusters found (no call edges)."));
    }

    // Group entity names by community id.
    let mut clusters: HashMap<usize, Vec<String>> = HashMap::new();
    for (name, comm) in &communities {
        clusters.entry(*comm).or_default().push(name.clone());
    }

    let hierarchy = app.provider.require_hierarchy().map_err(internal)?;
    let now = Utc::now();
    let scope = app.scope.clone();
    let prov = provenance("mcp-hierarchy-build");
    let pol = policy();

    // Create + persist one HierarchyNode per cluster (layer 0).
    let mut cluster_ids: HashMap<usize, Id> = HashMap::new();
    for (comm_id, members) in &clusters {
        let node_id = Id::from(format!("cluster-{comm_id}"));
        let memberships = members
            .iter()
            .map(|name| engram_domain::HierarchyMembership {
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
        block_on(hierarchy.put_node(node)).map_err(internal)?;
        cluster_ids.insert(*comm_id, node_id);
    }

    // Create inter-cluster relations from cross-community call edges.
    let mut inter_counts: HashMap<(usize, usize), u32> = HashMap::new();
    for r in &rels {
        if r.predicate != "calls" {
            continue;
        }
        let s_comm = entity_key(&r.subject).and_then(|k| communities.get(&k));
        let o_comm = entity_key(&r.object).and_then(|k| communities.get(&k));
        if let (Some(sc), Some(oc)) = (s_comm, o_comm) {
            if sc != oc {
                *inter_counts.entry((*sc, *oc)).or_insert(0) += 1;
            }
        }
    }
    let mut rel_count = 0;
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
        block_on(hierarchy.put_relation(rel)).map_err(internal)?;
        rel_count += 1;
    }

    Ok(protocol::text_content(format!(
        "Hierarchy built: {} cluster nodes (layer 0), {} of {} entities clustered, {} inter-cluster relations. \
         Use hierarchy_path with seed entity names to navigate.",
        clusters.len(),
        communities.len(),
        entities.len(),
        rel_count
    )))
}

/// `hierarchy_path`: navigation path (LCA + nodes + relations) for seed entity
/// ids. Explores the clustered structure above the raw knowledge graph.
pub fn hierarchy_path(app: &App, args: &Value) -> Result<Value, ToolError> {
    let seed_ids: Vec<String> = args["seeds"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if seed_ids.is_empty() {
        return Err(ToolError::new(
            -32602,
            "seeds is required (array of entity ids or names)".to_owned(),
        ));
    }
    let max_layer = args["max_layer"].as_u64().map(|n| n as u32);
    let repo = app.provider.require_hierarchy().map_err(internal)?;
    let path = block_on(repo.path_for(&seed_ids, &app.scope, max_layer)).map_err(internal)?;
    Ok(protocol::text_content(format!(
        "HierarchyPath: {} seed(s), {} node(s), {} relation(s), lca {:?}, max_layer {:?}",
        path.seed_ids.len(),
        path.nodes.len(),
        path.relations.len(),
        path.lca_id,
        path.max_layer,
    )))
}
