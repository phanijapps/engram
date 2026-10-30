//! Durable read custody for published generation sets, independent of any backend.
use crate::CodeEligibility;
use async_trait::async_trait;
use engram_domain::{Scope, Timestamp};
use engram_runtime::CoreResult;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeReadLease {
    pub id: String,
    pub eligibility: CodeEligibility,
    pub fencing_token: u64,
    pub expires_at: Timestamp,
}
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeReadLeases: Send + Sync {
    async fn acquire_read_lease(
        &self,
        lease: CodeReadLease,
        now: Timestamp,
    ) -> CoreResult<CodeReadLease>;
    async fn renew_read_lease(
        &self,
        lease: &CodeReadLease,
        now: Timestamp,
        expires_at: Timestamp,
    ) -> CoreResult<CodeReadLease>;
    async fn release_read_lease(&self, lease: &CodeReadLease) -> CoreResult<()>;
    /// Protect references only; this is not permission to delete unreferenced data.
    async fn retained_generations(
        &self,
        scope: &Scope,
        now: Timestamp,
        grace_millis: u64,
    ) -> CoreResult<Vec<String>>;

    /// LEASE-ONLY coverage (corrective r2, finding 2): the generations
    /// covered by LIVE read leases (`expires_at > now`). Unlike
    /// [`Self::retained_generations`] this EXCLUDES deletion-protection
    /// sets — it answers exactly "which historical generations does an
    /// explicitly authorized pinned lease currently cover?".
    async fn leased_generations(&self, scope: &Scope, now: Timestamp) -> CoreResult<Vec<String>>;

    /// Validates a caller-presented historical read lease against its durable
    /// record.  This is intentionally not a generation union: the exact lease
    /// object must still be live, byte-equal to the durable record, and cover
    /// this scope, repository, and generation.
    async fn validate_historical_read_lease(
        &self,
        _lease: &CodeReadLease,
        _scope: &Scope,
        _repo_id: &str,
        _generation_id: &str,
        _now: Timestamp,
    ) -> CoreResult<bool> {
        Ok(false)
    }
}
