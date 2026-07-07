//! Hierarchy capability fixture.
//!
//! Exercises hierarchy node/relation persistence and parent-chain path
//! navigation against the in-memory `SqlHierarchyStore`.

use chrono::Utc;
use engram_domain::*;
use engram_hierarchy::HierarchyRepository;
use engram_runtime::{CoreError, CoreResult};
use engram_store_hierarchy_sqlite::SqlHierarchyStore;
use futures::executor::block_on;

use super::support::{policy, provenance, scope};

/// Runs the hierarchy capability fixture: build a 3-node chain and walk the
/// parent path from the leaf.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if node/relation writes or path navigation fail.
pub fn run_hierarchy_fixture() -> CoreResult<()> {
    let store = SqlHierarchyStore::open_in_memory()?;

    block_on(store.put_node(node("a", 0, None))).map_err(err("put_node"))?;
    block_on(store.put_node(node("b", 1, Some("a")))).map_err(err("put_node"))?;
    block_on(store.put_node(node("c", 2, Some("b")))).map_err(err("put_node"))?;
    block_on(store.put_relation(relation("r1", "a", "b"))).map_err(err("put_relation"))?;
    block_on(store.put_relation(relation("r2", "b", "c"))).map_err(err("put_relation"))?;

    let path = block_on(store.path_for(&["c".to_string()], &scope("tenant-a"), None))
        .map_err(err("path_for"))?;

    if path.nodes.len() != 3 {
        return Err(err("path_for")(CoreError::Conflict {
            reason: format!("expected 3-node parent chain, got {}", path.nodes.len()),
        }));
    }
    // Nodes are returned root-first (a -> b -> c).
    if path.nodes[0].id != HierarchyNodeId::from("a")
        || path.nodes[2].id != HierarchyNodeId::from("c")
    {
        return Err(err("path_for")(CoreError::Conflict {
            reason: "parent chain order incorrect".to_string(),
        }));
    }
    Ok(())
}

fn node(id: &str, layer: u32, parent: Option<&str>) -> HierarchyNode {
    HierarchyNode {
        id: HierarchyNodeId::from(id),
        scope: scope("tenant-a"),
        kind: if layer == 0 {
            HierarchyNodeKind::Base
        } else {
            HierarchyNodeKind::Aggregate
        },
        layer,
        name: id.to_owned(),
        summary: None,
        parent_id: parent.map(HierarchyNodeId::from),
        members: Vec::new(),
        source_target_type: None,
        source_target_id: None,
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn relation(id: &str, source: &str, target: &str) -> HierarchyRelation {
    HierarchyRelation {
        id: id.to_owned(),
        scope: scope("tenant-a"),
        source_id: HierarchyNodeId::from(source),
        target_id: HierarchyNodeId::from(target),
        predicate: "parent_of".to_owned(),
        layer: None,
        strength: None,
        is_inter_cluster: None,
        evidence: Vec::new(),
        provenance: provenance(),
        created_at: Utc::now(),
    }
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.hierarchy".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_fixture_passes() {
        if let Err(e) = run_hierarchy_fixture() {
            panic!("hierarchy fixture failed: {e}");
        }
    }
}
