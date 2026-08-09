use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    PlainTextChunker, PlainTextChunkerOptions,
};
use engram_knowledge::KnowledgeGraphRepository;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

/// Ingests one code file and returns the ingested source/document/chunks.
fn ingest_code(
    store: &SqlKnowledgeStore,
    stable_source_key: &str,
    path: &str,
    text: &str,
) -> engram_ingest::IngestedKnowledge {
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let request = DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "demo".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some(path.to_owned()),
            ..Default::default()
        },
        text: text.to_owned(),
        policy: policy(),
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        stable_source_key: Some(stable_source_key.to_owned()),
        source_metadata: None,
    };
    block_on(ingestor.ingest(store, request)).expect("ingest")
}

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
        source_metadata: None,
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

#[test]
fn code_entities_carry_logical_names() {
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
        source_metadata: None,
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

    // RFC-0020 rev: the entity NAME is the bare logical symbol. The repo/path/
    // branch live in source_refs + provenance/metadata, NOT jammed into the name.
    let names: Vec<String> = extracted.entities.iter().map(|e| e.name.clone()).collect();
    assert!(
        names.contains(&"alpha".to_owned()),
        "bare alpha missing: {names:?}"
    );
    assert!(
        names.contains(&"beta".to_owned()),
        "bare beta missing: {names:?}"
    );
    assert!(
        names.contains(&"Widget".to_owned()),
        "bare Widget missing: {names:?}"
    );

    // Re-extraction converges: entity ids are stable across runs (id keyed on
    // `graph_id + name`, both unchanged across runs).
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
        .find(|e| e.name == "alpha")
        .expect("alpha entity")
        .id
        .clone();
    let id_first = extracted
        .entities
        .iter()
        .find(|e| e.name == "alpha")
        .expect("alpha entity")
        .id
        .clone();
    assert_eq!(
        id_first, id_again,
        "entity id must be stable across re-extraction"
    );
}

#[test]
fn cross_file_calls_resolve_after_qualification() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let mut index: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Doc A defines `foo`. extract_into registers it under both the qualified
    // name and the bare tail in the shared cross-file index (RFC-0020 T2).
    let a = ingest_code(&store, "repo", "a.rs", "fn foo() {}\n");
    let ext_a = block_on(GraphExtractor::new().extract_into(
        &store,
        &a.source,
        &a.document,
        &a.chunks,
        Some(&mut index),
    ))
    .expect("extract A");
    let foo_id = ext_a
        .entities
        .iter()
        .find(|e| e.name == "foo")
        .expect("foo entity")
        .id
        .to_string();
    // The bare secondary key resolves to foo's id — the mechanism that lets a
    // cross-file AST callee (bare "foo") resolve post-qualification.
    assert_eq!(index.get("foo"), Some(&foo_id));

    // Doc B calls foo (cross-file). extract_with_calls forms a bar->foo edge
    // with a bare, unresolved object ref; the shared index resolves it (this
    // mirrors the scanner's resolution step, deterministically).
    let b = ingest_code(&store, "repo", "b.rs", "fn bar() {}\n");
    let mut ext_b = GraphExtractor::new()
        .extract_with_calls(
            &b.source,
            &b.document,
            &b.chunks,
            Some(&[("bar".to_string(), "foo".to_string())]),
        )
        .expect("extract B");
    for rel in &mut ext_b.relationships {
        if rel.predicate == "calls" && rel.object.id.is_none() {
            if let Some(name) = &rel.object.name {
                if let Some(id) = index.get(name) {
                    rel.object.id = Some(Id::from(id.clone()));
                }
            }
        }
    }
    let bar_calls_foo = ext_b
        .relationships
        .iter()
        .find(|r| r.predicate == "calls" && r.object.name.as_deref() == Some("foo"))
        .expect("bar -> foo cross-file calls edge");
    assert_eq!(
        bar_calls_foo.object.id.as_ref().map(|id| id.to_string()),
        Some(foo_id),
        "cross-file callee must resolve to A's foo entity id"
    );
}

#[test]
fn non_code_documents_emit_no_graph_entities() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ingestor = KnowledgeIngestor::new(
        PlainTextChunker::new(PlainTextChunkerOptions::default()).expect("chunker"),
    );
    let request = DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "demo".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Markdown,
        document: DocumentMetadata {
            path: Some("ARCHITECTURE.md".to_owned()),
            ..Default::default()
        },
        text:
            "# Architecture\n\nThe system uses SQLite for storage.\n\n## Overview\n\nIt is fast.\n"
                .to_owned(),
        policy: policy(),
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        stable_source_key: Some("repo".to_owned()),
        source_metadata: None,
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

    // RFC-0020 T3: no Concept entities from documents — the naive heading-as-node
    // rule is gone; documents are chunks-only at ingest.
    assert!(
        extracted
            .entities
            .iter()
            .all(|e| e.kind != EntityKind::Concept),
        "no Concept entities from non-code docs, got: {entities:?}",
        entities = extracted
            .entities
            .iter()
            .map(|e| (e.kind.clone(), e.name.clone()))
            .collect::<Vec<_>>()
    );
    // The graph record is still persisted (the extract-knowledge op discovers
    // documents via listGraphs — the T3↔T5 invariant).
    let graphs = block_on(store.list_graphs(&scope())).expect("list graphs");
    assert!(
        graphs.iter().any(|g| g.id == extracted.graph.id),
        "graph record must be persisted for non-code documents"
    );
}
