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

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Spec Agent".to_owned()),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.write".to_owned()],
        on_behalf_of: None,
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

fn provenance() -> Provenance {
    Provenance {
        source: "write_memory_spec".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn write_request() -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: "Engram writes v1 memories through the accepted contract.".to_owned(),
            summary: Some("Contract-backed write".to_owned()),
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: Some("sha256:write-memory-spec".to_owned()),
        },
        scope: scope(),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: Some("write-memory-spec-001".to_owned()),
    }
}

fn service() -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        Arc::new(AllowWrites),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

#[derive(Debug)]
struct AllowWrites;

impl PolicyAuthorizer for AllowWrites {
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
fn write_memory_creates_active_record_and_written_event() {
    let service = service();
    let request = write_request();

    let response = block_on(service.write_memory(request.clone())).expect("write memory");

    assert_eq!(response.record.id.as_str(), "memory-000001");
    assert_eq!(response.record.kind, request.kind);
    assert_eq!(response.record.content, request.content);
    assert_eq!(response.record.scope, request.scope);
    assert_eq!(response.record.policy, request.policy);
    assert_eq!(response.record.provenance, request.provenance);
    assert_eq!(response.record.status, MemoryStatus::Active);
    assert_eq!(response.record.created_at, fixed_time());
    assert_eq!(response.event.id.as_str(), "event-000002");
    assert_eq!(response.event.kind, MemoryEventKind::Written);
    assert_eq!(response.event.memory_id, Some(response.record.id.clone()));
    assert_eq!(response.event.scope, response.record.scope);
    assert_eq!(response.event.actor, request.requester.actor);
    assert_eq!(response.event.occurred_at, fixed_time());
    assert_eq!(response.event.recorded_at, fixed_time());
    assert_eq!(response.deduplicated, Some(false));

    let stored = block_on(service.get_memory(&response.record.id, &response.record.scope))
        .expect("get memory")
        .expect("stored memory");
    assert_eq!(stored, response.record);

    let events =
        block_on(service.list_events_for_memory(&response.record.id, &response.record.scope))
            .expect("list memory events");
    assert_eq!(events, vec![response.event.clone()]);

    let event = block_on(service.get_event(&response.event.id, &response.record.scope))
        .expect("get event")
        .expect("stored event");
    assert_eq!(event, response.event);
}

#[test]
fn write_memory_reuses_idempotent_write_in_same_scope() {
    let service = service();
    let request = write_request();

    let first = block_on(service.write_memory(request.clone())).expect("first write");
    let second = block_on(service.write_memory(request)).expect("second write");

    assert_eq!(second.record.id, first.record.id);
    assert_eq!(second.event.id, first.event.id);
    assert_eq!(second.deduplicated, Some(true));

    let events = block_on(service.list_events_for_memory(&first.record.id, &first.record.scope))
        .expect("list memory events");
    assert_eq!(events.len(), 1);
}

#[test]
fn write_memory_rejects_missing_tenant_without_event() {
    let service = service();
    let mut request = write_request();
    request.scope.tenant.clear();

    let error = block_on(service.write_memory(request)).expect_err("missing tenant rejected");

    assert!(matches!(error, CoreError::InvalidRequest { .. }));
    let events = block_on(service.list_events_for_scope(&scope())).expect("list scope events");
    assert!(events.is_empty());
}

#[derive(Debug)]
struct DenyWrites;

impl PolicyAuthorizer for DenyWrites {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "test denial".to_owned(),
        })
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
fn denied_write_does_not_create_record_or_event() {
    let service = InMemoryMemoryService::with_dependencies(
        Arc::new(DenyWrites),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    );
    let request = write_request();

    let error = block_on(service.write_memory(request)).expect_err("write denied");

    assert!(matches!(error, CoreError::PolicyDenied { .. }));
    let events = block_on(service.list_events_for_scope(&scope())).expect("list scope events");
    assert!(events.is_empty());
}
