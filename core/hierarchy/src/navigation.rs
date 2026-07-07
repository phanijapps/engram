//! Shared hierarchy path navigation.
//!
//! Pure graph traversal over scope-visible nodes and relations, used by every
//! `HierarchyRepository` adapter (in-memory, SQLite, …) so navigation behavior
//! is identical across backends. Callers scope-filter nodes and relations before
//! invoking [`navigate`].

use std::collections::BTreeSet;

use engram_domain::*;

/// Computes a navigation path from `seed_ids` up through parent chains.
///
/// Seeds resolve by node id or `source_target_id` (within `max_layer`); the path
/// walks `parent_id` chains to the root, computes the lowest common ancestor
/// across chains, and includes relations whose endpoints are both on the path.
/// `nodes` and `relations` must already be scope-filtered by the caller — this
/// function is storage-agnostic and performs no scope checks of its own.
pub fn navigate(
    nodes: &[HierarchyNode],
    relations: &[HierarchyRelation],
    seed_ids: &[String],
    max_layer: Option<u32>,
) -> HierarchyPath {
    let visible: Vec<&HierarchyNode> = nodes.iter().collect();
    let mut chains = Vec::new();
    for seed_id in seed_ids {
        if let Some(seed) = find_seed_node(&visible, seed_id, max_layer) {
            chains.push(parent_chain(&visible, seed, max_layer));
        }
    }

    let lca_id = common_ancestor(&chains);
    let mut included_ids = BTreeSet::new();
    let mut path_nodes = Vec::new();
    for chain in chains {
        for node in chain {
            if included_ids.insert(node.id.to_string()) {
                path_nodes.push(node.clone());
            }
        }
    }
    path_nodes.sort_by(|left, right| {
        left.layer
            .cmp(&right.layer)
            .then_with(|| left.id.cmp(&right.id))
    });

    let path_relations = relations
        .iter()
        .filter(|relation| {
            included_ids.contains(relation.source_id.as_str())
                && included_ids.contains(relation.target_id.as_str())
        })
        .cloned()
        .collect();

    HierarchyPath {
        seed_ids: seed_ids.to_vec(),
        lca_id,
        nodes: path_nodes,
        relations: path_relations,
        max_layer,
    }
}

fn find_seed_node<'a>(
    nodes: &'a [&'a HierarchyNode],
    seed_id: &str,
    max_layer: Option<u32>,
) -> Option<&'a HierarchyNode> {
    nodes
        .iter()
        .copied()
        .filter(|node| max_layer.is_none_or(|limit| node.layer <= limit))
        .find(|node| {
            node.id.as_str() == seed_id || node.source_target_id.as_deref() == Some(seed_id)
        })
}

fn parent_chain<'a>(
    nodes: &'a [&'a HierarchyNode],
    seed: &'a HierarchyNode,
    max_layer: Option<u32>,
) -> Vec<&'a HierarchyNode> {
    let mut chain = Vec::new();
    let mut current = Some(seed);
    while let Some(node) = current {
        if max_layer.is_none_or(|limit| node.layer <= limit) {
            chain.push(node);
        }
        current = node.parent_id.as_ref().and_then(|parent_id| {
            nodes
                .iter()
                .copied()
                .find(|candidate| candidate.id == *parent_id)
        });
    }
    chain
}

fn common_ancestor(chains: &[Vec<&HierarchyNode>]) -> Option<HierarchyNodeId> {
    let first = chains.first()?;
    first.iter().find_map(|candidate| {
        chains
            .iter()
            .all(|chain| chain.iter().any(|node| node.id == candidate.id))
            .then(|| candidate.id.clone())
    })
}
