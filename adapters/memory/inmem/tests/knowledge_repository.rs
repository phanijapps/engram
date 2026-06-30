use engram_core::KnowledgeRepository;
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
        id: Id::from("actor-test"),
        kind: ActorKind::Agent,
        display_name: Some("Knowledge Test".to_owned()),
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
        source: "knowledge_repository_test".to_owned(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_owned()),
    }
}

#[test]
fn knowledge_repository_round_trips_chunk_inside_scope() {
    let service = InMemoryMemoryService::new();
    let source = KnowledgeSource {
        id: Id::from("source-1"),
        kind: SourceKind::Filesystem,
        scope: scope("engram"),
        name: "repo".to_owned(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };
    let document = SourceDocument {
        id: Id::from("document-1"),
        source_id: source.id.clone(),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some("README.md".to_owned()),
        title: Some("Readme".to_owned()),
        mime_type: Some("text/markdown".to_owned()),
        language: Some("en".to_owned()),
        version: None,
        content_hash: "sha256:doc".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };
    let chunk = KnowledgeChunk {
        id: Id::from("chunk-1"),
        document_id: document.id.clone(),
        source_id: source.id.clone(),
        kind: KnowledgeChunkKind::DocumentSection,
        text: "Engram stores knowledge separately from memory.".to_owned(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: "sha256:chunk".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };

    block_on(service.put_source(source)).expect("put source");
    block_on(service.put_document(document)).expect("put document");
    block_on(service.put_chunk(chunk.clone())).expect("put chunk");

    let loaded = block_on(service.get_chunk(&chunk.id, &scope("engram"))).expect("get chunk");
    assert_eq!(loaded.expect("visible chunk").text, chunk.text);
    let hidden = block_on(service.get_chunk(&chunk.id, &scope("other"))).expect("get hidden chunk");
    assert!(hidden.is_none());
}
