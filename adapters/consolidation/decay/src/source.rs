//! Narrow memory read/write ports for the decay executor.
//!
//! Mirrors the `ActiveMemorySource` / `BeliefSink` pattern from `engram-reflection`
//! — narrow injected traits so tests stub one method instead of the full
//! `MemoryService` / `MemoryRepository` surface.

use async_trait::async_trait;
use engram_domain::{MemoryId, MemoryStatus, Policy, Retention, Scope};
use engram_runtime::CoreResult;

/// A memory record relevant to decay (id + policy + retention + status).
#[derive(Clone, Debug)]
pub struct DecayCandidate {
    pub id: MemoryId,
    pub status: MemoryStatus,
    pub policy: Policy,
}

/// Read access to scoped active memories for decay evaluation + write access to
/// expire them. Production wiring adapts `MemoryRepository` / `MemoryService`.
#[async_trait]
pub trait DecayMemorySource: Send + Sync {
    /// Returns all in-scope memories (the executor filters active + expired-by-policy).
    async fn memories(&self, scope: &Scope) -> CoreResult<Vec<DecayCandidate>>;

    /// Marks a memory as `Expired`.
    async fn expire(&self, id: &MemoryId, scope: &Scope) -> CoreResult<()>;
}

impl DecayCandidate {
    /// Whether this record is active and past its policy expiry deadline.
    pub fn is_due(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.status == MemoryStatus::Active
            && self.policy.retention != Retention::LegalHold
            && self
                .policy
                .expires_at
                .map(|deadline| deadline <= now)
                .unwrap_or(false)
    }
}
