//! Evaluation gate helpers for mutating consolidation.
//!
//! This module interprets protected evaluation reports and converts failures
//! into consolidation task/error records. It does not run fixtures or execute
//! mutation work.

use engram_domain::{
    ConsolidationError, ConsolidationTaskKind, ConsolidationTaskResult, ConsolidationTaskStatus,
    Timestamp,
};

use crate::EvaluationReport;

pub(crate) const EVALUATION_FAILED: &str = "evaluation_failed";

/// Converts one evaluation report into a consolidation task result.
///
/// The task is completed only when every case passes. Failed cases are copied
/// into task-local errors so the returned run can explain the regression.
pub(crate) fn evaluation_task(
    report: &EvaluationReport,
    timestamp: Timestamp,
) -> ConsolidationTaskResult {
    let errors = evaluation_errors(report);
    ConsolidationTaskResult {
        task: ConsolidationTaskKind::Evaluation,
        status: if errors.is_empty() {
            ConsolidationTaskStatus::Completed
        } else {
            ConsolidationTaskStatus::Failed
        },
        started_at: timestamp,
        completed_at: Some(timestamp),
        items_read: Some(report.cases.len() as u64),
        items_written: Some(0),
        items_updated: Some(0),
        items_skipped: Some(0),
        model_calls: Some(0),
        errors,
        output_refs: Vec::new(),
    }
}

/// Extracts recoverable consolidation errors from failed evaluation cases.
///
/// Passing cases are ignored. Each failed case becomes one error anchored to the
/// case ID so callers can identify the protected fixture behavior that regressed.
pub(crate) fn evaluation_errors(report: &EvaluationReport) -> Vec<ConsolidationError> {
    report
        .cases
        .iter()
        .filter(|case| !case.passed)
        .map(|case| ConsolidationError {
            task: Some(ConsolidationTaskKind::Evaluation),
            code: EVALUATION_FAILED.to_owned(),
            message: if case.failures.is_empty() {
                format!("evaluation case failed: {}", case.case_id)
            } else {
                format!(
                    "evaluation case failed: {}: {}",
                    case.case_id,
                    case.failures.join("; ")
                )
            },
            target_type: None,
            target_id: Some(case.case_id.clone()),
            recoverable: true,
        })
        .collect()
}

/// Converts an evaluation runner failure into consolidation error evidence.
///
/// Runner failures are recoverable gate failures rather than panics. The
/// mutating service uses this when the fixture could not be executed at all.
pub(crate) fn adapter_error(message: String) -> ConsolidationError {
    ConsolidationError {
        task: Some(ConsolidationTaskKind::Evaluation),
        code: "evaluation_runner_failed".to_owned(),
        message,
        target_type: None,
        target_id: None,
        recoverable: true,
    }
}
