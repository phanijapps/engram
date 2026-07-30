//! Hierarchy-surface tool (RFC-0016 P2, Layer 4): navigation / context
//! compression over the `hierarchy` provider handle.
//!
//! Hierarchy nodes are built via consolidation or external tooling (the provider
//! exposes the navigation port, not a builder handle), so `hierarchy_path`
//! returns an empty path until a hierarchy has been built for the scope.

use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::internal;

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
