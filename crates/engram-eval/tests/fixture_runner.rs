use std::sync::Arc;

use engram_core::EvaluationRunner;
use engram_domain::*;
use engram_eval::MemoryFixtureRunner;
use engram_store_memory::InMemoryMemoryService;
use futures::executor::block_on;

const ACCEPTED_RETRIEVAL_FIXTURES: &[&str] = &[
    include_str!("../../../contracts/v1/examples/evaluation-fixture.positive-recall.json"),
    include_str!("../../../contracts/v1/examples/evaluation-fixture.forbidden-recall.json"),
    include_str!("../../../contracts/v1/examples/evaluation-fixture.budget-omission.json"),
    include_str!("../../../contracts/v1/examples/evaluation-fixture.no-result.json"),
];

#[test]
fn runner_passes_contract_evaluation_fixture() {
    let service = Arc::new(InMemoryMemoryService::new());
    let runner = MemoryFixtureRunner::new(service);
    let fixture: EvaluationFixture = serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/evaluation-fixture.json"
    ))
    .expect("deserialize evaluation fixture");

    let report = block_on(runner.run_fixture(fixture)).expect("run fixture");

    assert_eq!(report.fixture_id.as_str(), "eval-contract-001");
    assert_eq!(report.cases.len(), 1);
    assert!(report.cases[0].passed, "{:?}", report.cases[0].failures);
}

#[test]
fn runner_passes_accepted_retrieval_fixtures() {
    for fixture_json in ACCEPTED_RETRIEVAL_FIXTURES {
        let service = Arc::new(InMemoryMemoryService::new());
        let runner = MemoryFixtureRunner::new(service);
        let fixture: EvaluationFixture =
            serde_json::from_str(fixture_json).expect("deserialize accepted retrieval fixture");
        let fixture_id = fixture.id.to_string();

        let report = block_on(runner.run_fixture(fixture)).expect("run retrieval fixture");

        assert_eq!(report.cases.len(), 1, "{fixture_id}");
        assert!(
            report.cases.iter().all(|case| case.passed),
            "{fixture_id}: {:?}",
            report.cases
        );
    }
}

#[test]
fn runner_reports_forbidden_target_leak() {
    let service = Arc::new(InMemoryMemoryService::new());
    let runner = MemoryFixtureRunner::new(service);
    let mut fixture: EvaluationFixture = serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/evaluation-fixture.json"
    ))
    .expect("deserialize evaluation fixture");
    fixture.cases[0].expect.must_exclude = fixture.cases[0].expect.must_include.clone();

    let report = block_on(runner.run_fixture(fixture)).expect("run fixture");

    assert!(!report.cases[0].passed);
    assert!(
        report.cases[0]
            .failures
            .iter()
            .any(|failure| failure.contains("forbidden target returned"))
    );
}
