//! Serializable summaries for executed evaluation reports.
//!
//! This module converts core `EvaluationReport` values into presentation-ready
//! data transfer objects. It owns only counting and failure preservation; it
//! does not execute fixtures, format terminal output, write files, or make
//! adapter-specific quality decisions.

use crate::EvaluationReport;
use serde::{Deserialize, Serialize};

/// Summary for a set of executed evaluation fixtures.
///
/// Use this shape when a CI job or future CLI needs one stable object that
/// includes aggregate pass/fail counts plus the per-fixture details needed to
/// diagnose regressions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureSetReportSummary {
    /// Number of fixture reports included in the summary.
    pub total_fixtures: usize,
    /// Number of fixtures where every case passed.
    pub passed_fixtures: usize,
    /// Number of fixtures with at least one failed case.
    pub failed_fixtures: usize,
    /// Number of cases across all fixture reports.
    pub total_cases: usize,
    /// Number of passing cases across all fixture reports.
    pub passed_cases: usize,
    /// Number of failing cases across all fixture reports.
    pub failed_cases: usize,
    /// Per-fixture summaries, preserving fixture IDs and case failures.
    pub fixtures: Vec<FixtureReportSummary>,
}

/// Summary for one executed evaluation fixture.
///
/// This keeps aggregate counts close to the fixture ID while still preserving
/// all case-level failures for consumers that need actionable output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureReportSummary {
    /// Identifier of the fixture that produced the report.
    pub fixture_id: String,
    /// True when every case in the fixture passed.
    pub passed: bool,
    /// Number of cases in the fixture report.
    pub total_cases: usize,
    /// Number of passing cases in the fixture report.
    pub passed_cases: usize,
    /// Number of failing cases in the fixture report.
    pub failed_cases: usize,
    /// Per-case summaries with failure messages preserved.
    pub cases: Vec<CaseReportSummary>,
}

/// Summary for one executed evaluation case.
///
/// Failure strings are intentionally copied verbatim from the core report so
/// downstream tooling can display the exact missing/forbidden/score reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseReportSummary {
    /// Identifier of the evaluated case.
    pub case_id: String,
    /// True when the evaluated case had no failures.
    pub passed: bool,
    /// Failure messages emitted by the fixture runner for this case.
    pub failures: Vec<String>,
}

/// Summarizes one executed evaluation report.
///
/// The helper is pure: it never re-runs a fixture and never mutates the input
/// report. It copies IDs and failures into serializable output.
pub fn summarize_report(report: &EvaluationReport) -> FixtureReportSummary {
    let cases = report
        .cases
        .iter()
        .map(|case| CaseReportSummary {
            case_id: case.case_id.clone(),
            passed: case.passed,
            failures: case.failures.clone(),
        })
        .collect::<Vec<_>>();
    let passed_cases = cases.iter().filter(|case| case.passed).count();
    let total_cases = cases.len();
    let failed_cases = total_cases - passed_cases;

    FixtureReportSummary {
        fixture_id: report.fixture_id.to_string(),
        passed: failed_cases == 0,
        total_cases,
        passed_cases,
        failed_cases,
        cases,
    }
}

/// Summarizes multiple executed evaluation reports as one fixture set.
///
/// This is the aggregation boundary for CI and future CLI consumers. It keeps
/// fixture ordering stable by preserving the caller-provided report order.
pub fn summarize_reports<'a>(
    reports: impl IntoIterator<Item = &'a EvaluationReport>,
) -> FixtureSetReportSummary {
    let fixtures = reports
        .into_iter()
        .map(summarize_report)
        .collect::<Vec<_>>();
    let total_fixtures = fixtures.len();
    let passed_fixtures = fixtures.iter().filter(|fixture| fixture.passed).count();
    let failed_fixtures = total_fixtures - passed_fixtures;
    let total_cases = fixtures.iter().map(|fixture| fixture.total_cases).sum();
    let passed_cases = fixtures.iter().map(|fixture| fixture.passed_cases).sum();
    let failed_cases = fixtures.iter().map(|fixture| fixture.failed_cases).sum();

    FixtureSetReportSummary {
        total_fixtures,
        passed_fixtures,
        failed_fixtures,
        total_cases,
        passed_cases,
        failed_cases,
        fixtures,
    }
}
