use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{Clock, CoreError, MemoryEventRepository, MemoryService};
use engram_domain::*;
use engram_store_memory::{AllowAllPolicyAuthorizer, InMemoryMemoryService, SequentialIdGenerator};
use futures::executor::block_on;

#[derive(Debug)]
struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 1, 0)
        .single()
        .expect("fixed timestamp")
}

fn service() -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        Arc::new(AllowAllPolicyAuthorizer),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

fn read_request(path_contents: &str) -> WriteMemoryRequest {
    serde_json::from_str(path_contents).expect("deserialize write memory request fixture")
}

#[test]
fn valid_write_memory_example_executes_against_service() {
    let service = service();
    let request = read_request(include_str!(
        "../../../contracts/v1/examples/write-memory-request.json"
    ));

    let response = block_on(service.write_memory(request)).expect("write fixture");

    assert_eq!(response.record.id.as_str(), "memory-000001");
    assert_eq!(response.record.status, MemoryStatus::Active);
    assert_eq!(response.event.kind, MemoryEventKind::Written);
    assert_eq!(response.event.recorded_at, fixed_time());

    let events =
        block_on(service.list_events_for_memory(&response.record.id, &response.record.scope))
            .expect("list fixture events");
    assert_eq!(events, vec![response.event]);
}

#[test]
fn missing_scope_tenant_fixture_fails_before_service_execution() {
    let error = serde_json::from_str::<WriteMemoryRequest>(include_str!(
        "../../../contracts/v1/examples/invalid/write-memory-request.missing-scope-tenant.json"
    ))
    .expect_err("missing tenant should fail deserialization");

    assert!(error.to_string().contains("tenant"));
}

#[test]
fn training_export_fixture_is_rejected_by_v1_write_behavior() {
    let service = service();
    let request = read_request(include_str!(
        "../../../contracts/v1/examples/invalid/write-memory-request.training-export.json"
    ));

    let error = block_on(service.write_memory(request)).expect_err("training export rejected");

    assert!(matches!(error, CoreError::InvalidRequest { .. }));
}
