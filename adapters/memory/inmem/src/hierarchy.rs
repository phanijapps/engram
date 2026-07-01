//! In-memory hierarchy repository and path navigation.
//!
//! This module stores explicit hierarchy nodes and relations, then delegates
//! path navigation to the shared traversal in `engram-hierarchy`. It does not
//! construct, cluster, summarize, or rank hierarchy structures.

use async_trait::async_trait;
use engram_core::{HierarchyRepository, navigation};
use engram_domain::*;
use engram_runtime::CoreResult;

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
        let nodes = state
            .hierarchy_nodes
            .values()
            .filter(|node| scope_allows(&node.scope, scope))
            .cloned()
            .collect::<Vec<_>>();
        let relations = state
            .hierarchy_relations
            .values()
            .filter(|relation| scope_allows(&relation.scope, scope))
            .cloned()
            .collect::<Vec<_>>();
        Ok(navigation::navigate(
            &nodes, &relations, seed_ids, max_layer,
        ))
    }
}
