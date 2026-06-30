use chrono::Utc;
use engram_domain::*;
use serde_json::json;

fn actor() -> Actor {
    Actor {
        id: Id::from("agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Ontology Agent".to_owned()),
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: Some("project-a".to_owned()),
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

fn provenance(now: Timestamp) -> Provenance {
    Provenance {
        source: "ontology-test".to_owned(),
        actor: actor(),
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

#[test]
fn ontology_records_use_contract_json_names() {
    let now = Utc::now();
    let ontology = Ontology {
        id: Id::from("ont-code"),
        uri: "urn:engram:ontology:code".to_owned(),
        name: "Code Ontology".to_owned(),
        scope: scope(),
        language: OntologyLanguage::PropertyGraph,
        version: "1.0.0".to_owned(),
        status: OntologyStatus::Active,
        imports: vec![OntologyImport {
            uri: "urn:engram:ontology:base".to_owned(),
            version: Some("1.0.0".to_owned()),
            alias: Some("base".to_owned()),
        }],
        policy: policy(),
        provenance: provenance(now),
        created_at: now,
        updated_at: None,
        metadata: None,
    };

    let value = serde_json::to_value(ontology).expect("serialize ontology");

    assert_eq!(value["language"], json!("property_graph"));
    assert_eq!(value["status"], json!("active"));
    assert_eq!(value["createdAt"], json!(now));
    assert_eq!(value["imports"][0]["alias"], json!("base"));
    assert!(value.get("created_at").is_none());
}

#[test]
fn knowledge_graph_records_reference_ontologies() {
    let now = Utc::now();
    let graph = KnowledgeGraph {
        id: Id::from("kg-code"),
        scope: scope(),
        name: "Code Knowledge Graph".to_owned(),
        uri: Some("urn:engram:graph:code".to_owned()),
        version: Some("1.0.0".to_owned()),
        ontology_refs: vec![OntologyRef {
            id: Some(Id::from("ont-code")),
            uri: Some("urn:engram:ontology:code".to_owned()),
            version: Some("1.0.0".to_owned()),
        }],
        policy: policy(),
        provenance: provenance(now),
        created_at: now,
        updated_at: None,
        metadata: None,
    };

    let value = serde_json::to_value(graph).expect("serialize graph");

    assert_eq!(value["ontologyRefs"][0]["version"], json!("1.0.0"));
    assert_eq!(value["createdAt"], json!(now));
    assert!(value.get("ontology_refs").is_none());
}

#[test]
fn ontology_axioms_can_describe_graph_constraints() {
    let now = Utc::now();
    let axiom = OntologyAxiom {
        id: Id::from("axiom-function-calls-function"),
        ontology_id: Id::from("ont-code"),
        kind: OntologyAxiomKind::Domain,
        subject_class_id: Some(Id::from("class-function")),
        property_id: Some(Id::from("prop-calls")),
        object_class_id: Some(Id::from("class-function")),
        expression: Some(json!({ "minCardinality": 0 })),
        provenance: provenance(now),
        created_at: now,
        metadata: None,
    };

    let value = serde_json::to_value(axiom).expect("serialize axiom");

    assert_eq!(value["kind"], json!("domain"));
    assert_eq!(value["ontologyId"], json!("ont-code"));
    assert_eq!(value["propertyId"], json!("prop-calls"));
    assert!(value.get("ontology_id").is_none());
}
