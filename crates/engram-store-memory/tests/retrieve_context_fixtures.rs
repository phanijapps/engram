use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{Clock, MemoryService};
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

#[test]
fn valid_retrieval_example_executes_against_service() {
    let service = service();
    let write_request: WriteMemoryRequest = serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/write-memory-request.json"
    ))
    .expect("deserialize write fixture");
    let retrieval_request: RetrievalRequest = serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/retrieval-request.json"
    ))
    .expect("deserialize retrieval fixture");

    block_on(service.write_memory(write_request)).expect("write fixture");
    let context = block_on(service.retrieve(retrieval_request)).expect("retrieve fixture");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_type, RetrievalTargetType::Memory);
    assert!(context.items[0].content.contains("Rust 2024"));
    assert!(context.items[0].explanation.is_some());
    assert_eq!(context.created_at, fixed_time());
}

#[test]
fn missing_requester_retrieval_fixture_fails_before_service_execution() {
    let error = serde_json::from_str::<RetrievalRequest>(include_str!(
        "../../../contracts/v1/examples/invalid/retrieval-request.missing-requester.json"
    ))
    .expect_err("missing requester should fail deserialization");

    assert!(error.to_string().contains("requester"));
}
