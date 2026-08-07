//! Hierarchy-surface tools (RFC-0016, Layer 4): build + navigation over the
//! `hierarchy` provider handle.
//!
//! `hierarchy_build` clusters the knowledge graph via Louvain communities,
//! persists cluster nodes (layer 0) with entity members + inter-cluster
//! relations, and makes `hierarchy_path` return results. The cluster→persist
//! logic lives in `engram_integration::build_hierarchy_from_communities` (shared
//! with the N-API binding); this handler is the MCP-text wrapper over it.

use engram_integration::build_hierarchy_from_communities;
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::internal;

/// `hierarchy_build`: cluster the KG via Louvain communities, persist cluster
/// nodes + inter-cluster relations. After this, `hierarchy_path` returns
/// navigation results. Optional `{ max_passes }` (default 3).
pub fn hierarchy_build(app: &App, args: &Value) -> Result<Value, ToolError> {
    let max_passes = args["max_passes"].as_u64().unwrap_or(3) as usize;
    let knowledge_query = app.provider.require_knowledge_query().map_err(internal)?;
    let hierarchy = app.provider.require_hierarchy().map_err(internal)?;
    let stats = block_on(build_hierarchy_from_communities(
        knowledge_query,
        hierarchy,
        &app.scope,
        max_passes,
        "engram-mcp",
    ))
    .map_err(internal)?;

    if stats.total_relationships == 0 {
        return Ok(protocol::text_content(
            "No relationships in scope — scan_repo first, then build the hierarchy.",
        ));
    }
    if stats.cluster_count == 0 {
        return Ok(protocol::text_content("No clusters found (no call edges)."));
    }

    Ok(protocol::text_content(format!(
        "Hierarchy built: {} cluster nodes (layer 0), {} of {} entities clustered, {} inter-cluster relations. \
         Use hierarchy_path with seed entity names to navigate.",
        stats.cluster_count,
        stats.entities_clustered,
        stats.total_entities,
        stats.inter_cluster_relation_count,
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
