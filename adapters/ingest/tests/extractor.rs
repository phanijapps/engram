use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
};
use engram_knowledge::KnowledgeGraphRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
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
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

#[test]
fn extracts_code_symbols_and_calls_edges() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let request = DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "demo".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some("lib.rs".to_owned()),
            ..Default::default()
        },
        text: "fn alpha() { beta(); }\nfn beta() {}\nstruct Widget;\n".to_owned(),
        policy: policy(),
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        stable_source_key: None,
    };

    let ingested = block_on(ingestor.ingest(&store, request)).expect("ingest");
    let extracted = block_on(GraphExtractor::new().extract_into(
        &store,
        &ingested.source,
        &ingested.document,
        &ingested.chunks,
    ))
    .expect("extract");

    let names: Vec<String> = extracted.entities.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains(&"alpha".to_owned()));
    assert!(names.contains(&"beta".to_owned()));
    assert!(names.contains(&"Widget".to_owned()));

    let calls: Vec<(String, String)> = extracted
        .relationships
        .iter()
        .filter(|r| r.predicate == "calls")
        .map(|r| {
            (
                r.subject.name.clone().unwrap_or_default(),
                r.object.name.clone().unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        calls.iter().any(|(s, o)| s == "alpha" && o == "beta"),
        "expected alpha -> beta calls edge, got {calls:?}"
    );

    // The graph is persisted, so neighbors traverses the real store.
    let alpha_id = extracted
        .entities
        .iter()
        .find(|e| e.name == "alpha")
        .expect("alpha entity")
        .id
        .clone();
    let neighbors = block_on(store.neighbors(&extracted.graph.id, &alpha_id, &scope(), None))
        .expect("neighbors");
    assert!(
        neighbors
            .iter()
            .any(|r| r.object.name.as_deref() == Some("beta"))
    );

    // Chunks carry the entity refs of the symbols extracted from them (Part A).
    assert!(
        !extracted.chunk_entities.is_empty(),
        "expected chunk_entities to be populated"
    );
    let total_refs: usize = extracted
        .chunk_entities
        .iter()
        .map(|(_, refs)| refs.len())
        .sum();
    assert_eq!(total_refs, 3, "3 symbols → 3 chunk-entity refs");
    // Each ref has the entity name.
    for (_idx, refs) in &extracted.chunk_entities {
        for r in refs {
            assert!(r.name.is_some(), "entity ref should have a name");
        }
    }
}
