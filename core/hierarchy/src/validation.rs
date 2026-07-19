//! Pure hierarchy construction validation.
//!
//! Builders use these checks before storing nodes so invalid parentage is caught
//! independently of any SQLite, graph, or model-backed implementation.

use std::collections::{BTreeMap, BTreeSet};

use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

/// Validates parent pointers for one hierarchy version.
///
/// The checks enforce the current tree-version contract: a node cannot parent
/// itself, every parent pointer must resolve inside the supplied node set,
/// parents must be on a lower layer than children, and parent chains must not
/// cycle.
pub fn validate_hierarchy_parentage(nodes: &[HierarchyNode]) -> CoreResult<()> {
    let by_id = nodes
        .iter()
        .map(|node| (node.id.to_string(), node))
        .collect::<BTreeMap<_, _>>();

    for node in nodes {
        let Some(parent_id) = &node.parent_id else {
            continue;
        };
        if parent_id == &node.id {
            return invalid_parentage(node, "node cannot be its own parent");
        }
        let Some(parent) = by_id.get(parent_id.as_str()) else {
            return invalid_parentage(node, "parent node is missing from build output");
        };
        if parent.layer >= node.layer {
            return invalid_parentage(node, "parent layer must be lower than child layer");
        }
        detect_cycle(node, &by_id)?;
    }

    Ok(())
}

fn detect_cycle(node: &HierarchyNode, by_id: &BTreeMap<String, &HierarchyNode>) -> CoreResult<()> {
    let mut seen = BTreeSet::new();
    let mut current = Some(node);
    while let Some(candidate) = current {
        if !seen.insert(candidate.id.to_string()) {
            return invalid_parentage(node, "parent chain contains a cycle");
        }
        current = candidate
            .parent_id
            .as_ref()
            .and_then(|parent_id| by_id.get(parent_id.as_str()).copied());
    }
    Ok(())
}

fn invalid_parentage(node: &HierarchyNode, reason: &str) -> CoreResult<()> {
    Err(CoreError::InvalidRequest {
        reason: format!("invalid hierarchy parentage for {}: {reason}", node.id),
    })
}
