//! Hierarchy ports for the engram engine.
//!
//! Hierarchy navigation and build behavior contracts that adapters implement.
//! Domain types live in `engram-domain`; this crate owns the ports and the
//! shared navigation traversal adapters reuse.

pub mod navigation;
mod validation;

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

pub use validation::validate_hierarchy_parentage;

/// Persistence and navigation port for hierarchy structures.
///
/// Hierarchy adapters may materialize trees, DAG-like relation sets, or cached
/// paths internally. The public results must still expose explainable nodes,
/// relations, and provenance for navigation and context compression.
#[async_trait]
pub trait HierarchyRepository: Send + Sync {
    /// Stores a hierarchy node from a build or manual curation step.
    async fn put_node(&self, node: HierarchyNode) -> CoreResult<HierarchyNode>;

    /// Stores an explainable relation between hierarchy nodes.
    async fn put_relation(&self, relation: HierarchyRelation) -> CoreResult<HierarchyRelation>;

    /// Finds a navigation path for seed objects without crossing scope boundaries.
    async fn path_for(
        &self,
        seed_ids: &[String],
        scope: &Scope,
        max_layer: Option<u32>,
    ) -> CoreResult<HierarchyPath>;
}

/// Builds hierarchy nodes for navigation and context compression.
///
/// Builders may use clustering, taxonomy, graph structure, or model-assisted
/// summaries internally. Outputs must preserve algorithm provenance and avoid
/// creating multiple parent pointers inside a single tree version.
#[async_trait]
pub trait HierarchyBuilder: Send + Sync {
    /// Builds hierarchy nodes for a scope using a recorded build configuration.
    async fn build_hierarchy(
        &self,
        config: &HierarchyBuildConfig,
        scope: &Scope,
    ) -> CoreResult<Vec<HierarchyNode>>;
}
