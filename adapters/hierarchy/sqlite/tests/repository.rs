//! Durable hierarchy repository: round-trip, parent-chain navigation, and
//! scope isolation for the SQLite adapter.

use chrono::{TimeZone, Utc};
use engram_domain::*;
use engram_hierarchy::HierarchyRepository;
use engram_store_hierarchy_sqlite::SqlHierarchyStore;
use futures::executor::block_on;

fn now() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-1"),
        kind: ActorKind::System,
        display_name: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "hierarchy-test".to_owned(),
        actor: actor(),
        observed_at: now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn node(id: &str, layer: u32, parent: Option<&str>, tenant: &str) -> HierarchyNode {
    HierarchyNode {
        id: HierarchyNodeId::from(id),
        scope: scope(tenant),
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
        created_at: now(),
        updated_at: None,
        metadata: None,
    }
}

fn relation(id: &str, source: &str, target: &str, tenant: &str) -> HierarchyRelation {
    HierarchyRelation {
        id: id.to_owned(),
        scope: scope(tenant),
        source_id: HierarchyNodeId::from(source),
        target_id: HierarchyNodeId::from(target),
        predicate: "parent_of".to_owned(),
        layer: None,
        strength: None,
        is_inter_cluster: None,
        evidence: Vec::new(),
        provenance: provenance(),
        created_at: now(),
    }
}

#[test]
fn navigates_parent_chain_with_relations_and_lca() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    block_on(store.put_node(node("a", 0, None, "t1"))).expect("put a");
    block_on(store.put_node(node("b", 1, Some("a"), "t1"))).expect("put b");
    block_on(store.put_node(node("c", 2, Some("b"), "t1"))).expect("put c");
    block_on(store.put_relation(relation("r1", "a", "b", "t1"))).expect("put r1");
    block_on(store.put_relation(relation("r2", "b", "c", "t1"))).expect("put r2");

    let path = block_on(store.path_for(&["c".to_string()], &scope("t1"), None)).expect("path");
    assert_eq!(path.seed_ids, vec!["c".to_string()]);
    assert_eq!(path.nodes.len(), 3, "full parent chain c -> b -> a");
    assert_eq!(
        path.nodes[0].id,
        HierarchyNodeId::from("a"),
        "sorted by layer ascending"
    );
    assert_eq!(path.nodes[2].id, HierarchyNodeId::from("c"));
    assert_eq!(
        path.lca_id,
        Some(HierarchyNodeId::from("c")),
        "single-seed lca is the seed"
    );
    assert_eq!(
        path.relations.len(),
        2,
        "both relations have endpoints in the path"
    );
}

#[test]
fn max_layer_caps_navigation() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    block_on(store.put_node(node("a", 0, None, "t1"))).expect("put a");
    block_on(store.put_node(node("b", 1, Some("a"), "t1"))).expect("put b");
    block_on(store.put_node(node("c", 2, Some("b"), "t1"))).expect("put c");

    // Seed c (layer 2) exceeds max_layer=1 -> not found -> empty path.
    let capped =
        block_on(store.path_for(&["c".to_string()], &scope("t1"), Some(1))).expect("capped");
    assert!(capped.nodes.is_empty(), "seed above max_layer is not found");

    // Seed b (layer 1) is within max_layer=1 -> chain [b, a].
    let from_b =
        block_on(store.path_for(&["b".to_string()], &scope("t1"), Some(1))).expect("from b");
    assert_eq!(from_b.nodes.len(), 2);
    assert_eq!(from_b.nodes[0].id, HierarchyNodeId::from("a"));
    assert_eq!(from_b.nodes[1].id, HierarchyNodeId::from("b"));
}

#[test]
fn put_node_upserts_idempotently() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    block_on(store.put_node(node("a", 0, None, "t1"))).expect("put a");
    let mut updated = node("a", 0, None, "t1");
    updated.name = "renamed".to_owned();
    block_on(store.put_node(updated)).expect("upsert a");

    let path = block_on(store.path_for(&["a".to_string()], &scope("t1"), None)).expect("path");
    assert_eq!(path.nodes.len(), 1, "upsert did not duplicate");
    assert_eq!(path.nodes[0].name, "renamed");
}

#[test]
fn path_is_scope_isolated() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    block_on(store.put_node(node("a", 0, None, "t1"))).expect("put a t1");
    block_on(store.put_node(node("x", 0, None, "t2"))).expect("put x t2");

    let path = block_on(store.path_for(&["x".to_string()], &scope("t1"), None)).expect("path");
    assert!(path.nodes.is_empty(), "cross-tenant node is not visible");

    let own = block_on(store.path_for(&["x".to_string()], &scope("t2"), None)).expect("own");
    assert_eq!(own.nodes.len(), 1, "same-tenant node is visible");
}

#[test]
fn seeds_by_source_target_id() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    let mut base = node("a", 0, None, "t1");
    base.source_target_id = Some("memory-42".to_owned());
    block_on(store.put_node(base)).expect("put a");

    let path =
        block_on(store.path_for(&["memory-42".to_string()], &scope("t1"), None)).expect("path");
    assert_eq!(path.nodes.len(), 1, "seed resolves via source_target_id");
    assert_eq!(path.nodes[0].id, HierarchyNodeId::from("a"));
}

#[test]
fn lca_across_two_seeds_is_the_shared_root() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    block_on(store.put_node(node("a", 0, None, "t1"))).expect("put a (root)");
    block_on(store.put_node(node("b", 1, Some("a"), "t1"))).expect("put b");
    block_on(store.put_node(node("c", 2, Some("b"), "t1"))).expect("put c");
    block_on(store.put_node(node("d", 1, Some("a"), "t1"))).expect("put d");
    block_on(store.put_node(node("e", 2, Some("d"), "t1"))).expect("put e");

    let path = block_on(store.path_for(&["c".to_string(), "e".to_string()], &scope("t1"), None))
        .expect("path");
    assert_eq!(
        path.lca_id,
        Some(HierarchyNodeId::from("a")),
        "LCA across the two branches is the shared root"
    );
    assert_eq!(path.nodes.len(), 5, "all ancestors of both seeds included");
}

#[test]
fn relation_filtered_by_own_scope() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    block_on(store.put_node(node("a", 0, None, "t1"))).expect("put a");
    block_on(store.put_node(node("b", 1, Some("a"), "t1"))).expect("put b");
    // Relation between t1 nodes but scoped to t2 -> excluded from a t1 path.
    block_on(store.put_relation(relation("r-cross", "a", "b", "t2"))).expect("put cross");

    let path = block_on(store.path_for(&["b".to_string()], &scope("t1"), None)).expect("path");
    assert_eq!(path.nodes.len(), 2, "t1 nodes a and b are in the path");
    assert!(
        path.relations.is_empty(),
        "t2-scoped relation is excluded by its own scope even with in-path endpoints"
    );
}
