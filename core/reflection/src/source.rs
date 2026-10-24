//! Narrow read port for scoped active memories (MCCL-audit F2: lineage).
//!
//! The consolidation crate deliberately has no repository dependency, so the
//! synthesizer injects this focused trait (mirrors `GraphRelationshipSource`
//! in the associative-graph adapter). The payload is TYPED EVIDENCE —
//! identity, policy, and provenance travel with the text — so derived beliefs
//! can cite their inputs (`Belief.sources`, `DerivationRef.input_refs`) and
//! inherit a never-widened policy. The previous `Vec<String>` shape destroyed
//! lineage: correction propagation was structurally impossible.

use async_trait::async_trait;
use engram_domain::{Belief, MemoryId, Policy, Scope, Timestamp};
use engram_runtime::CoreResult;

/// One active memory with the identity and governance needed for lineage.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveMemoryEvidence {
    pub memory_id: MemoryId,
    pub text: String,
    pub policy: Policy,
    /// The input's `provenance.observed_at` (attribution travels with the
    /// derivation; the derived record's own provenance is written at synthesis).
    pub observed_at: Timestamp,
}

/// Read access to a scope's active memories for reflection.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait ActiveMemorySource: Send + Sync {
    /// Returns all `MemoryStatus::Active` memories visible to `scope`, in
    /// stable order, WITH identity/policy/provenance evidence.
    async fn active_memories(&self, scope: &Scope) -> CoreResult<Vec<ActiveMemoryEvidence>>;
}

/// Write access for persisting reflection-derived beliefs (mirrors
/// `BeliefRepository::put_belief`). Narrow so tests stub one method instead of
/// 13; production wiring adapts `BeliefRepository` to this trait.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait BeliefSink: Send + Sync {
    /// Persists a derived belief, returning the stored record.
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief>;
}
