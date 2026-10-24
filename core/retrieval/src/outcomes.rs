//! Outcome-ledger port (F2): the host reports post-action results; engram
//! stores them attributed and non-causal. Never a retrieval lane.

use async_trait::async_trait;
use engram_domain::{OutcomeWindow, RecordedOutcome, ReportOutcomeRequest, ReportOutcomeResult};
use engram_runtime::CoreResult;

/// Storage-neutral port for the attributed outcome ledger.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait OutcomeLedger: Send + Sync {
    /// Appends one attributed outcome (append-only).
    async fn report_outcome(
        &self,
        request: ReportOutcomeRequest,
    ) -> CoreResult<ReportOutcomeResult>;

    /// Reads outcomes in a scope within a time window (most recent first).
    async fn list_outcomes(&self, window: &OutcomeWindow) -> CoreResult<Vec<RecordedOutcome>>;
}
