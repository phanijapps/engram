use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository};
use engram_store_knowledge_memory::InMemoryKnowledgeStore;
use futures::executor::block_on;

fn actor() -> Actor {
    Actor {
        id: Id::from("agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Graph Agent".to_owned()),
        metadata: None,
    }
}

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "knowledge-graph-test".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

#[test]
fn knowledge_graph_repository_round_trips_scoped_graph() {
    let store = InMemoryKnowledgeStore::new();
    let graph = KnowledgeGraph {
        id: Id::from("graph-1"),
        scope: scope("tenant-a"),
        name: "Code Graph".to_owned(),
        uri: Some("urn:graph:code".to_owned()),
        version: Some("1.0.0".to_owned()),
        ontology_refs: vec![OntologyRef {
            id: Some(Id::from("ontology-1")),
            uri: Some("urn:ontology:code".to_owned()),
            version: Some("1.0.0".to_owned()),
        }],
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    };

    let stored = block_on(store.put_graph(graph.clone())).expect("put graph");
    let visible = block_on(store.get_graph(&graph.id, &scope("tenant-a"))).expect("get graph");
    let hidden = block_on(store.get_graph(&graph.id, &scope("tenant-b"))).expect("get graph");

    assert_eq!(stored.id, graph.id);
    assert_eq!(visible.expect("visible graph").name, "Code Graph");
    assert!(hidden.is_none());
}

#[test]
fn knowledge_graph_neighbors_respect_scope_and_limit() {
    let store = InMemoryKnowledgeStore::new();
    let graph_id = Id::from("graph-1");
    let function_a = Id::from("function-a");
    block_on(store.put_graph(KnowledgeGraph {
        id: graph_id.clone(),
        scope: scope("tenant-a"),
        name: "Code Graph".to_owned(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put graph");

    block_on(store.put_entity(KnowledgeEntity {
        id: function_a.clone(),
        graph_id: Some(graph_id.clone()),
        kind: EntityKind::Function,
        name: "function_a".to_owned(),
        aliases: Vec::new(),
        scope: scope("tenant-a"),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put entity");

    for (relationship_id, object_id) in [("rel-1", "function-b"), ("rel-2", "function-c")] {
        block_on(store.put_relationship(KnowledgeRelationship {
            id: Id::from(relationship_id),
            graph_id: Some(graph_id.clone()),
            subject: EntityRef {
                id: Some(function_a.clone()),
                kind: Some("function".to_owned()),
                name: Some("function_a".to_owned()),
                aliases: Vec::new(),
            },
            predicate: "calls".to_owned(),
            object: EntityRef {
                id: Some(Id::from(object_id)),
                kind: Some("function".to_owned()),
                name: Some(object_id.to_owned()),
                aliases: Vec::new(),
            },
            scope: scope("tenant-a"),
            evidence: Vec::new(),
            confidence: Some(0.9),
            provenance: provenance(),
            created_at: Utc::now(),
            updated_at: None,
        }))
        .expect("put relationship");
    }

    let neighbors = block_on(store.neighbors(&graph_id, &function_a, &scope("tenant-a"), Some(1)))
        .expect("neighbors");
    let hidden = block_on(store.neighbors(&graph_id, &function_a, &scope("tenant-b"), None))
        .expect("hidden neighbors");

    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].id, Id::from("rel-1"));
    assert!(hidden.is_empty());
}

#[test]
fn ontology_repository_validates_visible_graph_and_ontology() {
    let store = InMemoryKnowledgeStore::new();
    let graph_id = Id::from("graph-1");
    let ontology_id = Id::from("ontology-1");

    block_on(store.put_graph(KnowledgeGraph {
        id: graph_id.clone(),
        scope: scope("tenant-a"),
        name: "Code Graph".to_owned(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put graph");
    block_on(store.put_ontology(Ontology {
        id: ontology_id.clone(),
        uri: "urn:ontology:code".to_owned(),
        name: "Code Ontology".to_owned(),
        scope: scope("tenant-a"),
        language: OntologyLanguage::PropertyGraph,
        version: "1.0.0".to_owned(),
        status: OntologyStatus::Active,
        imports: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put ontology");

    let findings = block_on(store.validate_graph(&graph_id, &ontology_id, &scope("tenant-a")))
        .expect("validate graph");
    let hidden =
        block_on(store.get_ontology(&ontology_id, &scope("tenant-b"))).expect("get ontology");

    assert!(findings.is_empty());
    assert!(hidden.is_none());
}
