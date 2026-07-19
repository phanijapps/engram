use engram_core::EvaluationRunner;
use engram_domain::*;
use engram_eval::{MemoryContractRunner, MemoryFixtureRunner, accepted_examples};
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_store_sqlite::SqlMemoryService;
use futures::executor::block_on;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

fn write_fixture() -> WriteMemoryRequest {
    accepted_examples::write_memory_request().expect("deserialize write fixture")
}

fn role_actor() -> Actor {
    Actor {
        id: Id::from("role-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Role Agent".to_owned()),
        metadata: None,
    }
}

fn role_request(
    kind: MemoryKind,
    retention: Retention,
    session: Option<&str>,
    text: &str,
    key: &str,
) -> WriteMemoryRequest {
    let now = chrono::Utc::now();
    let actor = role_actor();
    WriteMemoryRequest {
        kind,
        content: MemoryContent {
            text: text.to_owned(),
            summary: Some(format!("summary for {key}")),
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope: Scope {
            tenant: "tenant-roles".to_owned(),
            subject: Some("subject-roles".to_owned()),
            workspace: Some("workspace-roles".to_owned()),
            session: session.map(str::to_owned),
            environment: Some("test".to_owned()),
        },
        requester: Requester {
            actor: actor.clone(),
            roles: vec!["maintainer".to_owned()],
            permissions: vec!["memory.write".to_owned(), "memory.retrieve".to_owned()],
            on_behalf_of: None,
        },
        provenance: Provenance {
            source: "role_sql_fixture".to_owned(),
            actor,
            observed_at: now,
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(0.95),
            method: Some("fixture".to_owned()),
        },
        policy: Policy {
            visibility: Visibility::Workspace,
            retention,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
            expires_at: None,
            delete_mode: Some(DeleteMode::Archive),
        },
        links: Vec::new(),
        idempotency_key: Some(key.to_owned()),
    }
}

fn requester(actor: Actor) -> Requester {
    Requester {
        actor,
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.retrieve".to_owned(), "memory.forget".to_owned()],
        on_behalf_of: None,
    }
}

fn role_retrieval_case(id: &str, query: &str, alias: &str) -> EvaluationCase {
    EvaluationCase {
        id: id.to_owned(),
        request: RetrievalRequest {
            query: query.to_owned(),
            scope: Scope {
                tenant: "tenant-roles".to_owned(),
                subject: Some("subject-roles".to_owned()),
                workspace: Some("workspace-roles".to_owned()),
                session: None,
                environment: Some("test".to_owned()),
            },
            requester: requester(role_actor()),
            modes: vec![RetrievalMode::Keyword],
            filters: None,
            cues: Vec::new(),
            limit: Some(1),
            budget: None,
            include_explanations: Some(true),
        },
        expect: EvaluationExpectation {
            must_include: vec![ExpectedTarget {
                target_type: RetrievalTargetType::Memory,
                target_id: alias.to_owned(),
            }],
            must_exclude: Vec::new(),
            min_score: Some(0.5),
            max_results: Some(1),
            requires_explanation: Some(true),
        },
    }
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
fn file_backed_service_seeds_ids_past_existing_rows_on_reopen() {
    // A reopened file-backed database must not regenerate IDs that collide with
    // rows a previous process already wrote. Without seeding, the second open
    // resets the counter and the event insert fails with a UNIQUE constraint
    // violation (memories survive via ON CONFLICT, memory_events does not).
    let dir = std::env::temp_dir().join(format!(
        "engram-reopen-id-test-{}-{}.db",
        std::process::id(),
        std::cell::Cell::new(0usize).get() // pin a per-test path
    ));
    let _ = std::fs::remove_file(&dir);

    let first_id = {
        let service = SqlMemoryService::open_file(&dir).expect("open file service");
        let mut request = write_fixture();
        request.idempotency_key = None; // force a genuine new write, not a dedupe
        let written = block_on(service.write_memory(request)).expect("first write");
        written.record.id
    };

    // Reopen the same database file and write again. This must not error.
    let service = SqlMemoryService::open_file(&dir).expect("reopen file service");
    let mut second_request = write_fixture();
    second_request.idempotency_key = None;
    second_request.content.text = "second distinct memory after reopen".to_owned();
    let second = block_on(service.write_memory(second_request)).expect("second write after reopen");

    assert_ne!(
        second.record.id, first_id,
        "reopened service must produce a fresh memory id, not collide"
    );

    let _ = std::fs::remove_file(&dir);
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
        "../../../contracts/v1/examples/evaluation-fixture.json"
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

#[test]
fn sql_service_round_trips_memory_roles_without_role_wire_field() {
    let service = SqlMemoryService::open_in_memory().expect("open sql service");
    let fixtures = [
        (
            MemoryRole::Working,
            role_request(
                MemoryKind::Observation,
                Retention::Session,
                Some("session-role"),
                "working role active context",
                "role-working",
            ),
        ),
        (
            MemoryRole::Episodic,
            role_request(
                MemoryKind::Episode,
                Retention::Durable,
                Some("session-role"),
                "episodic role remembered event",
                "role-episodic",
            ),
        ),
        (
            MemoryRole::Semantic,
            role_request(
                MemoryKind::Fact,
                Retention::Durable,
                None,
                "semantic role durable fact",
                "role-semantic",
            ),
        ),
        (
            MemoryRole::Procedural,
            role_request(
                MemoryKind::Procedure,
                Retention::Durable,
                None,
                "procedural role learned workflow",
                "role-procedural",
            ),
        ),
    ];

    for (expected_role, request) in fixtures {
        let response = block_on(service.write_memory(request.clone())).expect("write role memory");
        assert_eq!(response.record.role(), expected_role);
        assert_eq!(response.record.provenance.source, "role_sql_fixture");
        assert_eq!(response.record.policy, request.policy);
        assert!(
            serde_json::to_value(&response.record)
                .expect("serialize memory record")
                .get("role")
                .is_none()
        );

        let context = block_on(service.retrieve(RetrievalRequest {
            query: response.record.content.text.clone(),
            scope: response.record.scope.clone(),
            requester: requester(response.event.actor.clone()),
            modes: vec![RetrievalMode::Keyword],
            filters: Some(QueryFilter {
                memory_kinds: vec![response.record.kind.clone()],
                source_kinds: Vec::new(),
                chunk_kinds: Vec::new(),
                concept_ids: Vec::new(),
                entity_ids: Vec::new(),
                since: None,
                until: None,
                min_confidence: Some(0.9),
                include_archived: Some(false),
                as_of: None,
            }),
            cues: Vec::new(),
            limit: Some(1),
            budget: None,
            include_explanations: Some(true),
        }))
        .expect("retrieve role memory");
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].target_id, response.record.id.to_string());

        let forget = block_on(service.forget(ForgetRequest {
            target_type: ForgetTargetType::Memory,
            target_id: response.record.id.to_string(),
            scope: response.record.scope.clone(),
            requester: requester(response.event.actor),
            mode: DeleteMode::Archive,
            reason: Some("role fixture archive".to_owned()),
        }))
        .expect("archive role memory");
        assert_eq!(forget.status, ForgetStatus::Archived);

        let archived = block_on(service.get_memory(&response.record.id, &response.record.scope))
            .expect("get archived memory")
            .expect("archived memory remains stored");
        assert_eq!(archived.status, MemoryStatus::Archived);
        assert_eq!(archived.role(), expected_role);
    }
}

#[test]
fn sql_service_reports_role_eval_fixture_results() {
    let service = Arc::new(SqlMemoryService::open_in_memory().expect("open sql service"));
    let runner = MemoryFixtureRunner::new(service);
    let now = chrono::Utc::now();
    let fixture = EvaluationFixture {
        id: Id::from("eval-memory-roles"),
        name: "Memory role retrieval fixture".to_owned(),
        scope: Scope {
            tenant: "tenant-roles".to_owned(),
            subject: Some("subject-roles".to_owned()),
            workspace: Some("workspace-roles".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        },
        setup: EvaluationSetup {
            memories: vec![
                role_request(
                    MemoryKind::Observation,
                    Retention::Session,
                    Some("session-role"),
                    "eval working role active context",
                    "eval-role-working",
                ),
                role_request(
                    MemoryKind::Episode,
                    Retention::Durable,
                    Some("session-role"),
                    "eval episodic role remembered event",
                    "eval-role-episodic",
                ),
                role_request(
                    MemoryKind::Fact,
                    Retention::Durable,
                    None,
                    "eval semantic role durable fact",
                    "eval-role-semantic",
                ),
                role_request(
                    MemoryKind::Procedure,
                    Retention::Durable,
                    None,
                    "eval procedural role learned workflow",
                    "eval-role-procedural",
                ),
            ],
            sources: Vec::new(),
            documents: Vec::new(),
            chunks: Vec::new(),
        },
        cases: vec![
            role_retrieval_case(
                "working-role-recall",
                "eval working role active context",
                "memory-1",
            ),
            role_retrieval_case(
                "episodic-role-recall",
                "eval episodic role remembered event",
                "memory-2",
            ),
            role_retrieval_case(
                "semantic-role-recall",
                "eval semantic role durable fact",
                "memory-3",
            ),
            role_retrieval_case(
                "procedural-role-recall",
                "eval procedural role learned workflow",
                "memory-4",
            ),
        ],
        created_at: now,
    };

    let report = block_on(runner.run_fixture(fixture)).expect("run role fixture");

    assert_eq!(report.cases.len(), 4);
    for case in report.cases {
        assert!(case.passed, "{}: {:?}", case.case_id, case.failures);
    }
}

fn temp_database_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("{name}-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}
