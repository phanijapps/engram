use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::Clock;
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
fn valid_retrieval_example_executes_against_service() {
    let runner = MemoryContractRunner::new(Arc::new(service()));

    let outcome = block_on(runner.retrieve_accepted_example()).expect("retrieve fixture");
    let context = outcome.context;

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_type, RetrievalTargetType::Memory);
    assert!(context.items[0].content.contains("Rust 2024"));
    assert!(context.items[0].explanation.is_some());
    assert_eq!(context.created_at, fixed_time());
}

#[test]
fn missing_requester_retrieval_fixture_fails_before_service_execution() {
    let error = accepted_examples::invalid_retrieval_missing_requester()
        .expect_err("missing requester should fail deserialization");

    assert!(error.to_string().contains("requester"));
}
