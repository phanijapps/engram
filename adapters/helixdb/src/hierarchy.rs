//! Scope-confined hierarchy storage with canonical navigation.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_hierarchy::HierarchyRepository;
use engram_runtime::{CoreError, CoreResult};
#[async_trait]
impl HierarchyRepository for HelixStore {
    async fn put_node(&self, v: HierarchyNode) -> CoreResult<HierarchyNode> {
        self.check_scope(&v.scope).map_err(core)?;
        self.domain_atomic(|s| {
            let mut nodes = s.all::<HierarchyNode>(HIERARCHY_NODE).map_err(core)?;
            nodes.retain(|n| n.id != v.id);
            nodes.push(v.clone());
            engram_hierarchy::validate_hierarchy_parentage(&nodes)?;
            s.put(HIERARCHY_NODE, &v).map_err(core)
        })
    }
    async fn put_relation(&self, v: HierarchyRelation) -> CoreResult<HierarchyRelation> {
        self.check_scope(&v.scope).map_err(core)?;
        self.domain_atomic(|s| {
            for id in [&v.source_id, &v.target_id] {
                if s.get::<HierarchyNode>(HIERARCHY_NODE, id.as_str())
                    .map_err(core)?
                    .is_none()
                {
                    return Err(CoreError::InvalidRequest {
                        reason: "hierarchy relation endpoint missing in partition".into(),
                    });
                }
            }
            s.put(HIERARCHY_RELATION, &v).map_err(core)
        })
    }
    async fn path_for(
        &self,
        seeds: &[String],
        scope: &Scope,
        max: Option<u32>,
    ) -> CoreResult<HierarchyPath> {
        if !self.visible_scope(scope) {
            return Ok(engram_hierarchy::navigation::navigate(&[], &[], seeds, max));
        }
        let nodes = self.all(HIERARCHY_NODE).map_err(core)?;
        let relations = self.all(HIERARCHY_RELATION).map_err(core)?;
        Ok(engram_hierarchy::navigation::navigate(
            &nodes, &relations, seeds, max,
        ))
    }
}
