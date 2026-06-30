use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{Clock, CoreResult, HierarchyRepository, MemoryService, PolicyAuthorizer};
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
fn hierarchical_mode_adds_path_context_to_matching_memory_results() {
    let service = service();
    let memory = write_memory(
        &service,
        "Engram hierarchy retrieval context memory.",
        scope("engram"),
    );
    seed_hierarchy(&service, &memory, scope("engram"));

    let context = block_on(service.retrieve(retrieval_request(
        "hierarchy retrieval",
        scope("engram"),
        vec![RetrievalMode::Keyword, RetrievalMode::Hierarchical],
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    let item = &context.items[0];
    assert_eq!(item.target_type, RetrievalTargetType::Memory);
    assert_eq!(item.target_id, memory.id.as_str());
    assert_eq!(item.score.hierarchical_fit, Some(1.0));
    let explanation = item.explanation.as_ref().expect("explanation");
    assert_eq!(
        explanation.path,
        vec![
            "base:Engram hierarchy retrieval context memory.",
            "aggregate:Engram retrieval memories",
        ]
    );
    assert!(explanation.reason.contains("Hierarchy context attached."));
}

#[test]
fn keyword_only_mode_does_not_add_hierarchy_context() {
    let service = service();
    let memory = write_memory(
        &service,
        "Engram hierarchy retrieval context memory.",
        scope("engram"),
    );
    seed_hierarchy(&service, &memory, scope("engram"));

    let context = block_on(service.retrieve(retrieval_request(
        "hierarchy retrieval",
        scope("engram"),
        vec![RetrievalMode::Keyword],
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].score.hierarchical_fit, None);
    assert!(
        context.items[0]
            .explanation
            .as_ref()
            .expect("explanation")
            .path
            .is_empty()
    );
}

#[test]
fn out_of_scope_hierarchy_nodes_do_not_annotate_results() {
    let service = service();
    let memory = write_memory(
        &service,
        "Engram hierarchy retrieval context memory.",
        scope("engram"),
    );
    seed_hierarchy(&service, &memory, scope("private"));

    let context = block_on(service.retrieve(retrieval_request(
        "hierarchy retrieval",
        scope("engram"),
        vec![RetrievalMode::Keyword, RetrievalMode::Hierarchical],
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].score.hierarchical_fit, None);
    assert!(
        context.items[0]
            .explanation
            .as_ref()
            .expect("explanation")
            .path
            .is_empty()
    );
}

fn service() -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        Arc::new(AllowAll),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

fn write_memory(service: &InMemoryMemoryService, text: &str, scope: Scope) -> MemoryRecord {
    block_on(service.write_memory(WriteMemoryRequest {
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
    }))
    .expect("write memory")
    .record
}

fn seed_hierarchy(service: &InMemoryMemoryService, memory: &MemoryRecord, hierarchy_scope: Scope) {
    let parent_id = Id::from(format!("parent-{}", memory.id));
    block_on(service.put_node(HierarchyNode {
        id: parent_id.clone(),
        scope: hierarchy_scope.clone(),
        kind: HierarchyNodeKind::Aggregate,
        layer: 1,
        name: "Engram retrieval memories".to_owned(),
        summary: Some("Retrieval memories".to_owned()),
        parent_id: None,
        members: Vec::new(),
        source_target_type: None,
        source_target_id: None,
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(),
        provenance: provenance(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put parent node");

    block_on(service.put_node(HierarchyNode {
        id: Id::from(format!("base-{}", memory.id)),
        scope: hierarchy_scope,
        kind: HierarchyNodeKind::Base,
        layer: 0,
        name: memory.content.text.clone(),
        summary: memory.content.summary.clone(),
        parent_id: Some(parent_id),
        members: Vec::new(),
        source_target_type: Some(RetrievalTargetType::Memory),
        source_target_id: Some(memory.id.to_string()),
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(),
        provenance: provenance(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put base node");
}

fn retrieval_request(query: &str, scope: Scope, modes: Vec<RetrievalMode>) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope,
        requester: requester(),
        modes,
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
        display_name: Some("Hierarchy Retrieval Agent".to_owned()),
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
        source: "hierarchy_retrieval_test".to_owned(),
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
