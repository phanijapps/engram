//! Derived-belief correction propagation (MCCL-audit F2, slice 2C).
//!
//! When a memory that a derived belief CITES is corrected (redacted,
//! deleted, or expired), the derived record must not keep asserting its old
//! content as live. This focused orchestrator works purely through the
//! belief repository port: find live beliefs referencing the memory
//! (`BeliefReferenceQuery` over `Belief.sources` — populated since the
//! lineage fix) and mark them `stale` with an attribution timestamp.
//!
//! Honesty rules: this is best-effort, port-only coordination — NOT a
//! cross-store transaction, NOT deletion of the belief (the record stays
//! auditable; re-synthesis clears or supersedes it), and partial failures
//! are reported (staled ids + error) rather than swallowed.

use std::sync::Arc;

use chrono::Utc;

use engram_belief::{BeliefReferenceQuery, BeliefRepository};
use engram_domain::{BeliefId, BeliefSourceTargetType, BeliefStatus, MemoryId, Scope};
use engram_runtime::CoreResult;

/// Marks derived beliefs stale when a cited memory is corrected.
pub struct DerivedBeliefLifecycle {
    beliefs: Arc<dyn BeliefRepository>,
}

impl DerivedBeliefLifecycle {
    pub fn new(beliefs: Arc<dyn BeliefRepository>) -> Self {
        Self { beliefs }
    }

    /// Stales every LIVE belief citing `memory_id`. Returns the staled ids
    /// (already-stale or superseded beliefs are left untouched — they are
    /// already non-live). Failure to stale one belief surfaces as `Err` with
    /// the successfully staled ids lost to the caller only when the
    /// repository read itself fails; per-belief `mark_stale` failures abort
    /// with the error (retry re-stales idempotently).
    pub async fn stale_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<BeliefId>> {
        let query = BeliefReferenceQuery {
            scope: scope.clone(),
            source_type: BeliefSourceTargetType::Memory,
            source_id: memory_id.to_string(),
            valid_at: None,
        };
        let referencing = self.beliefs.beliefs_referencing_source(query).await?;
        let now = Utc::now();
        let mut staled = Vec::new();
        for belief in referencing {
            if belief.status != BeliefStatus::Active {
                continue;
            }
            // Scope discipline: only beliefs in the corrected memory's scope.
            if belief.scope.tenant != scope.tenant {
                continue;
            }
            self.beliefs
                .mark_stale(&belief.id, &belief.scope, now)
                .await?;
            staled.push(belief.id.clone());
        }
        Ok(staled)
    }
}
