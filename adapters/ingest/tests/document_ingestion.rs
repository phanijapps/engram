use engram_domain::*;
use engram_ingest::{
    DocumentIngestRequest, DocumentMetadata, KnowledgeIngestor, PlainTextChunker,
    PlainTextChunkerOptions,
};
use engram_knowledge::KnowledgeRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

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

fn request() -> DocumentIngestRequest {
    DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "engram-docs".to_owned(),
        scope: Scope {
            tenant: "tenant-demo".to_owned(),
            subject: None,
            workspace: Some("engram".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        },
        document_kind: SourceDocumentKind::Markdown,
        document: DocumentMetadata {
            path: Some("docs/intro.md".to_owned()),
            title: Some("Intro".to_owned()),
            mime_type: Some("text/markdown".to_owned()),
            language: Some("en".to_owned()),
            ..DocumentMetadata::default()
        },
        text: "# Engram\n\nMemory and knowledge are separate interface axes.\n\nChunks keep provenance."
            .to_owned(),
        policy: policy(),
        actor: actor(),
        stable_source_key: None,
    }
}

#[test]
fn ingests_text_document_into_source_document_and_chunks() {
    let repository = SqlKnowledgeStore::open_in_memory().expect("open knowledge store");
    let ingestor = KnowledgeIngestor::new(
        PlainTextChunker::new(PlainTextChunkerOptions {
            max_chars_per_chunk: 48,
        })
        .expect("chunker"),
    );

    let ingested = block_on(ingestor.ingest(&repository, request())).expect("ingest document");

    assert_eq!(ingested.source.name, "engram-docs");
    assert_eq!(ingested.document.path.as_deref(), Some("docs/intro.md"));
    assert!(ingested.document.content_hash.starts_with("sha256:"));
    assert!(ingested.chunks.len() > 1);
    assert!(ingested.chunks.iter().all(|chunk| {
        chunk.source_id == ingested.source.id
            && chunk.document_id == ingested.document.id
            && chunk.embedding_refs.is_empty()
            && chunk.content_hash.starts_with("sha256:")
    }));
    assert_eq!(
        ingested.chunks[0]
            .location
            .as_ref()
            .and_then(|location| location.path.as_deref()),
        Some("docs/intro.md")
    );

    let loaded = block_on(repository.get_chunk(
        &ingested.chunks[0].id,
        &Scope {
            tenant: "tenant-demo".to_owned(),
            subject: None,
            workspace: Some("engram".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        },
    ))
    .expect("load chunk");
    assert_eq!(loaded.expect("visible chunk").id, ingested.chunks[0].id);
}

#[test]
fn unchanged_reingestion_produces_stable_ids_and_hashes() {
    let first_repository = SqlKnowledgeStore::open_in_memory().expect("open first store");
    let second_repository = SqlKnowledgeStore::open_in_memory().expect("open second store");
    let ingestor = KnowledgeIngestor::new(PlainTextChunker::default());

    let first = block_on(ingestor.ingest(&first_repository, request())).expect("first ingest");
    let second = block_on(ingestor.ingest(&second_repository, request())).expect("second ingest");

    assert_eq!(first.source.id, second.source.id);
    assert_eq!(first.document.id, second.document.id);
    assert_eq!(first.document.content_hash, second.document.content_hash);
    assert_eq!(
        first
            .chunks
            .iter()
            .map(|chunk| (&chunk.id, &chunk.content_hash))
            .collect::<Vec<_>>(),
        second
            .chunks
            .iter()
            .map(|chunk| (&chunk.id, &chunk.content_hash))
            .collect::<Vec<_>>()
    );
}
