use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, KnowledgeIngestor,
};
use engram_knowledge::KnowledgeRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

#[test]
fn ingestor_persists_code_symbol_chunks_with_source_paths() {
    let repository = SqlKnowledgeStore::open_in_memory().expect("open knowledge store");
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);

    let ingested =
        block_on(ingestor.ingest(&repository, code_request())).expect("ingest code document");

    assert_eq!(ingested.document.kind, SourceDocumentKind::Code);
    assert_eq!(
        ingested
            .chunks
            .iter()
            .map(|chunk| {
                (
                    chunk.kind.clone(),
                    chunk.location.as_ref().and_then(|location| {
                        Some((
                            location.path.as_deref()?,
                            location.anchor.as_deref()?,
                            location.start_line?,
                            location.end_line?,
                        ))
                    }),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                KnowledgeChunkKind::CodeSymbol,
                Some(("src/memory.rs", "struct MemoryRecord", 1, 3))
            ),
            (
                KnowledgeChunkKind::CodeSymbol,
                Some(("src/memory.rs", "fn remember", 5, 7))
            )
        ]
    );
    assert!(ingested.chunks.iter().all(|chunk| {
        chunk.source_id == ingested.source.id
            && chunk.document_id == ingested.document.id
            && chunk.embedding_refs.is_empty()
            && chunk.content_hash.starts_with("sha256:")
    }));

    let loaded = block_on(repository.get_chunk(&ingested.chunks[0].id, &scope()))
        .expect("load chunk")
        .expect("visible chunk");
    assert_eq!(loaded.kind, KnowledgeChunkKind::CodeSymbol);
}

fn code_request() -> DocumentIngestRequest {
    DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "engram-code".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some("src/memory.rs".to_owned()),
            title: Some("memory.rs".to_owned()),
            mime_type: Some("text/plain".to_owned()),
            language: Some("rust".to_owned()),
            ..DocumentMetadata::default()
        },
        text: r#"pub struct MemoryRecord {
    id: String,
}

fn remember() {
    println!("memory");
}
"#
        .to_owned(),
        policy: policy(),
        actor: actor(),
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-ingest"),
        kind: ActorKind::Agent,
        display_name: Some("Ingest Agent".to_owned()),
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}
