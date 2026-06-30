use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{Clock, CoreResult, KnowledgeRepository, MemoryService, PolicyAuthorizer};
use engram_domain::*;
use engram_store_memory::{InMemoryMemoryService, SequentialIdGenerator};
use futures::executor::block_on;

#[derive(Debug)]
struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[derive(Debug)]
struct AllowAll;

impl PolicyAuthorizer for AllowAll {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}

#[test]
fn retrieve_returns_matching_knowledge_chunk_with_source_explanation() {
    let service = service();
    seed_knowledge(
        &service,
        "chunk-rust",
        SourceKind::Filesystem,
        KnowledgeChunkKind::DocumentSection,
        "Engram keeps source-grounded Rust knowledge separate from agent memory.",
        scope("engram"),
    );

    let context = block_on(service.retrieve(retrieval_request("Rust knowledge", scope("engram"))))
        .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    let item = &context.items[0];
    assert_eq!(item.target_type, RetrievalTargetType::Chunk);
    assert_eq!(item.target_id, "chunk-rust");
    assert!(item.content.contains("source-grounded Rust"));
    assert_eq!(
        item.fusion_trace
            .as_ref()
            .map(|trace| trace.source.as_str()),
        Some("knowledge.keyword")
    );
    let explanation = item.explanation.as_ref().expect("chunk explanation");
    assert_eq!(explanation.path, vec!["docs/reference.md"]);
    assert_eq!(explanation.matched_terms, vec!["knowledge", "rust"]);
}

#[test]
fn retrieve_applies_source_and_chunk_kind_filters() {
    let service = service();
    seed_knowledge(
        &service,
        "chunk-doc",
        SourceKind::Filesystem,
        KnowledgeChunkKind::DocumentSection,
        "Engram retrieval filter target.",
        scope("engram"),
    );
    seed_knowledge(
        &service,
        "chunk-code",
        SourceKind::GitRepository,
        KnowledgeChunkKind::CodeSymbol,
        "Engram retrieval filter target.",
        scope("engram"),
    );

    let mut request = retrieval_request("retrieval filter", scope("engram"));
    request.filters = Some(QueryFilter {
        memory_kinds: Vec::new(),
        source_kinds: vec![SourceKind::GitRepository],
        chunk_kinds: vec![KnowledgeChunkKind::CodeSymbol],
        concept_ids: Vec::new(),
        entity_ids: Vec::new(),
        since: None,
        until: None,
        min_confidence: None,
        include_archived: Some(false),
    });
    let context = block_on(service.retrieve(request)).expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_id, "chunk-code");
}

#[test]
fn retrieve_does_not_leak_knowledge_across_scope() {
    let service = service();
    seed_knowledge(
        &service,
        "chunk-private",
        SourceKind::Filesystem,
        KnowledgeChunkKind::DocumentSection,
        "Private source-grounded knowledge.",
        scope("private"),
    );

    let context = block_on(service.retrieve(retrieval_request("source-grounded", scope("engram"))))
        .expect("retrieve context");

    assert!(context.items.is_empty());
    assert!(context.omitted.is_empty());
}

#[test]
fn retrieve_truncates_after_memory_and_knowledge_candidates_are_fused() {
    let service = service();
    block_on(service.write_memory(write_request(
        "Engram retrieval combines memory candidates.",
        scope("engram"),
    )))
    .expect("write memory");
    seed_knowledge(
        &service,
        "chunk-combined",
        SourceKind::Filesystem,
        KnowledgeChunkKind::DocumentSection,
        "Engram retrieval combines knowledge candidates.",
        scope("engram"),
    );

    let mut request = retrieval_request("retrieval combines", scope("engram"));
    request.limit = Some(1);
    let context = block_on(service.retrieve(request)).expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.omitted.len(), 1);
    assert_eq!(context.omitted[0].reason, OmittedReason::BudgetExceeded);
    assert!(
        context
            .omitted
            .iter()
            .any(|omitted| omitted.target_type == RetrievalTargetType::Memory
                || omitted.target_type == RetrievalTargetType::Chunk)
    );
}

fn service() -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        Arc::new(AllowAll),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

fn seed_knowledge(
    service: &InMemoryMemoryService,
    chunk_id: &str,
    source_kind: SourceKind,
    chunk_kind: KnowledgeChunkKind,
    text: &str,
    scope: Scope,
) {
    let source = KnowledgeSource {
        id: Id::from(format!("source-{chunk_id}")),
        kind: source_kind,
        scope,
        name: "Knowledge Source".to_owned(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    };
    let document = SourceDocument {
        id: Id::from(format!("document-{chunk_id}")),
        source_id: source.id.clone(),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some("docs/reference.md".to_owned()),
        title: Some("Reference".to_owned()),
        mime_type: Some("text/markdown".to_owned()),
        language: Some("en".to_owned()),
        version: None,
        content_hash: format!("sha256:document-{chunk_id}"),
        provenance: provenance(),
        policy: policy(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    };
    let chunk = KnowledgeChunk {
        id: Id::from(chunk_id),
        document_id: document.id.clone(),
        source_id: source.id.clone(),
        kind: chunk_kind,
        text: text.to_owned(),
        summary: Some("Knowledge summary".to_owned()),
        location: Some(SourceLocation {
            path: Some("docs/reference.md".to_owned()),
            start_line: Some(1),
            end_line: Some(3),
            start_offset: None,
            end_offset: None,
            anchor: Some("reference".to_owned()),
        }),
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: format!("sha256:{chunk_id}"),
        provenance: provenance(),
        policy: policy(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    };

    block_on(service.put_source(source)).expect("put source");
    block_on(service.put_document(document)).expect("put document");
    block_on(service.put_chunk(chunk)).expect("put chunk");
}

fn retrieval_request(query: &str, scope: Scope) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope,
        requester: requester(),
        modes: vec![RetrievalMode::Keyword],
        filters: Some(QueryFilter {
            memory_kinds: Vec::new(),
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since: None,
            until: None,
            min_confidence: None,
            include_archived: Some(false),
        }),
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

fn write_request(text: &str, scope: Scope) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope,
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.retrieve".to_owned()],
        on_behalf_of: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Knowledge Retrieval Agent".to_owned()),
        metadata: None,
    }
}

fn scope(workspace: &str) -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
        subject: None,
        workspace: Some(workspace.to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "knowledge_retrieval_test".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_owned()),
    }
}

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}
