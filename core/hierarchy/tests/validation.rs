use engram_domain::*;
use engram_hierarchy::validate_hierarchy_parentage;

fn now() -> Timestamp {
    "2026-07-02T12:00:00Z".parse().expect("fixed timestamp")
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-hierarchy".to_owned(),
        subject: None,
        workspace: Some("workspace-hierarchy".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-hierarchy"),
        kind: ActorKind::System,
        display_name: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "hierarchy-validation-test".to_owned(),
        actor: actor(),
        observed_at: now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("hierarchy_build".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn node(id: &str, layer: u32, parent: Option<&str>) -> HierarchyNode {
    HierarchyNode {
        id: Id::from(id),
        scope: scope(),
        kind: if layer == 0 {
            HierarchyNodeKind::Base
        } else {
            HierarchyNodeKind::Aggregate
        },
        layer,
        name: id.to_owned(),
        summary: None,
        parent_id: parent.map(Id::from),
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

#[test]
fn accepts_parent_chain_with_lower_layer_parents() {
    let nodes = vec![
        node("root", 0, None),
        node("topic", 1, Some("root")),
        node("episode", 2, Some("topic")),
    ];

    validate_hierarchy_parentage(&nodes).expect("valid hierarchy");
}

#[test]
fn rejects_missing_parent() {
    let nodes = vec![node("child", 1, Some("missing"))];

    assert!(validate_hierarchy_parentage(&nodes).is_err());
}

#[test]
fn rejects_parent_on_same_or_higher_layer() {
    let nodes = vec![node("parent", 2, None), node("child", 1, Some("parent"))];

    assert!(validate_hierarchy_parentage(&nodes).is_err());
}

#[test]
fn rejects_self_parent() {
    let nodes = vec![node("self", 1, Some("self"))];

    assert!(validate_hierarchy_parentage(&nodes).is_err());
}

#[test]
fn rejects_cycles() {
    let nodes = vec![
        node("a", 1, Some("b")),
        node("b", 2, Some("a")),
        node("root", 0, None),
    ];

    assert!(validate_hierarchy_parentage(&nodes).is_err());
}
