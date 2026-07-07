use engram_eval::{
    ArchitectureEvalCapability, ArchitectureEvalCase, EvaluationCaseReport,
    required_architecture_capabilities, summarize_architecture_coverage,
};

#[test]
fn coverage_passes_when_every_required_architecture_bar_has_passing_cases() {
    let cases = required_architecture_capabilities()
        .into_iter()
        .enumerate()
        .map(|(index, capability)| ArchitectureEvalCase {
            case_id: format!("case-{index:02}"),
            capabilities: vec![capability],
            passed: true,
            failures: Vec::new(),
        })
        .collect::<Vec<_>>();

    let first =
        summarize_architecture_coverage(cases.clone(), &required_architecture_capabilities());
    let second = summarize_architecture_coverage(cases, &required_architecture_capabilities());

    assert!(first.passed());
    assert_eq!(first, second);
    assert_eq!(first.total_required, 11);
    assert_eq!(first.covered.len(), 11);
    assert!(first.missing.is_empty());
    assert!(first.failing.is_empty());
}

#[test]
fn coverage_reports_missing_required_capabilities() {
    let coverage = summarize_architecture_coverage(
        [ArchitectureEvalCase {
            case_id: "accepted-recall".to_owned(),
            capabilities: vec![ArchitectureEvalCapability::AcceptedRecall],
            passed: true,
            failures: Vec::new(),
        }],
        &required_architecture_capabilities(),
    );

    assert!(!coverage.passed());
    assert_eq!(
        coverage.covered,
        vec![ArchitectureEvalCapability::AcceptedRecall]
    );
    assert!(
        coverage
            .missing
            .contains(&ArchitectureEvalCapability::TaxonomyDrift)
    );
    assert!(
        coverage
            .missing
            .contains(&ArchitectureEvalCapability::AdapterReadiness)
    );
}

#[test]
fn coverage_reports_failing_capabilities_without_losing_case_failures() {
    let report = EvaluationCaseReport {
        case_id: "forbidden-recall".to_owned(),
        passed: false,
        failures: vec!["forbidden target returned memory:memory-1".to_owned()],
    };
    let case = ArchitectureEvalCase::from_case_report(
        &report,
        vec![
            ArchitectureEvalCapability::ForbiddenRecall,
            ArchitectureEvalCapability::Leakage,
        ],
    );

    let coverage = summarize_architecture_coverage(
        [case.clone()],
        &[
            ArchitectureEvalCapability::ForbiddenRecall,
            ArchitectureEvalCapability::Leakage,
        ],
    );

    assert!(!coverage.passed());
    assert_eq!(
        coverage.failing,
        vec![
            ArchitectureEvalCapability::ForbiddenRecall,
            ArchitectureEvalCapability::Leakage,
        ]
    );
    assert_eq!(
        case.failures,
        vec!["forbidden target returned memory:memory-1"]
    );
}
