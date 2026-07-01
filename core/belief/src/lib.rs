//! Belief and contradiction ports for the engram engine.
//!
//! Belief and contradiction behavior contracts that adapters (in-memory
//! fixtures, the SQLite belief adapter, …) implement. Domain types live in
//! `engram-domain`; this crate owns only the ports.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

/// Persistence port for derived beliefs and contradiction records.
///
/// Beliefs should be recomputable from evidence or explicitly marked as manual.
/// Contradictions are review records; writing one must not silently mutate the
/// targets in conflict.
#[async_trait]
pub trait BeliefRepository: Send + Sync {
    /// Stores a derived or manually asserted belief.
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief>;

    /// Stores a reviewable contradiction between memories, beliefs, or knowledge.
    async fn put_contradiction(&self, contradiction: Contradiction) -> CoreResult<Contradiction>;

    /// Looks up a contradiction review record inside the supplied scope.
    async fn get_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
    ) -> CoreResult<Option<Contradiction>>;

    /// Applies an explicit reviewer resolution to a contradiction record.
    async fn resolve_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
        resolution: ContradictionResolution,
    ) -> CoreResult<Contradiction>;
}

/// Derives belief records from current evidence.
///
/// Synthesizers should keep evidence links intact and mark beliefs stale or
/// superseded rather than destructively rewriting unsupported conclusions.
#[async_trait]
pub trait BeliefSynthesizer: Send + Sync {
    /// Produces belief candidates for a consolidation request.
    async fn synthesize_beliefs(&self, request: &ConsolidationRequest) -> CoreResult<Vec<Belief>>;
}

/// Detects reviewable contradictions across beliefs and their evidence.
///
/// Detection is advisory. Implementations should create contradiction records
/// with severity and reasoning, leaving resolution to a later explicit step.
#[async_trait]
pub trait ContradictionDetector: Send + Sync {
    /// Returns contradictions found in the supplied belief set.
    async fn detect_contradictions(&self, beliefs: &[Belief]) -> CoreResult<Vec<Contradiction>>;
}
