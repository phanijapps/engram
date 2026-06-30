use engram_core::EvaluationRunner;
use engram_domain::*;
use engram_eval::{MemoryContractRunner, MemoryFixtureRunner, accepted_examples};
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

fn write_fixture() -> WriteMemoryRequest {
    accepted_examples::write_memory_request().expect("deserialize write fixture")
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
    let runner = MemoryContractRunner::new(Arc::new(
        SqlMemoryService::open_in_memory().expect("open sql service"),
    ));

    let outcome = block_on(runner.retrieve_accepted_example()).expect("retrieve context");

    assert_eq!(outcome.context.items.len(), 1);
    assert!(outcome.context.items[0].content.contains("Rust 2024"));
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
        "../../../../contracts/v1/examples/evaluation-fixture.json"
    ))
    .expect("deserialize evaluation fixture");

    let report = block_on(runner.run_fixture(fixture)).expect("run fixture");

    assert!(report.cases[0].passed, "{:?}", report.cases[0].failures);
}

#[test]
fn sql_service_file_backed_store_persists_across_reopen() {
    let path = temp_database_path("engram-sql-service-reopen");
    let first = SqlMemoryService::open_file(&path).expect("open file-backed sql service");
    let response = block_on(first.write_memory(write_fixture())).expect("write memory");
    drop(first);

    let second = SqlMemoryService::open_file(&path).expect("reopen file-backed sql service");
    let fetched = block_on(second.get_memory(&response.record.id, &response.record.scope))
        .expect("get memory")
        .expect("persisted memory");

    assert_eq!(fetched.content.text, response.record.content.text);
    let _ = std::fs::remove_file(path);
}

fn temp_database_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("{name}-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}
