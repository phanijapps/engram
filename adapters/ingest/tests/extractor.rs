use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
};
use engram_knowledge::KnowledgeGraphRepository;
use engram_store_sqlite::SqlKnowledgeStore;
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
        None,
    ))
    .expect("extract");

    let names: Vec<String> = extracted.entities.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains(&"lib.rs::alpha".to_owned()));
    assert!(names.contains(&"lib.rs::beta".to_owned()));
    assert!(names.contains(&"lib.rs::Widget".to_owned()));

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
        calls
            .iter()
            .any(|(s, o)| s == "lib.rs::alpha" && o == "lib.rs::beta"),
        "expected alpha -> beta calls edge, got {calls:?}"
    );

    // The graph is persisted, so neighbors traverses the real store.
    let alpha_id = extracted
        .entities
        .iter()
        .find(|e| e.name == "lib.rs::alpha")
        .expect("alpha entity")
        .id
        .clone();
    let neighbors = block_on(store.neighbors(&extracted.graph.id, &alpha_id, &scope(), None))
        .expect("neighbors");
    assert!(
        neighbors
            .iter()
            .any(|r| r.object.name.as_deref() == Some("lib.rs::beta"))
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

#[test]
fn code_entities_carry_qualified_identities() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let request = DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "demo".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some("src/lib.rs".to_owned()),
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
        stable_source_key: Some("my-repo".to_owned()),
    };

    let ingested = block_on(ingestor.ingest(&store, request)).expect("ingest");
    let extracted = block_on(GraphExtractor::new().extract_into(
        &store,
        &ingested.source,
        &ingested.document,
        &ingested.chunks,
        None,
    ))
    .expect("extract");

    // Identity is {repo}/{path}::{bare_name} with {repo} = stable-source-key.
    let names: Vec<String> = extracted.entities.iter().map(|e| e.name.clone()).collect();
    assert!(
        names.contains(&"my-repo/src/lib.rs::alpha".to_owned()),
        "qualified alpha missing: {names:?}"
    );
    assert!(
        names.contains(&"my-repo/src/lib.rs::beta".to_owned()),
        "qualified beta missing: {names:?}"
    );
    assert!(
        names.contains(&"my-repo/src/lib.rs::Widget".to_owned()),
        "qualified Widget missing: {names:?}"
    );

    // Re-extraction converges: entity ids are stable across runs (id keyed on
    // the qualified name).
    let again = block_on(GraphExtractor::new().extract_into(
        &store,
        &ingested.source,
        &ingested.document,
        &ingested.chunks,
        None,
    ))
    .expect("extract again");
    let id_again = again
        .entities
        .iter()
        .find(|e| e.name.ends_with("::alpha"))
        .expect("alpha entity")
        .id
        .clone();
    let id_first = extracted
        .entities
        .iter()
        .find(|e| e.name.ends_with("::alpha"))
        .expect("alpha entity")
        .id
        .clone();
    assert_eq!(
        id_first, id_again,
        "entity id must be stable across re-extraction"
    );
}
