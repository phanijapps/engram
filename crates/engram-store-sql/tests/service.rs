use engram_core::{EvaluationRunner, MemoryEventRepository, MemoryRepository, MemoryService};
use engram_domain::*;
use engram_eval::MemoryFixtureRunner;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;
use std::sync::Arc;
use std::thread;

fn write_fixture() -> WriteMemoryRequest {
    serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/write-memory-request.json"
    ))
    .expect("deserialize write fixture")
}

fn retrieval_fixture() -> RetrievalRequest {
    serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/retrieval-request.json"
    ))
    .expect("deserialize retrieval fixture")
}

#[test]
fn sql_service_reuses_idempotent_write_without_duplicate_event() {
    let service = SqlMemoryService::open_in_memory().expect("open sql service");
    let request = write_fixture();

    let first = block_on(service.write_memory(request.clone())).expect("first write");
    let second = block_on(service.write_memory(request)).expect("second write");

    assert_eq!(second.record.id, first.record.id);
    assert_eq!(second.event.id, first.event.id);
    assert_eq!(second.deduplicated, Some(true));
    let events = block_on(service.list_events_for_memory(&first.record.id, &first.record.scope))
        .expect("list events");
    assert_eq!(events.len(), 1);
}

#[test]
fn sql_service_serializes_concurrent_idempotent_writes() {
    let service = Arc::new(SqlMemoryService::open_in_memory().expect("open sql service"));
    let request = write_fixture();
    let first_service = Arc::clone(&service);
    let first_request = request.clone();
    let second_service = Arc::clone(&service);
    let second_request = request;

    let first =
        thread::spawn(move || block_on(first_service.write_memory(first_request)).expect("write"));
    let second = thread::spawn(move || {
        block_on(second_service.write_memory(second_request)).expect("write")
    });

    let first = first.join().expect("first thread");
    let second = second.join().expect("second thread");

    assert_eq!(first.record.id, second.record.id);
    assert_eq!(first.event.id, second.event.id);
    assert_eq!(
        [first.deduplicated, second.deduplicated]
            .into_iter()
            .filter(|value| *value == Some(true))
            .count(),
        1
    );
    let events = block_on(service.list_events_for_memory(&first.record.id, &first.record.scope))
        .expect("list events");
    assert_eq!(events.len(), 1);
}

#[test]
fn sql_service_retrieves_written_memory() {
    let service = SqlMemoryService::open_in_memory().expect("open sql service");
    block_on(service.write_memory(write_fixture())).expect("write memory");

    let context = block_on(service.retrieve(retrieval_fixture())).expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert!(context.items[0].content.contains("Rust 2024"));
}

#[test]
fn sql_service_forget_delete_removes_memory_and_keeps_event() {
    let service = SqlMemoryService::open_in_memory().expect("open sql service");
    let response = block_on(service.write_memory(write_fixture())).expect("write memory");
    let request = ForgetRequest {
        target_type: ForgetTargetType::Memory,
        target_id: response.record.id.to_string(),
        scope: response.record.scope.clone(),
        requester: Requester {
            actor: response.event.actor.clone(),
            roles: vec!["maintainer".to_owned()],
            permissions: vec!["memory.forget".to_owned()],
            on_behalf_of: None,
        },
        mode: DeleteMode::Delete,
        reason: Some("sql test cleanup".to_owned()),
    };

    let result = block_on(service.forget(request)).expect("forget memory");

    assert_eq!(result.status, ForgetStatus::Deleted);
    assert!(
        block_on(service.get_memory(&response.record.id, &response.record.scope))
            .expect("get memory")
            .is_none()
    );
    let events =
        block_on(service.list_events_for_memory(&response.record.id, &response.record.scope))
            .expect("list events");
    assert_eq!(events.len(), 2);
}

#[test]
fn sql_service_runs_evaluation_fixture() {
    let service = Arc::new(SqlMemoryService::open_in_memory().expect("open sql service"));
    let runner = MemoryFixtureRunner::new(service);
    let fixture: EvaluationFixture = serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/evaluation-fixture.json"
    ))
    .expect("deserialize evaluation fixture");

    let report = block_on(runner.run_fixture(fixture)).expect("run fixture");

    assert!(report.cases[0].passed, "{:?}", report.cases[0].failures);
}
