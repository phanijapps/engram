use engram_domain::Id;
use engram_eval::{EvaluationCaseReport, EvaluationReport};
use engram_eval::{summarize_report, summarize_reports};

#[test]
fn summarizes_passing_report_counts() {
    let report = EvaluationReport {
        fixture_id: Id::from("fixture-pass"),
        cases: vec![
            EvaluationCaseReport {
                case_id: "case-a".to_owned(),
                passed: true,
                failures: Vec::new(),
            },
            EvaluationCaseReport {
                case_id: "case-b".to_owned(),
                passed: true,
                failures: Vec::new(),
            },
        ],
    };

    let summary = summarize_report(&report);

    assert!(summary.passed);
    assert_eq!(summary.fixture_id, "fixture-pass");
    assert_eq!(summary.total_cases, 2);
    assert_eq!(summary.passed_cases, 2);
    assert_eq!(summary.failed_cases, 0);
}

#[test]
fn summarizes_failing_report_without_losing_failure_details() {
    let report = EvaluationReport {
        fixture_id: Id::from("fixture-fail"),
        cases: vec![EvaluationCaseReport {
            case_id: "case-leak".to_owned(),
            passed: false,
            failures: vec!["forbidden target returned memory:memory-002".to_owned()],
        }],
    };

    let summary = summarize_report(&report);

    assert!(!summary.passed);
    assert_eq!(summary.failed_cases, 1);
    assert_eq!(summary.cases[0].case_id, "case-leak");
    assert_eq!(
        summary.cases[0].failures,
        vec!["forbidden target returned memory:memory-002"]
    );
}

#[test]
fn summarizes_report_set_totals() {
    let passing = EvaluationReport {
        fixture_id: Id::from("fixture-pass"),
        cases: vec![EvaluationCaseReport {
            case_id: "case-pass".to_owned(),
            passed: true,
            failures: Vec::new(),
        }],
    };
    let failing = EvaluationReport {
        fixture_id: Id::from("fixture-fail"),
        cases: vec![EvaluationCaseReport {
            case_id: "case-fail".to_owned(),
            passed: false,
            failures: vec!["missing required target memory:memory-001".to_owned()],
        }],
    };

    let summary = summarize_reports([&passing, &failing]);

    assert_eq!(summary.total_fixtures, 2);
    assert_eq!(summary.passed_fixtures, 1);
    assert_eq!(summary.failed_fixtures, 1);
    assert_eq!(summary.total_cases, 2);
    assert_eq!(summary.passed_cases, 1);
    assert_eq!(summary.failed_cases, 1);
}
