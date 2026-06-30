use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository, TaxonomyRepository};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn actor() -> Actor {
    Actor {
        id: Id::from("agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Knowledge Agent".to_owned()),
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
        source: "knowledge-sqlite-test".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn graph(graph_id: &str, tenant: &str) -> KnowledgeGraph {
    KnowledgeGraph {
        id: Id::from(graph_id),
        scope: scope(tenant),
        name: "Code Graph".to_owned(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

#[test]
fn round_trips_scoped_graph_entities_and_neighbors() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let graph_id = Id::from("graph-1");
    let function_a = Id::from("function-a");

    block_on(store.put_graph(graph(graph_id.as_str(), "tenant-a"))).expect("put graph");
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

    let visible_graph =
        block_on(store.get_graph(&graph_id, &scope("tenant-a"))).expect("get graph");
    let hidden_graph = block_on(store.get_graph(&graph_id, &scope("tenant-b"))).expect("get graph");
    let neighbors = block_on(store.neighbors(&graph_id, &function_a, &scope("tenant-a"), Some(1)))
        .expect("neighbors");
    let hidden_neighbors =
        block_on(store.neighbors(&graph_id, &function_a, &scope("tenant-b"), None))
            .expect("hidden neighbors");

    assert!(visible_graph.is_some());
    assert!(hidden_graph.is_none());
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].id, Id::from("rel-1"));
    assert!(hidden_neighbors.is_empty());
}

#[test]
fn chunk_inherits_source_scope() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let source_id = Id::from("source-1");
    let document_id = Id::from("document-1");
    let chunk_id = Id::from("chunk-1");

    block_on(store.put_source(KnowledgeSource {
        id: source_id.clone(),
        kind: SourceKind::Filesystem,
        scope: scope("tenant-a"),
        name: "docs".to_owned(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put source");
    block_on(store.put_document(SourceDocument {
        id: document_id.clone(),
        source_id: source_id.clone(),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some("docs/intro.md".to_owned()),
        title: None,
        mime_type: None,
        language: None,
        version: None,
        content_hash: "sha256:abc".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put document");
    block_on(store.put_chunk(KnowledgeChunk {
        id: chunk_id.clone(),
        document_id: document_id.clone(),
        source_id: source_id.clone(),
        kind: KnowledgeChunkKind::Paragraph,
        text: "engram keeps memory and knowledge separate.".to_owned(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: "sha256:chunk".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put chunk");

    let visible = block_on(store.get_chunk(&chunk_id, &scope("tenant-a"))).expect("get chunk");
    let hidden = block_on(store.get_chunk(&chunk_id, &scope("tenant-b"))).expect("get chunk");

    assert!(visible.is_some());
    assert!(hidden.is_none());
}

#[test]
fn taxonomy_round_trips_scoped_scheme_and_concepts() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let scheme_id = Id::from("scheme-1");

    block_on(store.put_concept_scheme(ConceptScheme {
        id: scheme_id.clone(),
        uri: "urn:scheme:langs".to_owned(),
        name: "Languages".to_owned(),
        scope: scope("tenant-a"),
        version: "1.0.0".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: Utc::now(),
        updated_at: None,
    }))
    .expect("put scheme");

    for concept_id in ["concept-rust", "concept-ts"] {
        block_on(store.put_concept(Concept {
            id: Id::from(concept_id),
            uri: format!("urn:concept:{concept_id}"),
            scheme_id: scheme_id.clone(),
            pref_label: ConceptLabel {
                value: concept_id.to_owned(),
                language: Some("en".to_owned()),
            },
            alt_labels: Vec::new(),
            definition: None,
            notation: None,
            status: ConceptStatus::Active,
            provenance: provenance(),
            created_at: Utc::now(),
            updated_at: None,
        }))
        .expect("put concept");
    }

    block_on(store.put_concept_relation(ConceptRelation {
        id: "rel-rust-ts".to_owned(),
        scheme_id: scheme_id.clone(),
        subject_id: Id::from("concept-rust"),
        predicate: ConceptRelationKind::Related,
        object_id: Id::from("concept-ts"),
        provenance: provenance(),
        created_at: Utc::now(),
    }))
    .expect("put relation");

    let visible_scheme =
        block_on(store.get_concept_scheme(&scheme_id, &scope("tenant-a"))).expect("get scheme");
    let hidden_scheme =
        block_on(store.get_concept_scheme(&scheme_id, &scope("tenant-b"))).expect("get scheme");
    let concepts =
        block_on(store.list_concepts(&scheme_id, &scope("tenant-a"))).expect("list concepts");
    let hidden_concepts =
        block_on(store.list_concepts(&scheme_id, &scope("tenant-b"))).expect("list concepts");

    assert!(visible_scheme.is_some());
    assert!(hidden_scheme.is_none());
    assert_eq!(concepts.len(), 2);
    assert_eq!(concepts[0].id, Id::from("concept-rust"));
    assert!(hidden_concepts.is_empty());
}
