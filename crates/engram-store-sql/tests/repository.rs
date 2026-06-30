use chrono::{TimeZone, Utc};
use engram_core::{MemoryEventRepository, MemoryRepository};
use engram_domain::*;
use engram_store_sql::SqlMemoryStore;
use futures::executor::block_on;
use serde_json::json;

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("SQL Agent".to_owned()),
        metadata: None,
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

fn provenance() -> Provenance {
    Provenance {
        source: "sql_repository_test".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
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

fn memory(id: &str, tenant: &str) -> MemoryRecord {
    MemoryRecord {
        id: Id::from(id),
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: "SQL adapter stores accepted memory JSON.".to_owned(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope: scope(tenant),
        provenance: provenance(),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }
}

fn event(id: &str, memory_id: &MemoryId, tenant: &str) -> MemoryEvent {
    MemoryEvent {
        id: Id::from(id),
        kind: MemoryEventKind::Written,
        scope: scope(tenant),
        actor: actor(),
        memory_id: Some(memory_id.clone()),
        payload: json!({}),
        provenance: provenance(),
        occurred_at: fixed_time(),
        recorded_at: fixed_time(),
    }
}

#[test]
fn sql_store_round_trips_memory_inside_scope() {
    let store = SqlMemoryStore::open_in_memory().expect("open sql store");
    let record = memory("memory-001", "tenant-demo");

    let stored = block_on(store.put_memory(record.clone())).expect("put memory");
    let fetched = block_on(store.get_memory(&stored.id, &stored.scope))
        .expect("get memory")
        .expect("stored memory");

    assert_eq!(fetched, record);
}

#[test]
fn sql_store_hides_memory_outside_scope() {
    let store = SqlMemoryStore::open_in_memory().expect("open sql store");
    let record = memory("memory-001", "tenant-demo");
    block_on(store.put_memory(record.clone())).expect("put memory");

    let fetched =
        block_on(store.get_memory(&record.id, &scope("tenant-other"))).expect("get memory");

    assert!(fetched.is_none());
}

#[test]
fn sql_store_preserves_event_order_for_memory() {
    let store = SqlMemoryStore::open_in_memory().expect("open sql store");
    let record = memory("memory-001", "tenant-demo");
    block_on(store.put_memory(record.clone())).expect("put memory");
    block_on(store.append_event(event("event-001", &record.id, "tenant-demo")))
        .expect("append first event");
    block_on(store.append_event(event("event-002", &record.id, "tenant-demo")))
        .expect("append second event");

    let events =
        block_on(store.list_events_for_memory(&record.id, &record.scope)).expect("list events");

    assert_eq!(events[0].id.as_str(), "event-001");
    assert_eq!(events[1].id.as_str(), "event-002");
}

#[test]
fn sql_store_updates_memory_status() {
    let store = SqlMemoryStore::open_in_memory().expect("open sql store");
    let record = memory("memory-001", "tenant-demo");
    block_on(store.put_memory(record.clone())).expect("put memory");

    let updated =
        block_on(store.update_memory_status(&record.id, &record.scope, MemoryStatus::Archived))
            .expect("update status");

    assert_eq!(updated.status, MemoryStatus::Archived);
}
