use engram_core::HierarchyRepository;
use engram_domain::*;
use engram_store_memory::InMemoryMemoryService;
use futures::executor::block_on;

fn scope(workspace: &str) -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
        subject: None,
        workspace: Some(workspace.to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-hierarchy"),
        kind: ActorKind::Agent,
        display_name: Some("Hierarchy Test".to_owned()),
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "hierarchy_repository_test".to_owned(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_owned()),
    }
}

fn node(
    id: &str,
    parent_id: Option<&str>,
    layer: u32,
    source_target_id: Option<&str>,
    workspace: &str,
) -> HierarchyNode {
    HierarchyNode {
        id: Id::from(id),
        scope: scope(workspace),
        kind: if parent_id.is_some() {
            HierarchyNodeKind::Aggregate
        } else {
            HierarchyNodeKind::Domain
        },
        layer,
        name: id.to_owned(),
        summary: None,
        parent_id: parent_id.map(Id::from),
        members: Vec::new(),
        source_target_type: source_target_id.map(|_| RetrievalTargetType::Memory),
        source_target_id: source_target_id.map(str::to_owned),
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

#[test]
fn hierarchy_path_returns_parent_chain_and_common_ancestor() {
    let service = InMemoryMemoryService::new();
    let root = node("node-root", None, 3, None, "engram");
    let topic = node("node-topic", Some("node-root"), 2, None, "engram");
    let first = node(
        "node-first",
        Some("node-topic"),
        1,
        Some("memory-1"),
        "engram",
    );
    let second = node(
        "node-second",
        Some("node-topic"),
        1,
        Some("memory-2"),
        "engram",
    );
    for node in [root.clone(), topic.clone(), first.clone(), second.clone()] {
        block_on(service.put_node(node)).expect("put node");
    }
    block_on(service.put_relation(HierarchyRelation {
        id: "relation-topic-root".to_owned(),
        scope: scope("engram"),
        source_id: topic.id.clone(),
        target_id: root.id.clone(),
        predicate: "broader".to_owned(),
        layer: Some(2),
        strength: Some(1.0),
        is_inter_cluster: Some(false),
        evidence: Vec::new(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
    }))
    .expect("put relation");

    let path = block_on(service.path_for(
        &["memory-1".to_owned(), "memory-2".to_owned()],
        &scope("engram"),
        None,
    ))
    .expect("path");

    assert_eq!(path.lca_id, Some(topic.id));
    assert_eq!(path.nodes.len(), 4);
    assert_eq!(path.relations.len(), 1);
}

#[test]
fn hierarchy_path_respects_scope() {
    let service = InMemoryMemoryService::new();
    block_on(service.put_node(node(
        "node-hidden",
        None,
        1,
        Some("memory-hidden"),
        "hidden",
    )))
    .expect("put hidden node");

    let path = block_on(service.path_for(&["memory-hidden".to_owned()], &scope("engram"), None))
        .expect("path");

    assert!(path.nodes.is_empty());
    assert!(path.lca_id.is_none());
}
