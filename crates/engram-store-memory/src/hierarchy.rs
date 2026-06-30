//! In-memory hierarchy repository and path navigation.
//!
//! This module stores explicit hierarchy nodes and relations, then navigates
//! parent links. It does not construct, cluster, summarize, or rank hierarchy
//! structures.

use std::collections::BTreeSet;

use async_trait::async_trait;
use engram_core::{CoreResult, HierarchyRepository};
use engram_domain::*;

use crate::{scope::scope_allows, service::InMemoryMemoryService};

#[async_trait]
impl HierarchyRepository for InMemoryMemoryService {
    async fn put_node(&self, node: HierarchyNode) -> CoreResult<HierarchyNode> {
        let mut state = self.lock_state()?;
        state
            .hierarchy_nodes
            .insert(node.id.to_string(), node.clone());
        Ok(node)
    }

    async fn put_relation(&self, relation: HierarchyRelation) -> CoreResult<HierarchyRelation> {
        let mut state = self.lock_state()?;
        state
            .hierarchy_relations
            .insert(relation.id.clone(), relation.clone());
        Ok(relation)
    }

    async fn path_for(
        &self,
        seed_ids: &[String],
        scope: &Scope,
        max_layer: Option<u32>,
    ) -> CoreResult<HierarchyPath> {
        let state = self.lock_state()?;
        let visible_nodes = state
            .hierarchy_nodes
            .values()
            .filter(|node| scope_allows(&node.scope, scope))
            .collect::<Vec<_>>();
        let mut chains = Vec::new();
        for seed_id in seed_ids {
            if let Some(seed) = find_seed_node(&visible_nodes, seed_id, max_layer) {
                chains.push(parent_chain(&visible_nodes, seed, max_layer));
            }
        }

        let lca_id = common_ancestor(&chains);
        let mut included_ids = BTreeSet::new();
        let mut nodes = Vec::new();
        for chain in chains {
            for node in chain {
                if included_ids.insert(node.id.to_string()) {
                    nodes.push(node.clone());
                }
            }
        }

        nodes.sort_by(|left, right| {
            left.layer
                .cmp(&right.layer)
                .then_with(|| left.id.cmp(&right.id))
        });
        let relations = state
            .hierarchy_relations
            .values()
            .filter(|relation| {
                scope_allows(&relation.scope, scope)
                    && included_ids.contains(relation.source_id.as_str())
                    && included_ids.contains(relation.target_id.as_str())
            })
            .cloned()
            .collect();

        Ok(HierarchyPath {
            seed_ids: seed_ids.to_vec(),
            lca_id,
            nodes,
            relations,
            max_layer,
        })
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
