use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{Clock, CoreError, MemoryEventRepository};
use engram_domain::*;
use engram_eval::{MemoryContractRunner, accepted_examples};
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

#[test]
fn valid_write_memory_example_executes_against_service() {
    let service = service();
    let runner = MemoryContractRunner::new(Arc::new(service.clone()));

    let response = block_on(runner.write_accepted_example()).expect("write fixture");

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
    let error = accepted_examples::invalid_write_missing_scope_tenant()
        .expect_err("missing tenant should fail deserialization");

    assert!(error.to_string().contains("tenant"));
}

#[test]
fn training_export_fixture_is_rejected_by_v1_write_behavior() {
    let service = service();
    let runner = MemoryContractRunner::new(Arc::new(service));
    let request = accepted_examples::invalid_write_training_export()
        .expect("deserialize training export fixture");

    let error = block_on(runner.write_request(request)).expect_err("training export rejected");

    assert!(matches!(error, CoreError::InvalidRequest { .. }));
}
