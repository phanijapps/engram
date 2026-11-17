//! Append-only attributed outcomes; never a retrieval lane.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
#[async_trait]
impl engram_retrieval::OutcomeLedger for HelixStore {
    async fn report_outcome(
        &self,
        request: ReportOutcomeRequest,
    ) -> CoreResult<ReportOutcomeResult> {
        self.check_scope(&request.outcome.scope).map_err(core)?;
        let v = RecordedOutcome {
            id: Id::from(format!("outcome-{}", uuid::Uuid::new_v4())),
            outcome: request.outcome,
        };
        self.put(OUTCOME, &v).map_err(core)?;
        Ok(ReportOutcomeResult { recorded: v })
    }
    async fn list_outcomes(&self, window: &OutcomeWindow) -> CoreResult<Vec<RecordedOutcome>> {
        if window.since > window.until {
            return Err(CoreError::InvalidRequest {
                reason: "invalid outcome time window".into(),
            });
        }
        if !self.visible_scope(&window.scope) {
            return Ok(vec![]);
        }
        let mut rows = self.all::<RecordedOutcome>(OUTCOME).map_err(core)?;
        rows.retain(|v| {
            v.outcome.occurred_at >= window.since && v.outcome.occurred_at <= window.until
        });
        rows.sort_by(|a, b| {
            b.outcome
                .occurred_at
                .cmp(&a.outcome.occurred_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(rows)
    }
}
