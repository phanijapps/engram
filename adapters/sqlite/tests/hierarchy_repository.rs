//! Durable hierarchy repository: round-trip, parent-chain navigation, and
//! scope isolation for the SQLite adapter.

use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use engram_domain::*;
use engram_hierarchy::HierarchyRepository;
use engram_store_sqlite::SqlHierarchyStore;
use futures::executor::block_on;
use serde_json::json;

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

fn build_config() -> HierarchyBuildConfig {
    HierarchyBuildConfig {
        id: "build-config-layered".to_owned(),
        algorithm: "deterministic-layered-fixture".to_owned(),
        version: "1".to_owned(),
        target_cluster_size: Some(4),
        max_layers: Some(3),
        similarity_metric: Some("none".to_owned()),
        inter_cluster_threshold: None,
        llm_budget: Some(0),
        created_at: now(),
    }
}

fn build_record() -> HierarchyBuildRecord {
    HierarchyBuildRecord {
        id: "build-layered-1".to_owned(),
        scope: scope("t1"),
        config: build_config(),
        status: HierarchyBuildStatus::Completed,
        input_refs: vec![EvidenceRef {
            target_type: EvidenceTargetType::Event,
            target_id: Some("event-raw-1".to_owned()),
            uri: None,
            quote: None,
            location: None,
        }],
        output_node_ids: vec![
            Id::from("event-raw"),
            Id::from("episode-rollup"),
            Id::from("schema-cluster"),
            Id::from("domain-root"),
        ],
        output_relation_ids: vec![
            "r-domain-schema".to_owned(),
            "r-schema-episode".to_owned(),
            "r-episode-event".to_owned(),
        ],
        stats: Some(BTreeMap::from([
            ("nodesCreated".to_owned(), json!(4)),
            ("relationsCreated".to_owned(), json!(3)),
        ])),
        errors: Vec::new(),
        provenance: provenance(),
        started_at: now(),
        completed_at: Some(now()),
    }
}

fn built_node(
    id: &str,
    kind: HierarchyNodeKind,
    layer: u32,
    parent: Option<&str>,
    source_type: Option<RetrievalTargetType>,
    source_id: Option<&str>,
    build_id: &str,
) -> HierarchyNode {
    let mut node = node(id, layer, parent, "t1");
    node.kind = kind;
    node.summary = Some(format!("{id} summary"));
    node.source_target_type = source_type;
    node.source_target_id = source_id.map(str::to_owned);
    node.provenance.method = Some("hierarchy_build".to_owned());
    node.metadata = Some(BTreeMap::from([(
        "buildRecordId".to_owned(),
        json!(build_id),
    )]));
    node
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
fn layered_build_fixture_is_durable_explainable_and_navigable() {
    let store = SqlHierarchyStore::open_in_memory().expect("open store");
    let build = build_record();
    let nodes = vec![
        built_node(
            "domain-root",
            HierarchyNodeKind::Domain,
            0,
            None,
            None,
            None,
            &build.id,
        ),
        built_node(
            "schema-cluster",
            HierarchyNodeKind::Schema,
            1,
            Some("domain-root"),
            Some(RetrievalTargetType::Concept),
            Some("schema:runtime"),
            &build.id,
        ),
        built_node(
            "episode-rollup",
            HierarchyNodeKind::Topic,
            2,
            Some("schema-cluster"),
            Some(RetrievalTargetType::Memory),
            Some("episode-1"),
            &build.id,
        ),
        built_node(
            "event-raw",
            HierarchyNodeKind::Base,
            3,
            Some("episode-rollup"),
            Some(RetrievalTargetType::Event),
            Some("event-raw-1"),
            &build.id,
        ),
    ];
    for node in nodes {
        block_on(store.put_node(node)).expect("put built node");
    }
    for relation in [
        relation("r-domain-schema", "domain-root", "schema-cluster", "t1"),
        relation("r-schema-episode", "schema-cluster", "episode-rollup", "t1"),
        relation("r-episode-event", "episode-rollup", "event-raw", "t1"),
    ] {
        block_on(store.put_relation(relation)).expect("put build relation");
    }

    let path = block_on(store.path_for(&["event-raw-1".to_string()], &scope("t1"), Some(3)))
        .expect("path");

    assert_eq!(path.nodes.len(), 4);
    assert_eq!(path.relations.len(), 3);
    assert_eq!(path.nodes[0].kind, HierarchyNodeKind::Domain);
    assert_eq!(path.nodes[1].kind, HierarchyNodeKind::Schema);
    assert_eq!(path.nodes[2].kind, HierarchyNodeKind::Topic);
    assert_eq!(path.nodes[3].kind, HierarchyNodeKind::Base);
    for node in &path.nodes {
        assert_eq!(node.provenance.method.as_deref(), Some("hierarchy_build"));
        assert_eq!(
            node.metadata
                .as_ref()
                .and_then(|metadata| metadata.get("buildRecordId")),
            Some(&json!("build-layered-1"))
        );
    }
    assert_eq!(build.output_node_ids.len(), 4);
    assert_eq!(
        build.stats.as_ref().unwrap().get("nodesCreated"),
        Some(&json!(4))
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
