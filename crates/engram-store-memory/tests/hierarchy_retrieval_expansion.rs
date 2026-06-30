use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{
    Clock, CoreResult, HierarchyRepository, MemoryRepository, MemoryService, PolicyAuthorizer,
};
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
fn hierarchical_mode_expands_to_sibling_memory_candidates() {
    let service = service();
    let matched = write_memory(
        &service,
        "Engram hierarchy seed recalls Rust memory.",
        scope("engram"),
        policy(vec![AllowedUse::Retrieval]),
    );
    let sibling = write_memory(
        &service,
        "SQLite vector testing uses FastEmbed BGE small.",
        scope("engram"),
        policy(vec![AllowedUse::Retrieval]),
    );
    seed_sibling_hierarchy(&service, scope("engram"), &[&matched, &sibling]);

    let context = block_on(service.retrieve(retrieval_request(
        "Rust memory",
        scope("engram"),
        vec![RetrievalMode::Keyword, RetrievalMode::Hierarchical],
    )))
    .expect("retrieve context");

    assert_eq!(
        context
            .items
            .iter()
            .map(|item| item.target_id.as_str())
            .collect::<Vec<_>>(),
        vec![matched.id.as_str(), sibling.id.as_str()]
    );
    let expanded = context
        .items
        .iter()
        .find(|item| item.target_id == sibling.id.as_str())
        .expect("expanded sibling result");
    assert_eq!(expanded.target_type, RetrievalTargetType::Memory);
    assert_eq!(expanded.score.hierarchical_fit, Some(1.0));
    assert_eq!(
        expanded
            .fusion_trace
            .as_ref()
            .map(|trace| trace.source.as_str()),
        Some("hierarchy.expansion")
    );
    assert!(
        expanded
            .explanation
            .as_ref()
            .expect("expanded explanation")
            .path
            .iter()
            .any(|path| path.contains("Engram sibling memories"))
    );
}

#[test]
fn keyword_only_mode_does_not_expand_to_siblings() {
    let service = service();
    let matched = write_memory(
        &service,
        "Engram hierarchy seed recalls Rust memory.",
        scope("engram"),
        policy(vec![AllowedUse::Retrieval]),
    );
    let sibling = write_memory(
        &service,
        "SQLite vector testing uses FastEmbed BGE small.",
        scope("engram"),
        policy(vec![AllowedUse::Retrieval]),
    );
    seed_sibling_hierarchy(&service, scope("engram"), &[&matched, &sibling]);

    let context = block_on(service.retrieve(retrieval_request(
        "Rust memory",
        scope("engram"),
        vec![RetrievalMode::Keyword],
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_id, matched.id.as_str());
}

#[test]
fn expansion_respects_policy_scope_and_direct_match_deduplication() {
    let service = service();
    let matched = write_memory(
        &service,
        "Engram hierarchy seed recalls Rust memory.",
        scope("engram"),
        policy(vec![AllowedUse::Retrieval]),
    );
    let direct = write_memory(
        &service,
        "Engram Rust memory direct sibling match.",
        scope("engram"),
        policy(vec![AllowedUse::Retrieval]),
    );
    let denied = put_memory_record(
        &service,
        "memory-denied",
        "Policy denied sibling should not leak.",
        scope("engram"),
        policy(vec![AllowedUse::TrainingExport]),
    );
    let private = write_memory(
        &service,
        "Private sibling should not leak.",
        scope("private"),
        policy(vec![AllowedUse::Retrieval]),
    );
    seed_sibling_hierarchy(&service, scope("engram"), &[&matched, &direct, &denied]);
    seed_sibling_hierarchy(&service, scope("private"), &[&private]);

    let context = block_on(service.retrieve(retrieval_request(
        "Rust memory",
        scope("engram"),
        vec![RetrievalMode::Keyword, RetrievalMode::Hierarchical],
    )))
    .expect("retrieve context");

    assert_eq!(
        context
            .items
            .iter()
            .filter(|item| item.target_id == direct.id.as_str())
            .count(),
        1
    );
    assert!(
        context
            .items
            .iter()
            .all(|item| item.target_id != denied.id.as_str()
                && item.target_id != private.id.as_str())
    );
    assert!(
        context
            .omitted
            .iter()
            .any(|omitted| omitted.target_id == denied.id.as_str()
                && omitted.reason == OmittedReason::PolicyDenied)
    );
    assert!(
        context
            .omitted
            .iter()
            .all(|omitted| omitted.target_id != private.id.as_str())
    );
}

fn service() -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        Arc::new(AllowAll),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

fn write_memory(
    service: &InMemoryMemoryService,
    text: &str,
    scope: Scope,
    policy: Policy,
) -> MemoryRecord {
    block_on(service.write_memory(WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: Some(format!("summary: {text}")),
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope,
        requester: requester(),
        provenance: provenance(),
        policy,
        links: Vec::new(),
        idempotency_key: None,
    }))
    .expect("write memory")
    .record
}

fn put_memory_record(
    service: &InMemoryMemoryService,
    id: &str,
    text: &str,
    scope: Scope,
    policy: Policy,
) -> MemoryRecord {
    let record = MemoryRecord {
        id: Id::from(id),
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: Some(format!("summary: {text}")),
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope,
        provenance: provenance(),
        policy,
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    };
    block_on(service.put_memory(record)).expect("put memory record")
}

fn seed_sibling_hierarchy(
    service: &InMemoryMemoryService,
    hierarchy_scope: Scope,
    memories: &[&MemoryRecord],
) {
    let parent_id = Id::from(format!(
        "parent-{}",
        hierarchy_scope
            .workspace
            .clone()
            .unwrap_or_else(|| "default".to_owned())
    ));
    block_on(service.put_node(HierarchyNode {
        id: parent_id.clone(),
        scope: hierarchy_scope.clone(),
        kind: HierarchyNodeKind::Aggregate,
        layer: 1,
        name: "Engram sibling memories".to_owned(),
        summary: Some("Sibling memories".to_owned()),
        parent_id: None,
        members: Vec::new(),
        source_target_type: None,
        source_target_id: None,
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(vec![AllowedUse::Retrieval]),
        provenance: provenance(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put parent node");

    for memory in memories {
        block_on(service.put_node(HierarchyNode {
            id: Id::from(format!("base-{}", memory.id)),
            scope: hierarchy_scope.clone(),
            kind: HierarchyNodeKind::Base,
            layer: 0,
            name: memory.content.text.clone(),
            summary: memory.content.summary.clone(),
            parent_id: Some(parent_id.clone()),
            members: Vec::new(),
            source_target_type: Some(RetrievalTargetType::Memory),
            source_target_id: Some(memory.id.to_string()),
            embedding_refs: Vec::new(),
            status: HierarchyNodeStatus::Active,
            policy: policy(vec![AllowedUse::Retrieval]),
            provenance: provenance(),
            created_at: fixed_time(),
            updated_at: None,
            metadata: None,
        }))
        .expect("put base node");
    }
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
        display_name: Some("Hierarchy Expansion Agent".to_owned()),
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

fn policy(allowed_uses: Vec<AllowedUse>) -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses,
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "hierarchy_retrieval_expansion_test".to_owned(),
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
