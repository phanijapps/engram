use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{
    Clock, CoreError, CoreResult, MemoryEventRepository, MemoryRepository, MemoryService,
    PolicyAuthorizer,
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

#[derive(Debug)]
struct DenyForget;

impl PolicyAuthorizer for DenyForget {
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
        Err(CoreError::PolicyDenied {
            reason: "test forget denial".to_owned(),
        })
    }
}

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 2, 0)
        .single()
        .expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Forget Agent".to_owned()),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.write".to_owned(), "memory.forget".to_owned()],
        on_behalf_of: None,
    }
}

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
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
        source: "forget_memory_spec".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn write_request(tenant: &str) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: "Forget lifecycle memory about Rust bindings.".to_owned(),
            summary: Some("Forget lifecycle".to_owned()),
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope: scope(tenant),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn forget_request(memory_id: &MemoryId, tenant: &str, mode: DeleteMode) -> ForgetRequest {
    ForgetRequest {
        target_type: ForgetTargetType::Memory,
        target_id: memory_id.to_string(),
        scope: scope(tenant),
        requester: requester(),
        mode,
        reason: Some("test cleanup".to_owned()),
    }
}

fn retrieval_request(include_archived: bool) -> RetrievalRequest {
    RetrievalRequest {
        query: "Rust bindings".to_owned(),
        scope: scope("tenant-demo"),
        requester: requester(),
        modes: vec![RetrievalMode::Keyword],
        filters: Some(QueryFilter {
            memory_kinds: vec![MemoryKind::Fact],
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since: None,
            until: None,
            min_confidence: None,
            include_archived: Some(include_archived),
        }),
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

fn service(authorizer: Arc<dyn PolicyAuthorizer>) -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        authorizer,
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

#[test]
fn tombstone_prevents_normal_retrieval_and_writes_event() {
    let service = service(Arc::new(AllowAll));
    let write = block_on(service.write_memory(write_request("tenant-demo"))).expect("write memory");

    let result = block_on(service.forget(forget_request(
        &write.record.id,
        "tenant-demo",
        DeleteMode::Tombstone,
    )))
    .expect("tombstone memory");

    assert_eq!(result.status, ForgetStatus::Tombstoned);
    assert_eq!(
        result.event.as_ref().expect("forget event").kind,
        MemoryEventKind::Forgotten
    );
    let context = block_on(service.retrieve(retrieval_request(false))).expect("retrieve context");
    assert!(context.items.is_empty());
    let events = block_on(service.list_events_for_memory(&write.record.id, &write.record.scope))
        .expect("list events");
    assert_eq!(events.len(), 2);
}

#[test]
fn delete_removes_memory_but_keeps_audit_event() {
    let service = service(Arc::new(AllowAll));
    let write = block_on(service.write_memory(write_request("tenant-demo"))).expect("write memory");

    let result = block_on(service.forget(forget_request(
        &write.record.id,
        "tenant-demo",
        DeleteMode::Delete,
    )))
    .expect("delete memory");

    assert_eq!(result.status, ForgetStatus::Deleted);
    assert!(
        block_on(service.get_memory(&write.record.id, &write.record.scope))
            .expect("get memory")
            .is_none()
    );
    let events = block_on(service.list_events_for_memory(&write.record.id, &write.record.scope))
        .expect("list events");
    assert_eq!(events.len(), 2);
}

#[test]
fn redact_removes_content_and_blocks_retrieval() {
    let service = service(Arc::new(AllowAll));
    let write = block_on(service.write_memory(write_request("tenant-demo"))).expect("write memory");

    let result = block_on(service.forget(forget_request(
        &write.record.id,
        "tenant-demo",
        DeleteMode::Redact,
    )))
    .expect("redact memory");

    assert_eq!(result.status, ForgetStatus::Redacted);
    assert_eq!(
        result.event.as_ref().expect("forget event").kind,
        MemoryEventKind::Redacted
    );
    let stored = block_on(service.get_memory(&write.record.id, &write.record.scope))
        .expect("get memory")
        .expect("redacted memory");
    assert_eq!(stored.status, MemoryStatus::Redacted);
    assert!(stored.content.text.is_empty());
    let context = block_on(service.retrieve(retrieval_request(false))).expect("retrieve context");
    assert!(context.items.is_empty());
}

#[test]
fn archive_hides_memory_unless_archived_records_are_requested() {
    let service = service(Arc::new(AllowAll));
    let write = block_on(service.write_memory(write_request("tenant-demo"))).expect("write memory");

    let result = block_on(service.forget(forget_request(
        &write.record.id,
        "tenant-demo",
        DeleteMode::Archive,
    )))
    .expect("archive memory");

    assert_eq!(result.status, ForgetStatus::Archived);
    let normal = block_on(service.retrieve(retrieval_request(false))).expect("normal retrieval");
    assert!(normal.items.is_empty());
    let archived = block_on(service.retrieve(retrieval_request(true))).expect("archived retrieval");
    assert_eq!(archived.items.len(), 1);
}

#[test]
fn cross_tenant_forget_does_not_mutate_memory() {
    let service = service(Arc::new(AllowAll));
    let write = block_on(service.write_memory(write_request("tenant-demo"))).expect("write memory");

    let result = block_on(service.forget(forget_request(
        &write.record.id,
        "tenant-other",
        DeleteMode::Delete,
    )))
    .expect("cross tenant forget");

    assert_eq!(result.status, ForgetStatus::NotFound);
    assert!(
        block_on(service.get_memory(&write.record.id, &write.record.scope))
            .expect("get memory")
            .is_some()
    );
}

#[test]
fn denied_forget_does_not_mutate_memory_or_append_event() {
    let service = service(Arc::new(DenyForget));
    let write = block_on(service.write_memory(write_request("tenant-demo"))).expect("write memory");

    let error = block_on(service.forget(forget_request(
        &write.record.id,
        "tenant-demo",
        DeleteMode::Delete,
    )))
    .expect_err("forget denied");

    assert!(matches!(error, CoreError::PolicyDenied { .. }));
    assert!(
        block_on(service.get_memory(&write.record.id, &write.record.scope))
            .expect("get memory")
            .is_some()
    );
    let events = block_on(service.list_events_for_memory(&write.record.id, &write.record.scope))
        .expect("list events");
    assert_eq!(events.len(), 1);
}
